//! The UI↔core contract: TypeScript declarations of every type the commands and events carry,
//! generated from the Rust types into `src/api/contract.ts`.
//!
//! The test fails when the file is out of date. Regenerate it with
//! `AOIDE_WRITE_CONTRACT=1 cargo test -p aoide contract`.

use std::path::Path;

use ts_rs::{Config, TS};

use super::CoreEvent;

fn contract() -> String {
    let config = Config::new().with_large_int("number");
    let mut out = String::from(
        "// Generated from the Rust types by `AOIDE_WRITE_CONTRACT=1 cargo test -p aoide contract`.\n\
         // Do not edit: change the Rust type and regenerate.\n\n",
    );
    macro_rules! declare {
        ($($ty:ty),* $(,)?) => {$(
            out.push_str("export ");
            out.push_str(&<$ty as TS>::decl(&config));
            out.push_str("\n\n");
        )*};
    }
    declare!(
        // Library
        aoide_library::BookId,
        aoide_library::BookStatus,
        aoide_library::Collection,
        super::BookProgressView,
        super::BookSummary,
        super::ImportResult,
        super::AppInfo,
        // Preparation
        super::preparation::Stage,
        super::preparation::PrepareView,
        // Reading
        aoide_epub::BlockKind,
        crate::book::NoteMarker,
        crate::book::SegmentView,
        crate::book::BlockView,
        crate::book::ChapterSummary,
        crate::book::ChapterView,
        crate::book::ExcludedView,
        crate::book::SearchHit,
        super::BookView,
        // Playback and generation
        super::PlaybackStatus,
        super::SleepView,
        super::PlayerStateView,
        super::EngineState,
        super::EngineStatusView,
        super::PreparingView,
        super::GenerationStateView,
        // Engines and voices
        aoide_tts::ParamKind,
        aoide_tts::ChoiceOption,
        aoide_tts::ParamValue,
        aoide_tts::ParamSpec,
        aoide_tts::EngineDescriptor,
        aoide_tts::NetworkScope,
        aoide_tts::LaunchCommand,
        aoide_tts::EngineProfile,
        aoide_tts::Voice,
        super::EngineOptions,
        super::BookSettings,
        // Bookmarks, stats, details
        aoide_library::Bookmark,
        aoide_library::DayListening,
        super::extras::BookListeningView,
        super::extras::ListeningStatsView,
        aoide_library::BookDetails,
        aoide_metadata::Match,
        // Settings and storage
        super::extras::LibrarySort,
        super::extras::AlignerSettings,
        super::discovery::ScanSettings,
        super::extras::AppSettings,
        super::storage::BookStorage,
        super::storage::StorageReport,
        // Finding books
        super::discovery::CandidateStatus,
        super::discovery::Candidate,
        super::discovery::ScanStage,
        super::discovery::ScanProgress,
        super::discovery::ImportProgress,
        // Appearance
        aoide_config::Mode,
        aoide_config::ReaderMode,
        aoide_config::Appearance,
        aoide_config::Theme,
        super::config::PreferencesView,
        // Export
        super::export::ExportStage,
        super::export::ExportProgress,
    );
    // One line per `CoreEvent` variant.
    let events = [
        (CoreEvent::LIBRARY_CHANGED, "null".to_owned()),
        (
            CoreEvent::CONFIG_CHANGED,
            name::<super::config::PreferencesView>(&config),
        ),
        (
            CoreEvent::PLAYER_STATE,
            name::<super::PlayerStateView>(&config),
        ),
        (
            CoreEvent::ENGINE_STATUS,
            name::<super::EngineStatusView>(&config),
        ),
        (
            CoreEvent::GENERATION_STATE,
            name::<super::GenerationStateView>(&config),
        ),
        (
            CoreEvent::SCAN_PROGRESS,
            name::<super::discovery::ScanProgress>(&config),
        ),
        (CoreEvent::DISCOVERY_DETAILS, name::<String>(&config)),
        (
            CoreEvent::DISCOVERY_IMPORT,
            name::<super::discovery::ImportProgress>(&config),
        ),
        (
            CoreEvent::EXPORT_PROGRESS,
            name::<super::export::ExportProgress>(&config),
        ),
    ];
    out.push_str(
        "/** Every event Core emits, by name, with its payload. */\nexport type CoreEvents = {\n",
    );
    for (event, payload) in events {
        out.push_str(&format!("  \"{event}\": {payload},\n"));
    }
    out.push_str("};\n");
    out
}

fn name<T: TS>(config: &Config) -> String {
    T::name(config)
}

#[test]
fn the_typescript_contract_matches_the_rust_types() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/api/contract.ts");
    let text = contract();
    if std::env::var_os("AOIDE_WRITE_CONTRACT").is_some() {
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        current == text,
        "src/api/contract.ts is out of date: run `AOIDE_WRITE_CONTRACT=1 cargo test -p aoide contract`"
    );
}
