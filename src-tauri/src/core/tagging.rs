//! Tags from a local language model, when the user turns it on (Settings › Library): an item's
//! title, authors and description (or the start of its text) go to a model on this computer
//! (Ollama, llama.cpp, LM Studio; see `aoide_classifier`), and up to five tags come back, the
//! library's own preferred, kept as the classifier's (one the user removed stays removed).
//!
//! The user asks for one item's tags, or for every untagged item's. One run at a time.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use super::*;

/// Which model tags, and whether one does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TaggingSettings {
    /// Off until the user turns it on.
    pub enabled: bool,
    /// The model server's origin (Ollama listens on 127.0.0.1:11434).
    pub base_url: String,
    /// The model to ask, as the server names it.
    pub model: String,
    pub network: NetworkScope,
}

impl Default for TaggingSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "http://127.0.0.1:11434".into(),
            model: String::new(),
            network: NetworkScope::Loopback,
        }
    }
}

/// How a run over the untagged items went.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TaggingRun {
    /// Items that were given tags.
    pub tagged: u32,
    /// Items the model gave nothing for, or that failed.
    pub missed: u32,
}

/// What tagging needs of the rest of the app.
pub(super) trait TaggingHost: Send + Sync {
    /// What the model is told of item `id`.
    fn item_for_tagging(&self, id: &str) -> Result<aoide_classifier::Item, String>;
    /// Every tag on some item.
    fn known_tags(&self) -> Result<Vec<String>, String>;
    /// Items with no tags (not those that could not be read).
    fn untagged(&self) -> Result<Vec<String>, String>;
    /// Keeps the model's tags for item `id`.
    fn add_guessed_tags(&self, id: &str, tags: &[String]) -> Result<(), String>;
}

#[derive(Default)]
pub(super) struct Tagging {
    /// A run is under way.
    busy: AtomicBool,
}

/// Clears `busy` when a run ends, however it ends.
struct Running<'a>(&'a AtomicBool);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Tagging {
    fn start(&self) -> Result<Running<'_>, String> {
        if self.busy.swap(true, Ordering::SeqCst) {
            return Err("Aoide is already tagging".into());
        }
        Ok(Running(&self.busy))
    }

    fn classifier(settings: &TaggingSettings) -> Result<aoide_classifier::Classifier, String> {
        if !settings.enabled {
            return Err("Turn on tagging with a model in Settings › Library first".into());
        }
        aoide_classifier::Classifier::new(aoide_classifier::Options {
            base_url: settings.base_url.clone(),
            model: settings.model.clone(),
            network: settings.network,
            ..Default::default()
        })
        .map_err(|e| e.to_string())
    }

    /// Asks the model for item `id`'s tags and keeps them; returns them.
    pub async fn tag(
        &self,
        host: &dyn TaggingHost,
        settings: &TaggingSettings,
        id: &str,
    ) -> Result<Vec<String>, String> {
        let classifier = Self::classifier(settings)?;
        let _running = self.start()?;
        let known = host.known_tags()?;
        Self::tag_one(&classifier, host, &known, id).await
    }

    /// Tags every item that has none, one after another; an item that fails is counted and
    /// passed over.
    pub async fn tag_untagged(
        &self,
        host: &dyn TaggingHost,
        settings: &TaggingSettings,
    ) -> Result<TaggingRun, String> {
        let classifier = Self::classifier(settings)?;
        let _running = self.start()?;
        let started = Instant::now();
        let mut known = host.known_tags()?;
        let mut run = TaggingRun::default();
        for id in host.untagged()? {
            match Self::tag_one(&classifier, host, &known, &id).await {
                Ok(tags) if !tags.is_empty() => {
                    run.tagged += 1;
                    for tag in tags {
                        if !known.iter().any(|k| k.eq_ignore_ascii_case(&tag)) {
                            known.push(tag);
                        }
                    }
                }
                _ => run.missed += 1,
            }
        }
        tracing::info!(
            event = "tags.classify_all",
            outcome = "ok",
            tagged = run.tagged,
            missed = run.missed,
            took_ms = crate::logging::took(started),
            "Tagged the untagged items"
        );
        Ok(run)
    }

    async fn tag_one(
        classifier: &aoide_classifier::Classifier,
        host: &dyn TaggingHost,
        known: &[String],
        id: &str,
    ) -> Result<Vec<String>, String> {
        let started = Instant::now();
        let item = host.item_for_tagging(id)?;
        let result = classifier.suggest(&item, known).await;
        match &result {
            Ok(tags) => tracing::info!(
                event = "tags.classify",
                book = %crate::logging::book(id),
                outcome = "ok",
                tags = tags.len(),
                took_ms = crate::logging::took(started),
                "The model tagged an item"
            ),
            Err(error) => tracing::warn!(
                event = "tags.classify",
                book = %crate::logging::book(id),
                outcome = "error",
                %error,
                took_ms = crate::logging::took(started),
                "The model could not tag an item"
            ),
        }
        let tags = result.map_err(|e| format!("{}: {e}", e.summary()))?;
        host.add_guessed_tags(id, &tags)?;
        Ok(tags)
    }
}

impl TaggingHost for Core {
    fn item_for_tagging(&self, id: &str) -> Result<aoide_classifier::Item, String> {
        let record = self
            .shown_record(id)
            .ok_or_else(|| format!("no item {id}"))?;
        let description = self.book_details(id).ok().and_then(|d| d.description);
        // Without a description, the start of its text (an audiobook has none to give).
        let excerpt = if description.is_none() && record.kind != aoide_library::ItemKind::Audiobook
        {
            self.load_book(id).ok().map(|book| {
                book.chapters
                    .iter()
                    .flat_map(|c| c.text.split_whitespace())
                    .take(1_500)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        } else {
            None
        };
        Ok(aoide_classifier::Item {
            title: record.title.unwrap_or_default(),
            authors: record.authors,
            description,
            excerpt,
        })
    }

    fn known_tags(&self) -> Result<Vec<String>, String> {
        Ok(self.tags()?.into_iter().map(|t| t.name).collect())
    }

    fn untagged(&self) -> Result<Vec<String>, String> {
        let library = lock(&self.library);
        let tagged = library.book_tags().map_err(|e| e.to_string())?;
        Ok(library
            .books()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|b| !matches!(b.status, aoide_library::BookStatus::Error(_)))
            .filter(|b| !tagged.contains_key(&b.id))
            .map(|b| b.id.0)
            .collect())
    }

    fn add_guessed_tags(&self, id: &str, tags: &[String]) -> Result<(), String> {
        lock(&self.library)
            .add_tags(
                &BookId(id.into()),
                tags,
                aoide_library::TagSource::Classifier,
            )
            .map_err(|e| e.to_string())?;
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }
}

impl Core {
    /// Asks the tagging model for item `id`'s tags, keeps them and returns them.
    pub async fn tag_with_model(&self, id: &str) -> Result<Vec<String>, String> {
        let settings = self.app_settings().tagging;
        self.tagging.tag(self, &settings, id).await
    }

    /// Asks the tagging model for the tags of every item that has none.
    pub async fn tag_untagged_with_model(&self) -> Result<TaggingRun, String> {
        let settings = self.app_settings().tagging;
        self.tagging.tag_untagged(self, &settings).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    #[derive(Default)]
    struct Host {
        added: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl TaggingHost for Host {
        fn item_for_tagging(&self, id: &str) -> Result<aoide_classifier::Item, String> {
            if id == "unreadable" {
                return Err("no such item".into());
            }
            Ok(aoide_classifier::Item {
                title: format!("Item {id}"),
                ..Default::default()
            })
        }
        fn known_tags(&self) -> Result<Vec<String>, String> {
            Ok(vec!["Sea Stories".into()])
        }
        fn untagged(&self) -> Result<Vec<String>, String> {
            Ok(vec!["a".into(), "unreadable".into(), "b".into()])
        }
        fn add_guessed_tags(&self, id: &str, tags: &[String]) -> Result<(), String> {
            self.added.lock().unwrap().push((id.into(), tags.to_vec()));
            Ok(())
        }
    }

    /// A model server that answers every request with `content`, counting them.
    async fn model(content: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut buffer = [0u8; 8192];
                    loop {
                        let n = socket.read(&mut buffer).await.unwrap_or(0);
                        request.extend_from_slice(&buffer[..n]);
                        let text = String::from_utf8_lossy(&request).to_string();
                        let complete = text.find("\r\n\r\n").is_some_and(|end| {
                            let length: usize = text
                                .lines()
                                .find_map(|l| {
                                    l.to_lowercase()
                                        .strip_prefix("content-length:")
                                        .map(|v| v.trim().parse().unwrap())
                                })
                                .unwrap_or(0);
                            request.len() >= end + 4 + length
                        });
                        if complete || n == 0 {
                            break;
                        }
                    }
                    let body = serde_json::json!({
                        "choices": [{ "message": { "role": "assistant", "content": content } }]
                    })
                    .to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        });
        address
    }

    fn settings(base_url: String) -> TaggingSettings {
        TaggingSettings {
            enabled: true,
            base_url,
            model: "small".into(),
            network: NetworkScope::Loopback,
        }
    }

    #[tokio::test]
    async fn nothing_is_asked_until_tagging_is_turned_on() {
        let host = Host::default();
        let off = TaggingSettings::default();
        let error = Tagging::default().tag(&host, &off, "a").await.unwrap_err();
        assert!(error.contains("Settings"), "{error}");
        assert!(host.added.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_items_tags_come_from_the_model_in_the_librarys_casing() {
        let host = Host::default();
        let address = model(r#"["sea stories", "Lighthouses"]"#).await;
        let tags = Tagging::default()
            .tag(&host, &settings(address), "a")
            .await
            .unwrap();
        assert_eq!(tags, ["Sea Stories", "Lighthouses"]);
        assert_eq!(
            *host.added.lock().unwrap(),
            [(
                "a".to_string(),
                vec!["Sea Stories".to_string(), "Lighthouses".to_string()]
            )]
        );
    }

    #[tokio::test]
    async fn every_untagged_item_is_tagged_and_one_that_fails_is_passed_over() {
        let host = Host::default();
        let address = model(r#"["Mystery"]"#).await;
        let run = Tagging::default()
            .tag_untagged(&host, &settings(address))
            .await
            .unwrap();
        assert_eq!(
            run,
            TaggingRun {
                tagged: 2,
                missed: 1
            }
        );
        let added: Vec<String> = host
            .added
            .lock()
            .unwrap()
            .iter()
            .map(|(id, _)| id.clone())
            .collect();
        assert_eq!(added, ["a", "b"]);
    }
}
