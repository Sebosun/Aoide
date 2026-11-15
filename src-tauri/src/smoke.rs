//! Debug-only headless scenarios against a throwaway library folder (`AOIDE_SMOKE_HOME`), run
//! through Core's public API like the UI would and logged to stderr; the app exits when one ends
//! (with 1 when it failed).
//!
//! `AOIDE_SMOKE_SECONDS` starts a run. What runs is chosen by the other `AOIDE_SMOKE_*` variables
//! (see [`Scenario::from_env`]); the scenarios that need a book take the first present one whose
//! title contains `AOIDE_SMOKE_BOOK`. `AOIDE_SMOKE_ALIGNER` sets an aligner's address first.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::core::{AlignerSettings, CandidateStatus, Core, ImportChoice, Paths, SyncStage};

/// The paths of a library kept under `home` instead of the real one.
pub fn paths(home: &Path) -> Paths {
    Paths {
        home_dir: home.join("Home"),
        library_dir: home.join("Books"),
        covers_dir: home.join("data").join("covers"),
        audio_dir: home.join("cache").join("audio"),
        db_path: home.join("data").join("library.sqlite3"),
        config_dir: home.join("config").join("aoide"),
        models_dir: home.join("data").join("models"),
    }
}

/// What a smoke run does.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Scenario {
    /// `AOIDE_SMOKE_DISCOVER`: scans for books twice, then imports the new ones
    /// (`AOIDE_SMOKE_DETAILS`: looking up their details first).
    Discover { details: bool },
    /// `AOIDE_SMOKE_PREPARE`: prepares the book for the run's seconds, then cancels.
    Prepare,
    /// `AOIDE_SMOKE_EXPORT`: exports the book as an M4B.
    Export,
    /// `AOIDE_SMOKE_READ_ALONG`: exports the book with its synced recording as a read-along
    /// EPUB.
    ReadAlong,
    /// Otherwise: plays the book (from `AOIDE_SMOKE_CHAPTER`) for the run's seconds, then
    /// bookmarks it and stops.
    Play { chapter: Option<usize> },
}

impl Scenario {
    fn from_env() -> Self {
        let set = |name: &str| std::env::var_os(name).is_some();
        if set("AOIDE_SMOKE_DISCOVER") {
            Scenario::Discover {
                details: set("AOIDE_SMOKE_DETAILS"),
            }
        } else if set("AOIDE_SMOKE_PREPARE") {
            Scenario::Prepare
        } else if set("AOIDE_SMOKE_EXPORT") {
            Scenario::Export
        } else if set("AOIDE_SMOKE_READ_ALONG") {
            Scenario::ReadAlong
        } else {
            Scenario::Play {
                chapter: std::env::var("AOIDE_SMOKE_CHAPTER")
                    .ok()
                    .and_then(|c| c.parse().ok()),
            }
        }
    }
}

/// Runs the scenario the environment asks for once Core has settled, then exits the app.
pub fn spawn(app: tauri::AppHandle, core: Arc<Core>, seconds: u64) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let result = run(&core, Scenario::from_env(), seconds).await;
        if let Err(error) = &result {
            tracing::error!(event = "smoke", %error, "Smoke run failed");
        }
        app.exit(if result.is_ok() { 0 } else { 1 });
    });
}

async fn run(core: &Arc<Core>, scenario: Scenario, seconds: u64) -> Result<(), String> {
    if let Ok(url) = std::env::var("AOIDE_SMOKE_ALIGNER") {
        let mut settings = core.app_settings();
        settings.aligner = Some(AlignerSettings {
            base_url: url,
            network: Default::default(),
            launch: None,
        });
        tracing::info!(
            event = "smoke",
            "aligner settings: {:?}",
            core.save_app_settings(settings)
        );
    }
    if let Scenario::Discover { details } = scenario {
        return discover(core, details).await;
    }
    let id = book(core)?;
    if let Ok(paths) = std::env::var("AOIDE_SMOKE_ATTACH") {
        attach(core, &id, paths.split('|').map(str::to_owned).collect()).await?;
    }
    match scenario {
        Scenario::Discover { .. } => unreachable!("handled above"),
        Scenario::Prepare => prepare(core, &id, seconds).await,
        Scenario::Export => {
            let started = Instant::now();
            let result = core.export_book(&id).await;
            tracing::info!(
                event = "smoke",
                "export after {:?}: {result:?}",
                started.elapsed()
            );
            result.map(drop)
        }
        Scenario::ReadAlong => read_along(core, &id).await,
        Scenario::Play { chapter } => play(core, &id, chapter, seconds).await,
    }
}

/// The first present book whose title contains `AOIDE_SMOKE_BOOK`, opened.
fn book(core: &Arc<Core>) -> Result<String, String> {
    let wanted = std::env::var("AOIDE_SMOKE_BOOK")
        .unwrap_or_default()
        .to_lowercase();
    let books = core.list_books()?;
    tracing::info!(event = "smoke", "{} books", books.len());
    let book = books
        .iter()
        .find(|b| {
            matches!(b.status, aoide_library::BookStatus::Present)
                && b.title.to_lowercase().contains(&wanted)
        })
        .ok_or("no matching book")?;
    let view = core.open_book(&book.id);
    tracing::info!(
        event = "smoke",
        "opened {:?}: {:?}",
        book.title,
        view.as_ref().map(|v| v.chapters.len())
    );
    Ok(book.id.clone())
}

/// `AOIDE_SMOKE_ATTACH`: pairs these audio files (separated by `|`) with the book and waits out
/// the sync, accepting what it asks about.
async fn attach(core: &Arc<Core>, id: &str, paths: Vec<String>) -> Result<(), String> {
    let started = Instant::now();
    let view = core.attach_recording(id, paths)?;
    tracing::info!(event = "smoke", "attached: {:?}", view.stage);
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;
        let view = core.recording(id).ok_or("the recording is gone")?;
        tracing::info!(
            event = "smoke",
            "{:?} {:?} {} / {} ms",
            started.elapsed().as_secs(),
            view.stage,
            view.transcribed_ms,
            view.total_ms
        );
        match view.stage {
            SyncStage::Ready => break,
            SyncStage::PoorMatch | SyncStage::SampleMismatch => {
                let _ = core.sync_action(id, crate::core::SyncAction::Accept);
            }
            SyncStage::Failed | SyncStage::NeedsModel | SyncStage::Missing => {
                return Err(format!("sync stopped: {:?}", view.error));
            }
            _ => {}
        }
    }
    if let Ok(report) = core.sync_report(id) {
        tracing::info!(event = "smoke", "summary {:?}", report.summary);
    }
    Ok(())
}

async fn discover(core: &Arc<Core>, details: bool) -> Result<(), String> {
    for round in 1..=2 {
        let started = Instant::now();
        let found = core.scan_for_books().await?;
        tracing::info!(event = "smoke", "scan {round} took {:?}", started.elapsed());
        for c in &found {
            tracing::info!(
                event = "smoke",
                "  {:10?} {:?} by {:?} x{} cover={} err={:?}",
                c.status,
                c.title,
                c.authors,
                c.paths.len(),
                c.cover_path.is_some(),
                c.error
            );
        }
    }
    let new = || -> Vec<_> {
        core.discovered_books()
            .unwrap_or_default()
            .into_iter()
            .filter(|c| c.status == CandidateStatus::New)
            .collect()
    };
    if details {
        let ids = new().into_iter().map(|c| c.id).collect();
        tracing::info!(
            event = "smoke",
            "details: {:?}",
            core.fetch_discovered_details(ids).await
        );
        for c in new() {
            tracing::info!(
                event = "smoke",
                "  suggestion for {:?}: {:?}",
                c.title,
                c.suggestion.as_ref().map(|m| (&m.title, &m.key))
            );
        }
    }
    let choices = new()
        .into_iter()
        .map(|c| ImportChoice {
            id: c.id,
            apply_suggestion: false,
            pair_audiobook: false,
        })
        .collect();
    tracing::info!(
        event = "smoke",
        "import: {:?}",
        core.import_discovered(choices).await
    );
    let titles: Vec<String> = core
        .list_books()
        .unwrap_or_default()
        .into_iter()
        .map(|b| b.title)
        .collect();
    tracing::info!(event = "smoke", "library now: {titles:?}");
    for c in core.discovered_books().unwrap_or_default() {
        tracing::info!(
            event = "smoke",
            "  after import {:10?} {:?}",
            c.status,
            c.title
        );
    }
    Ok(())
}

async fn prepare(core: &Arc<Core>, id: &str, seconds: u64) -> Result<(), String> {
    core.prepare_book(id).await?;
    for t in 0..seconds {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let g = core.generation_state();
        tracing::info!(
            event = "smoke",
            "t={t:>2}s preparing={:?} gen={:?} err={:?}",
            g.preparing,
            g.generating_segment_id,
            g.error
        );
    }
    core.cancel_prepare(id);
    Ok(())
}

async fn read_along(core: &Arc<Core>, id: &str) -> Result<(), String> {
    // The recording is checked after start; wait for that to settle.
    for _ in 0..120 {
        if core.recording(id).map(|r| r.stage) != Some(SyncStage::Checking) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let started = Instant::now();
    let result = core.export_read_along(id).await;
    tracing::info!(
        event = "smoke",
        "read-along export after {:?}: {result:?}",
        started.elapsed()
    );
    result.map(drop)
}

async fn play(
    core: &Arc<Core>,
    id: &str,
    chapter: Option<usize>,
    seconds: u64,
) -> Result<(), String> {
    if let Err(error) = core.play(id, chapter, None).await {
        tracing::warn!(event = "smoke", "play failed: {error}");
    }
    let started = Instant::now();
    while started.elapsed().as_secs() < seconds {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let p = core.player_state();
        let g = core.generation_state();
        tracing::info!(event = "smoke", "t={:>2}s {:?} seg={:?} word={:?} frac={:.3} | gen={:?} buffered={}ms pending={} err={:?}",
            started.elapsed().as_secs(), p.status, p.segment_id, p.word_index, p.chapter_fraction,
            g.generating_segment_id, g.buffered_ahead_ms, g.pending, g.error
        );
    }
    match core.add_bookmark(id, "smoke note") {
        Ok(mark) => tracing::info!(
            event = "smoke",
            "bookmark {:?} at {:?}: {:?}",
            mark.id,
            mark.segment_id,
            mark.excerpt
        ),
        Err(error) => tracing::warn!(event = "smoke", "bookmark failed: {error}"),
    }
    core.stop();
    tokio::time::sleep(Duration::from_millis(200)).await;
    match core.listening_stats() {
        Ok(stats) => tracing::info!(
            event = "smoke",
            "listened total {} ms, days {:?}",
            stats.total_ms,
            stats.by_day
        ),
        Err(error) => tracing::warn!(event = "smoke", "stats failed: {error}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_throwaway_home_keeps_everything_under_it() {
        let home = Path::new("/smoke");
        let paths = paths(home);
        for path in [
            &paths.home_dir,
            &paths.library_dir,
            &paths.covers_dir,
            &paths.audio_dir,
            &paths.db_path,
            &paths.config_dir,
            &paths.models_dir,
        ] {
            assert!(path.starts_with(home), "{path:?}");
        }
    }
}
