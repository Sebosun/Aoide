# Roadmap

## Done

- **Read and listen**: read-along with sentence and word highlight, playback in Rust, generation just ahead of the playhead, whole-book preparation, a speech queue panel.
- **Voices**: engine, voice and settings pickers with per-book overrides; Chatterbox and OpenAI-compatible adapters; launch commands; voice preview.
- **Library**: content-hash identity, missing-file handling, *Find books* scan, Open Library details, collections, listening stats, bookmarks, search, sleep timer, MPRIS.
- **Export**: M4B with chapters and cover.
- **Exact word timing** through the optional local aligner.
- **Your own audiobook**: pair a DRM-free recording with its EPUB; local speech recognition (whisper.cpp, CPU or GPU) syncs it in the background and it plays with the same highlighting. Design, phases and measurements in [audiobook-sync.md](audiobook-sync.md).
- **Interface**: custom dropdowns, dialogs and toasts, book details page, settings with tabs, storage tab, interface zoom, keyboard shortcuts.
- **Reading app, phase 1 (the shell)**: a sidebar with Home, Library, Activity and collections; the reader takes the whole window; an item page with Read and Listen; one Add dialog; the engine shown only while it works or fails. See [reading-app/1-shell.md](reading-app/1-shell.md).
- **Reading app, phases 2 to 5**: read without audio (silent reading with its own place and reading time); audiobooks without an EPUB, played on their own and paired later; web articles saved by link, fetched once from public sites, read, narrated, and fetched again or from an archived copy when a page gives only part of them; RSS, Atom and JSON feeds checked in the background with conditional requests, read in the reader and saved to the library, old unsaved entries cleaned up, OPML in and out. See [reading-app/2-reading.md](reading-app/2-reading.md), [3-audiobooks.md](reading-app/3-audiobooks.md), [4-articles.md](reading-app/4-articles.md) and [5-feeds.md](reading-app/5-feeds.md).
- **Reading app, phase 6 (the optional browser)**: off by default; Chrome or Chromium (found, chosen, or Chrome for Testing downloaded and checked) renders pages plain HTTP cannot read, behind a proxy that keeps it to public addresses; sign in to a site or pass its check once in a visible window; per-site rules and sessions in Settings › Sites; items that need the user are marked in Feeds and Home. See [reading-app/6-browser.md](reading-app/6-browser.md).

## Next

- More engine adapters.
- Audiobook sync, phases 2 and 3 (see [audiobook-sync.md](audiobook-sync.md#phases)): listening while it syncs, an OpenAI-style transcription adapter, exact word timing for recordings, EPUB 3 Media Overlays export.
- Release builds (AppImage, RPM, DEB) instead of build-from-source only.

## Proposed

Plans for features that are not started, one per feature, with their order and sizes:
[next/plan.md](next/plan.md). They cover narration (saying a sentence again, a pronunciation
list, leaving chapters out, pauses, a voice for dialogue), the player, the reader (a selection
menu, highlights, typography, a dictionary, library-wide search), the library (editing details,
series, archive, formats, backup), articles and feeds (imports, folders, podcasts) and the
desktop (open with, notifications, background work, releases).
