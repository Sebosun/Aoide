//! The one error type commands answer with, and [`Reporter`], which tells the user about a
//! failure Aoide carried on from.
//!
//! Every variant carries the message to show, so the window never composes one. Modules move to
//! `CoreError` one at a time: a `String` error converts to [`CoreError::Internal`], and a
//! `CoreError` back to its message where a module still answers with `String`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::{CoreEvent, Events};

/// What went wrong, by kind, with the message to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum CoreError {
    /// A book, chapter, file or other thing asked for does not exist.
    NotFound { what: String, message: String },
    /// A value the user gave (or a file holds) is not acceptable; `field` names it.
    Invalid { field: String, message: String },
    /// The speech engine or another local service cannot be reached.
    EngineUnavailable { message: String },
    /// Something else is under way; try again later.
    Busy { message: String },
    /// Reading or writing a file or the network failed.
    Io { message: String },
    /// The user stopped it (a scan); nothing to report.
    Cancelled { message: String },
    /// A failure the user cannot act on.
    Internal { message: String },
}

/// What a job the user stopped answers with, while its module still answers with `String`.
pub const CANCELLED: &str = "cancelled";

impl CoreError {
    pub fn not_found(what: impl Into<String>) -> Self {
        let what = what.into();
        let message = format!("{what} was not found");
        CoreError::NotFound { what, message }
    }

    pub fn invalid(field: impl Into<String>, message: impl Into<String>) -> Self {
        CoreError::Invalid {
            field: field.into(),
            message: message.into(),
        }
    }

    pub fn engine(message: impl ToString) -> Self {
        CoreError::EngineUnavailable {
            message: message.to_string(),
        }
    }

    pub fn io(message: impl ToString) -> Self {
        CoreError::Io {
            message: message.to_string(),
        }
    }

    pub fn internal(message: impl ToString) -> Self {
        CoreError::Internal {
            message: message.to_string(),
        }
    }

    /// The message to show.
    pub fn message(&self) -> &str {
        match self {
            CoreError::NotFound { message, .. }
            | CoreError::Invalid { message, .. }
            | CoreError::EngineUnavailable { message }
            | CoreError::Busy { message }
            | CoreError::Io { message }
            | CoreError::Cancelled { message }
            | CoreError::Internal { message } => message,
        }
    }

    /// The kind's name, as the window gets it.
    pub fn kind(&self) -> &'static str {
        match self {
            CoreError::NotFound { .. } => "notFound",
            CoreError::Invalid { .. } => "invalid",
            CoreError::EngineUnavailable { .. } => "engineUnavailable",
            CoreError::Busy { .. } => "busy",
            CoreError::Io { .. } => "io",
            CoreError::Cancelled { .. } => "cancelled",
            CoreError::Internal { .. } => "internal",
        }
    }
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for CoreError {}

impl From<String> for CoreError {
    fn from(message: String) -> Self {
        if message == CANCELLED {
            return CoreError::Cancelled { message };
        }
        CoreError::Internal { message }
    }
}

impl From<&str> for CoreError {
    fn from(message: &str) -> Self {
        CoreError::internal(message)
    }
}

/// For modules that still answer with `String`.
impl From<CoreError> for String {
    fn from(error: CoreError) -> Self {
        error.to_string()
    }
}

impl From<std::io::Error> for CoreError {
    fn from(error: std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::NotFound => CoreError::NotFound {
                what: "file".into(),
                message: error.to_string(),
            },
            _ => CoreError::io(error),
        }
    }
}

impl From<aoide_library::LibraryError> for CoreError {
    fn from(error: aoide_library::LibraryError) -> Self {
        use aoide_library::LibraryError::*;
        match error {
            Io(error) => error.into(),
            Invalid(message) => CoreError::invalid("library", message),
            error @ (Database(_) | Corrupt(_)) => CoreError::internal(error),
        }
    }
}

impl From<aoide_epub::ParseError> for CoreError {
    fn from(error: aoide_epub::ParseError) -> Self {
        CoreError::invalid("epub", error.to_string())
    }
}

impl From<aoide_config::ConfigError> for CoreError {
    fn from(error: aoide_config::ConfigError) -> Self {
        use aoide_config::ConfigError::*;
        match &error {
            Io { .. } => CoreError::io(error),
            _ => CoreError::invalid("settings", error.to_string()),
        }
    }
}

impl From<aoide_tts::EngineError> for CoreError {
    fn from(error: aoide_tts::EngineError) -> Self {
        use aoide_tts::EngineError::*;
        match &error {
            Unavailable(_) | Timeout | Http { .. } => CoreError::engine(error),
            InvalidConfig(_) | InvalidInput(_) => CoreError::invalid("engine", error.to_string()),
            InvalidResponse(_) | InvalidAudio(_) => CoreError::internal(error),
        }
    }
}

impl From<aoide_chatterbox::Error> for CoreError {
    fn from(error: aoide_chatterbox::Error) -> Self {
        use aoide_chatterbox::Error::*;
        match &error {
            Transport(_) | Timeout | Http { .. } => CoreError::engine(error),
            InvalidEndpoint(_) | InvalidOptions(_) | InvalidInput(_) => {
                CoreError::invalid("engine", error.to_string())
            }
            _ => CoreError::internal(error),
        }
    }
}

impl From<aoide_aligner::Error> for CoreError {
    fn from(error: aoide_aligner::Error) -> Self {
        use aoide_aligner::Error::*;
        match &error {
            Unavailable(_) | Timeout | Http { .. } => CoreError::engine(error),
            InvalidConfig(_) | InvalidInput(_) => CoreError::invalid("aligner", error.to_string()),
            InvalidResponse(_) => CoreError::internal(error),
        }
    }
}

impl From<aoide_metadata::Error> for CoreError {
    fn from(error: aoide_metadata::Error) -> Self {
        use aoide_metadata::Error::*;
        match &error {
            Transport(_) | Timeout | Http(_) => CoreError::io(error),
            InvalidOptions(_) | InvalidInput(_) => {
                CoreError::invalid("metadata", error.to_string())
            }
            TooLarge(_) | InvalidResponse(_) => CoreError::internal(error),
        }
    }
}

impl From<aoide_audio::AudioError> for CoreError {
    fn from(error: aoide_audio::AudioError) -> Self {
        use aoide_audio::AudioError::*;
        match &error {
            Decode(_) => CoreError::invalid("audio", error.to_string()),
            Output(_) => CoreError::io(error),
            Encode(_) => CoreError::internal(error),
        }
    }
}

impl From<aoide_recording::RecordingError> for CoreError {
    fn from(error: aoide_recording::RecordingError) -> Self {
        use aoide_recording::RecordingError::*;
        match &error {
            Read { .. } => CoreError::io(error),
            OutOfRange(_) => CoreError::invalid("recording", error.to_string()),
        }
    }
}

impl From<aoide_transcribe::Error> for CoreError {
    fn from(error: aoide_transcribe::Error) -> Self {
        use aoide_transcribe::Error::*;
        match &error {
            Model(_) => CoreError::io(error),
            Aborted => CoreError::Busy {
                message: error.to_string(),
            },
            Failed(_) => CoreError::internal(error),
        }
    }
}

/// A failure the user is told about: what Aoide was doing, and the error.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ProblemView {
    /// The sentence to show: what failed and why.
    pub message: String,
    pub error: CoreError,
}

/// How long the same failure stays quiet after it was told.
const QUIET: Duration = Duration::from_secs(60);

/// Logs a failure Aoide carried on from and tells the user (`core://problem`), once a minute at
/// most for the same event. Cheap to clone; components that have no `Core` take one.
#[derive(Clone)]
pub struct Reporter {
    events: Option<Arc<dyn Events>>,
    told: Arc<Mutex<HashMap<&'static str, Instant>>>,
}

/// Logs only, telling no one (components in tests).
impl Default for Reporter {
    fn default() -> Self {
        Self {
            events: None,
            told: Arc::default(),
        }
    }
}

impl Reporter {
    pub fn new(events: Arc<dyn Events>) -> Self {
        Self {
            events: Some(events),
            told: Arc::default(),
        }
    }

    /// `event` is the log's dotted name for the failure; `doing` says what failed, as the user
    /// reads it ("Could not save your place").
    pub fn report(&self, event: &'static str, doing: &str, error: impl Into<CoreError>) {
        let error = error.into();
        let repeat = {
            let mut told = self.told.lock().unwrap_or_else(|p| p.into_inner());
            let now = Instant::now();
            let repeat = told.get(event).is_some_and(|at| now - *at < QUIET);
            if !repeat {
                told.insert(event, now);
            }
            repeat
        };
        tracing::warn!(event, kind = error.kind(), %error, repeat, "{doing}");
        let Some(events) = &self.events else {
            return;
        };
        if repeat {
            return;
        }
        let problem = ProblemView {
            message: format!("{doing}: {error}"),
            error,
        };
        let event = CoreEvent::Problem(problem);
        if let Ok(payload) = event.payload() {
            events.emit(event.name(), payload);
        }
    }
}

#[cfg(test)]
mod cancelled_tests {
    use super::*;

    #[test]
    fn a_job_the_user_stopped_answers_as_cancelled_not_as_a_failure() {
        let error = CoreError::from(CANCELLED.to_owned());
        assert_eq!(error.kind(), "cancelled");
        assert_eq!(error.message(), "cancelled");
        assert_eq!(
            CoreError::from("cancelled by the site".to_owned()).kind(),
            "internal"
        );
    }
}
