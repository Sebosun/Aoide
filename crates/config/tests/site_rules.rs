use aoide_config::{
    site_host, Fallback, SiteData, SiteFetch, SiteRule, SiteRules, SITE_RULES_FILE,
};

const RULES: &str = r#"
# Hand-written rules.
["example.com"]
fetch = "browser"
block = ["*/ads/*", "*.doubleclick.net/*"]
wait_for = "article .body"
fallbacks = ["archive"]
note = "kept, though Aoide does not know it"

["news.example.com"]
fetch = "http"
site_data = "clear_before_fetch"
user_agent = "Mozilla/5.0 (Reader)"
"#;

#[test]
fn rules_are_read_from_the_file_with_every_field() {
    let rules = SiteRules::parse(RULES).unwrap();
    let (host, rule) = rules.rule_for("example.com").unwrap();
    assert_eq!(host, "example.com");
    assert_eq!(
        rule,
        SiteRule {
            fetch: Some(SiteFetch::Browser),
            site_data: SiteData::Keep,
            block: vec!["*/ads/*".into(), "*.doubleclick.net/*".into()],
            wait_for: Some("article .body".into()),
            user_agent: None,
            fallbacks: vec![Fallback::Archive],
        }
    );
    let (_, news) = rules.rule_for("news.example.com").unwrap();
    assert_eq!(news.site_data, SiteData::ClearBeforeFetch);
    assert_eq!(news.user_agent.as_deref(), Some("Mozilla/5.0 (Reader)"));
    assert!(SiteRules::parse("[\"a.example\"]\nfetch = \"carrier pigeon\"").is_err());
    assert!(SiteRules::parse("not toml at all [").is_err());
}

#[test]
fn a_rule_covers_its_subdomains_unless_a_closer_one_exists() {
    let rules = SiteRules::parse(RULES).unwrap();
    let host = |name: &str| rules.rule_for(name).map(|(host, _)| host.to_owned());
    assert_eq!(host("www.example.com").as_deref(), Some("example.com"));
    assert_eq!(host("blog.example.com").as_deref(), Some("example.com"));
    assert_eq!(host("EXAMPLE.COM.").as_deref(), Some("example.com"));
    assert_eq!(
        host("live.news.example.com").as_deref(),
        Some("news.example.com")
    );
    assert_eq!(host("notexample.com"), None);
    assert_eq!(host("example.org"), None);
    assert_eq!(site_host("WWW.Example.com."), "example.com");
}

#[test]
fn settings_edit_fetch_and_site_data_and_keep_everything_else() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(SITE_RULES_FILE);
    std::fs::write(&path, RULES).unwrap();
    let mut rules = SiteRules::load(&path).unwrap();
    rules.set("www.example.com", None, SiteData::ClearBeforeFetch);
    rules.set("quiet.example.net", Some(SiteFetch::Http), SiteData::Keep);
    rules.save(&path).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("note = \"kept, though Aoide does not know it\""),
        "{text}"
    );
    let again = SiteRules::load(&path).unwrap();
    let (_, example) = again.rule_for("example.com").unwrap();
    assert_eq!(example.fetch, None, "back to the global choice");
    assert_eq!(example.site_data, SiteData::ClearBeforeFetch);
    assert_eq!(example.wait_for.as_deref(), Some("article .body"));
    assert_eq!(example.fallbacks, [Fallback::Archive]);
    let (_, quiet) = again.rule_for("quiet.example.net").unwrap();
    assert_eq!(quiet.fetch, Some(SiteFetch::Http));
    assert_eq!(
        again.hosts(),
        ["example.com", "news.example.com", "quiet.example.net"]
    );

    let mut fewer = again;
    fewer.remove("news.example.com");
    fewer.save(&path).unwrap();
    let (host, _) = SiteRules::load(&path)
        .unwrap()
        .rule_for("news.example.com")
        .map(|(h, r)| (h.to_owned(), r))
        .unwrap();
    assert_eq!(host, "example.com", "its parent's rule covers it again");
}

#[test]
fn a_missing_file_has_no_rules_and_a_changed_one_is_read_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(SITE_RULES_FILE);
    let mut rules = SiteRules::load(&path).unwrap();
    assert!(rules.hosts().is_empty());
    assert!(
        !rules.reload(&path).unwrap(),
        "still no file: nothing changed"
    );

    std::fs::write(&path, RULES).unwrap();
    assert!(rules.reload(&path).unwrap());
    assert_eq!(rules.hosts().len(), 2);
    assert!(!rules.reload(&path).unwrap(), "the same text again");

    // A broken edit is reported and the rules read before stay in use.
    std::fs::write(&path, "[broken").unwrap();
    assert!(rules.reload(&path).is_err());
    assert_eq!(rules.hosts().len(), 2);
}
