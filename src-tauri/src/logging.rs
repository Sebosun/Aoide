//! Aoide's log: one flat JSON object per line in a daily file, so it can be searched and
//! filtered with `rg` and `jq` (see docs/logging.md). Library crates emit `tracing` events; this
//! module owns the subscriber.

use std::borrow::Cow;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::{Context, Layer};

/// What the file keeps unless `AOIDE_LOG` says otherwise (EnvFilter syntax).
const FILE_DEFAULT: &str = "warn,aoide=debug";
/// What the terminal shows.
const STDERR_DEFAULT: &str = "warn,aoide=info";

/// Flushes the file's writer when dropped (at exit).
static GUARD: Mutex<Option<tracing_appender::non_blocking::WorkerGuard>> = Mutex::new(None);

/// Starts logging to `dir` (daily `aoide.YYYY-MM-DD.jsonl` files, seven kept) and to stderr, and
/// logs panics. Records of the `log` crate (tauri, reqwest, …) are logged too. Only the first
/// call does anything.
pub fn init(dir: &Path) {
    use tracing_appender::rolling::{Builder, Rotation};
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::EnvFilter;
    let file_filter =
        EnvFilter::try_from_env("AOIDE_LOG").unwrap_or_else(|_| EnvFilter::new(FILE_DEFAULT));
    let appender = std::fs::create_dir_all(dir)
        .map_err(|e| e.to_string())
        .and_then(|()| {
            Builder::new()
                .rotation(Rotation::DAILY)
                .filename_prefix("aoide")
                .filename_suffix("jsonl")
                .max_log_files(7)
                .build(dir)
                .map_err(|e| e.to_string())
        });
    let (file, guard, problem) = match appender {
        Ok(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            (
                Some(JsonLines::new(writer).with_filter(file_filter)),
                Some(guard),
                None,
            )
        }
        Err(error) => (None, None, Some(error)),
    };
    let stderr = tracing_subscriber::fmt::layer()
        .compact()
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_writer(std::io::stderr)
        .with_filter(EnvFilter::new(STDERR_DEFAULT));
    if tracing_subscriber::registry()
        .with(file)
        .with(stderr)
        .try_init()
        .is_err()
    {
        return;
    }
    *GUARD.lock().unwrap_or_else(|p| p.into_inner()) = guard;
    if let Some(error) = problem {
        tracing::warn!(event = "app.log", %error, "Cannot write the log file; logging to the terminal only");
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()));
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        tracing::error!(event = "app.panic", location, error = %message, "Aoide panicked");
        previous(info);
    }));
}

/// Writes out what is still buffered; call at exit.
pub fn flush() {
    drop(GUARD.lock().unwrap_or_else(|p| p.into_inner()).take());
}

/// A book id as logs carry it: its first 12 characters, enough to find it in the library.
pub fn book(id: &str) -> &str {
    id.get(..12).unwrap_or(id)
}

/// A file as logs name it: its name alone, never the folders it is in.
pub fn file(path: &Path) -> Cow<'_, str> {
    path.file_name()
        .map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy())
}

/// A state as logs name it: its serialized name (`playing`), the `kind` of a tagged one, or
/// `none`. The same names the window gets, so `from`/`to` can be matched against the contract.
pub fn name(value: impl serde::Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => name,
        Ok(serde_json::Value::Null) => "none".into(),
        Ok(serde_json::Value::Object(map)) if map.get("kind").is_some_and(|k| k.is_string()) => {
            map["kind"].as_str().unwrap_or_default().to_owned()
        }
        Ok(other) => other.to_string(),
        Err(_) => "?".into(),
    }
}

/// Milliseconds since `start`.
pub fn took(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

/// How bad a line from the window is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum UiLevel {
    Info,
    Warn,
    Error,
}

/// A line the window asks to log: a failed command or an error in a view.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct UiLog {
    pub level: UiLevel,
    /// A dotted name like Core's own (`ipc.error`, `ui.error`).
    pub event: String,
    pub msg: String,
    /// The command that failed or was slow.
    #[cfg_attr(test, ts(optional))]
    pub command: Option<String>,
    #[cfg_attr(test, ts(optional))]
    pub took_ms: Option<u64>,
    /// The view on screen.
    #[cfg_attr(test, ts(optional))]
    pub view: Option<String>,
}

/// Longest message kept from the window.
const MAX_UI_MSG: usize = 300;

/// Logs a line from the window under the `aoide::ui` target, its message cut short.
pub fn ui(entry: UiLog) {
    let UiLog {
        level,
        event,
        msg,
        command,
        took_ms,
        view,
    } = entry;
    let msg = match msg.char_indices().nth(MAX_UI_MSG) {
        Some((at, _)) => format!("{}…", &msg[..at]),
        None => msg,
    };
    let (event, command, view) = (event.as_str(), command.as_deref(), view.as_deref());
    match level {
        UiLevel::Info => {
            tracing::info!(target: "aoide::ui", event, command, took_ms, view, "{msg}")
        }
        UiLevel::Warn => {
            tracing::warn!(target: "aoide::ui", event, command, took_ms, view, "{msg}")
        }
        UiLevel::Error => {
            tracing::error!(target: "aoide::ui", event, command, took_ms, view, "{msg}")
        }
    }
}

/// Longest text value kept in a line; longer ones are cut, so no line floods a reader.
const MAX_VALUE: usize = 500;

/// Writes each event as one JSON line: `ts`, `level`, `event`, `msg`, the event's other fields,
/// then `target`. The home folder in any text becomes `~`.
pub struct JsonLines<W> {
    make: W,
    home: Option<String>,
}

impl<W> JsonLines<W> {
    /// Writes to `make`, with the user's home folder (`HOME`) written as `~`.
    pub fn new(make: W) -> Self {
        let home = std::env::var("HOME").ok();
        Self { make, home: None }.with_home(home.as_deref().unwrap_or_default())
    }

    /// Writes `home` as `~` (nothing for an empty or root `home`).
    pub fn with_home(mut self, home: &str) -> Self {
        let home = home.trim_end_matches('/');
        self.home = (!home.is_empty()).then(|| home.to_owned());
        self
    }
}

impl<S, W> Layer<S> for JsonLines<W>
where
    S: Subscriber,
    W: for<'a> MakeWriter<'a> + 'static,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        use tracing_log::NormalizeEvent;
        let normalized = event.normalized_metadata();
        let meta = normalized.as_ref().unwrap_or_else(|| event.metadata());
        let mut fields = Fields {
            home: self.home.as_deref(),
            ..Fields::default()
        };
        event.record(&mut fields);
        let mut line = String::with_capacity(256);
        line.push('{');
        let mut put = |key: &str, value: &serde_json::Value| {
            if line.len() > 1 {
                line.push(',');
            }
            line.push_str(&serde_json::Value::from(key).to_string());
            line.push(':');
            line.push_str(&value.to_string());
        };
        let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        put("ts", &ts.into());
        put("level", &meta.level().as_str().into());
        if let Some(name) = fields.take("event") {
            put("event", &name);
        }
        if let Some(message) = fields.take("message") {
            put("msg", &message);
        }
        for (key, value) in &fields.list {
            put(key, value);
        }
        put("target", &meta.target().into());
        line.push_str("}\n");
        let _ = self.make.make_writer().write_all(line.as_bytes());
    }
}

/// An event's fields in the order given; `log` records' own `log.*` fields left out.
#[derive(Default)]
struct Fields<'h> {
    list: Vec<(&'static str, serde_json::Value)>,
    home: Option<&'h str>,
}

impl Fields<'_> {
    fn take(&mut self, key: &str) -> Option<serde_json::Value> {
        let at = self.list.iter().position(|(k, _)| *k == key)?;
        Some(self.list.remove(at).1)
    }

    fn put(&mut self, field: &Field, value: serde_json::Value) {
        if !field.name().starts_with("log.") {
            self.list.push((field.name(), value));
        }
    }

    /// `text` with the home folder as `~`, cut to [`MAX_VALUE`] characters.
    fn text(&self, text: &str) -> serde_json::Value {
        let text = match self.home {
            Some(home) if text.contains(home) => Cow::Owned(text.replace(home, "~")),
            _ => Cow::Borrowed(text),
        };
        match text.char_indices().nth(MAX_VALUE) {
            Some((at, _)) => format!("{}…", &text[..at]).into(),
            None => text.into_owned().into(),
        }
    }
}

impl Visit for Fields<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        let value = self.text(value);
        self.put(field, value);
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.put(field, value.into());
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.put(field, value.into());
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.put(field, ((value * 1000.0).round() / 1000.0).into());
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.put(field, value.into());
    }
    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        let value = self.text(&value.to_string());
        self.put(field, value);
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let value = self.text(&format!("{value:?}"));
        self.put(field, value);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

    /// Lines written to memory.
    #[derive(Clone, Default)]
    struct Memory(Arc<Mutex<Vec<u8>>>);

    impl Write for Memory {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Memory {
        type Writer = Memory;
        fn make_writer(&'a self) -> Memory {
            self.clone()
        }
    }

    fn logged(emit: impl FnOnce()) -> Vec<String> {
        let memory = Memory::default();
        let subscriber = tracing_subscriber::registry().with(JsonLines::new(memory.clone()));
        tracing::subscriber::with_default(subscriber, emit);
        let text = String::from_utf8(memory.0.lock().unwrap().clone()).unwrap();
        text.lines().map(str::to_owned).collect()
    }

    #[test]
    fn an_event_is_one_flat_json_line_led_by_time_level_and_event() {
        let lines = logged(|| {
            tracing::info!(
                event = "sync.stage",
                book = "4b1e9c07",
                from = "waiting",
                to = "transcribing",
                took_ms = 12u64,
                "Started syncing"
            );
        });
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with(r#"{"ts":""#), "{}", lines[0]);
        let line: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(line["level"], "INFO");
        assert_eq!(line["event"], "sync.stage");
        assert_eq!(line["msg"], "Started syncing");
        assert_eq!(line["book"], "4b1e9c07");
        assert_eq!(line["to"], "transcribing");
        assert_eq!(line["took_ms"], 12);
        assert_eq!(line["target"], module_path!());
        let ts = line["ts"].as_str().unwrap();
        assert!(
            ts.ends_with('Z') && ts.len() == "2026-11-15T10:42:18.123Z".len(),
            "{ts}"
        );
    }

    #[test]
    fn books_go_by_a_short_id_and_files_by_name_alone() {
        assert_eq!(book("4b1e9c07aa11bb22cc33"), "4b1e9c07aa11");
        assert_eq!(book("short"), "short");
        assert_eq!(
            file(Path::new("/home/someone/Audiobooks/Lampmaker.m4b")),
            "Lampmaker.m4b"
        );
    }

    #[test]
    fn states_go_by_their_serialized_names() {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        enum Plain {
            NotStarted,
        }
        #[derive(serde::Serialize)]
        #[serde(tag = "kind", rename_all = "camelCase")]
        enum Tagged {
            Verifying { candidate: usize },
        }
        assert_eq!(name(Plain::NotStarted), "notStarted");
        assert_eq!(name(Some(Plain::NotStarted)), "notStarted");
        assert_eq!(name(None::<Plain>), "none");
        assert_eq!(name(Tagged::Verifying { candidate: 1 }), "verifying");
    }

    #[test]
    fn the_home_folder_is_written_as_a_tilde() {
        let memory = Memory::default();
        let layer = JsonLines::new(memory.clone()).with_home("/home/someone");
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(
                event = "library.watch",
                error = "cannot read /home/someone/Books/a.epub",
                "failed"
            )
        });
        let text = String::from_utf8(memory.0.lock().unwrap().clone()).unwrap();
        let line: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(line["error"], "cannot read ~/Books/a.epub");
    }

    #[test]
    fn a_line_from_the_window_is_logged_under_ui_without_its_empty_fields() {
        let lines = logged(|| {
            ui(UiLog {
                level: UiLevel::Warn,
                event: "ipc.error".into(),
                msg: "y".repeat(1_000),
                command: Some("list_books".into()),
                took_ms: None,
                view: None,
            })
        });
        let line: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(line["target"], "aoide::ui");
        assert_eq!(line["event"], "ipc.error");
        assert_eq!(line["command"], "list_books");
        assert!(line.get("took_ms").is_none() && line.get("view").is_none());
        assert_eq!(line["msg"].as_str().unwrap().chars().count(), 301);
    }

    #[test]
    fn long_values_are_cut_short() {
        let long = "x".repeat(2_000);
        let lines = logged(|| tracing::warn!(event = "ipc.error", error = %long, "failed"));
        let line: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert!(line["error"].as_str().unwrap().chars().count() <= 501);
    }
}
