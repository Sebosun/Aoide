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
- **Reading app, phases 2 to 4**: read without audio (silent reading with its own place and reading time); audiobooks without an EPUB, played on their own and paired later; web articles saved by link, fetched once from public sites, read, narrated, and fetched again or from an archived copy when a page gives only part of them. See [reading-app/2-reading.md](reading-app/2-reading.md), [3-audiobooks.md](reading-app/3-audiobooks.md) and [4-articles.md](reading-app/4-articles.md).

## Next

- **A reading app, not only a player** (phases 1 to 4 done): feeds next, then an optional browser for hard pages. Plan and phases in [reading-app/plan.md](reading-app/plan.md).

- More engine adapters.
- Audiobook sync, phases 2 and 3 (see [audiobook-sync.md](audiobook-sync.md#phases)): listening while it syncs, an OpenAI-style transcription adapter, exact word timing for recordings, EPUB 3 Media Overlays export.
- Release builds (AppImage, RPM, DEB) instead of build-from-source only.
