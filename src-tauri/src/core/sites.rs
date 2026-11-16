//! Settings › Sites: each site that needed the user or has a rule, whether the user is signed
//! in to it, and the parts of its rule Settings edits. The rows are the library's `sites`
//! table; the rules are `site-rules.toml`, owned by [`super::browsing::Browsing`].

use aoide_config::{site_host, SiteData, SiteFetch, SiteRule};
use aoide_library::SiteRow;
use serde::Serialize;

use std::sync::Mutex;

use aoide_library::Library;

use super::fetching::Reason;
use super::lock;

// ---------------------------------------------------------------- the sites table
// Every write to the library's `sites` table is one of these.

/// `site` asked for the user (`reason`): it is listed in Settings › Sites with what it asked.
pub(super) fn asked(library: &Mutex<Library>, site: &str, reason: Reason) {
    let noted = lock(library).site_wall(site, reason.name(), super::now_ms());
    if let Err(error) = noted {
        tracing::warn!(event = "library.save", %error, "Could not note the site's wall");
    }
}

/// The user signed in to `site`, or passed its check (`reason`), in the sign-in window.
pub(super) fn kept(library: &Mutex<Library>, site: &str, reason: Option<Reason>) {
    let library = lock(library);
    let kept = match reason {
        Some(Reason::Check) => library.site_check_passed(site, super::now_ms()),
        _ => library.site_signed_in(site, super::now_ms()),
    };
    if let Err(error) = kept {
        tracing::warn!(event = "library.save", %error, "Could not note the site's sign-in");
    }
}

/// Where a site's session stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum SiteSession {
    SignedIn {
        at: i64,
    },
    CheckPassed {
        at: i64,
    },
    /// It last needed the user for `reason`, and nothing was passed since.
    AskedFor {
        reason: Reason,
        at: i64,
    },
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SiteView {
    pub host: String,
    pub session: SiteSession,
    /// `None`: follows the global choice.
    pub fetch: Option<SiteFetch>,
    pub site_data: SiteData,
    /// An article from this site waiting for the user, to sign in with.
    pub waiting_article: Option<String>,
}

/// What a site's row says of its session: a sign-in or passed check (the later one), else the
/// last thing it asked for.
fn session(row: Option<&SiteRow>) -> SiteSession {
    let Some(row) = row else {
        return SiteSession::None;
    };
    match (row.signed_in_at, row.check_passed_at) {
        (Some(signed), Some(passed)) if passed > signed => SiteSession::CheckPassed { at: passed },
        (Some(at), _) => SiteSession::SignedIn { at },
        (None, Some(at)) => SiteSession::CheckPassed { at },
        (None, None) => {
            let reason = row.last_wall.as_deref().and_then(Reason::from_name);
            match (reason, row.last_wall_at) {
                (Some(reason), Some(at)) => SiteSession::AskedFor { reason, at },
                _ => SiteSession::None,
            }
        }
    }
}

/// Every site with a row or a rule, by host.
pub(super) fn site_views(
    rows: &[SiteRow],
    rule_hosts: &[String],
    rule_for: impl Fn(&str) -> Option<SiteRule>,
    waiting: &[(String, String)],
) -> Vec<SiteView> {
    let mut hosts: Vec<&str> = rows.iter().map(|r| r.host.as_str()).collect();
    hosts.extend(rule_hosts.iter().map(String::as_str));
    hosts.sort_unstable();
    hosts.dedup();
    hosts
        .into_iter()
        .map(|host| {
            let rule = rule_for(host).unwrap_or_default();
            SiteView {
                host: host.to_owned(),
                session: session(rows.iter().find(|r| r.host == host)),
                fetch: rule.fetch,
                site_data: rule.site_data,
                waiting_article: waiting
                    .iter()
                    .find(|(site, _)| site == host)
                    .map(|(_, id)| id.clone()),
            }
        })
        .collect()
}

impl super::Core {
    pub fn sites(&self) -> Result<Vec<SiteView>, String> {
        let (rows, articles) = {
            let library = lock(&self.library);
            let rows = library.sites().map_err(|e| e.to_string())?;
            (rows, library.articles().map_err(|e| e.to_string())?)
        };
        // Articles waiting for the user, by site.
        let waiting: Vec<(String, String)> = articles
            .into_iter()
            .filter(|a| a.stage == "needs_you")
            .filter_map(|a| Some((super::articles::site_of(&a.canonical_url)?, a.book_id.0)))
            .collect();
        let browsing = &self.browsing;
        Ok(site_views(
            &rows,
            &browsing.rule_hosts(),
            |host| browsing.rule_for(host),
            &waiting,
        ))
    }

    /// Sets how `host` is fetched and whether its data is kept, in `site-rules.toml`.
    pub fn set_site_rule(
        &self,
        host: &str,
        fetch: Option<SiteFetch>,
        site_data: SiteData,
    ) -> Result<(), String> {
        let host = site_host(host);
        if host.is_empty() {
            return Err("No site was named.".into());
        }
        self.browsing.set_rule(&host, fetch, site_data)?;
        lock(&self.library)
            .add_site(&host)
            .map_err(|e| e.to_string())?;
        tracing::info!(event = "site.rule", "Changed a site's rule");
        Ok(())
    }

    /// Clears `host`'s cookies and storage in the browser, signing the user out of it.
    pub async fn sign_out_site(&self, host: &str) -> Result<(), String> {
        let host = site_host(host);
        self.browsing.clear_site(self, &host).await?;
        lock(&self.library)
            .sign_out_site(&host)
            .map_err(|e| e.to_string())?;
        tracing::info!(event = "site.sign_out", "Signed out of a site");
        Ok(())
    }

    /// Signs out of `host` and drops its row and its rule.
    pub async fn forget_site(&self, host: &str) -> Result<(), String> {
        let host = site_host(host);
        self.browsing.clear_site(self, &host).await?;
        self.browsing.forget_rule(&host)?;
        lock(&self.library)
            .forget_site(&host)
            .map_err(|e| e.to_string())?;
        tracing::info!(event = "site.forget", "Forgot a site");
        Ok(())
    }

    /// Removes the browser's whole profile: every sign-in and all site data.
    pub async fn clear_browser_data(&self) -> Result<(), String> {
        self.browsing.clear_profile().await?;
        lock(&self.library)
            .sign_out_sites()
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(host: &str) -> SiteRow {
        SiteRow {
            host: host.into(),
            signed_in_at: None,
            check_passed_at: None,
            last_wall: None,
            last_wall_at: None,
        }
    }

    #[test]
    fn sites_join_rows_and_rules_with_the_latest_session() {
        let rows = [
            SiteRow {
                last_wall: Some("paywall".into()),
                last_wall_at: Some(5),
                ..row("quay.example")
            },
            SiteRow {
                signed_in_at: Some(10),
                check_passed_at: Some(20),
                last_wall: Some("check".into()),
                last_wall_at: Some(15),
                ..row("ferry.example")
            },
            SiteRow {
                signed_in_at: Some(30),
                ..row("anchor.example")
            },
        ];
        let browser = SiteRule {
            fetch: Some(SiteFetch::Browser),
            site_data: SiteData::ClearBeforeFetch,
            ..SiteRule::default()
        };
        let views = site_views(
            &rows,
            &["lantern.example".into(), "quay.example".into()],
            |host| (host == "lantern.example").then(|| browser.clone()),
            &[("quay.example".into(), "article-1".into())],
        );
        let summary: Vec<_> = views
            .iter()
            .map(|v| (v.host.as_str(), v.session.clone(), v.fetch))
            .collect();
        assert_eq!(
            summary,
            [
                ("anchor.example", SiteSession::SignedIn { at: 30 }, None),
                ("ferry.example", SiteSession::CheckPassed { at: 20 }, None),
                (
                    "lantern.example",
                    SiteSession::None,
                    Some(SiteFetch::Browser)
                ),
                (
                    "quay.example",
                    SiteSession::AskedFor {
                        reason: Reason::Paywall,
                        at: 5
                    },
                    None
                ),
            ]
        );
        assert_eq!(views[2].site_data, SiteData::ClearBeforeFetch);
        assert_eq!(views[3].waiting_article.as_deref(), Some("article-1"));
        assert_eq!(views[0].waiting_article, None);
    }
}
