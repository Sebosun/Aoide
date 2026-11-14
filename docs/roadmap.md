# Roadmap

## Done

- **Read and listen**: read-along with sentence and word highlight, playback in Rust, generation just ahead of the playhead, whole-book preparation, a speech queue panel.
- **Voices**: engine, voice and settings pickers with per-book overrides; Chatterbox and OpenAI-compatible adapters; launch commands; voice preview.
- **Library**: content-hash identity, missing-file handling, *Find books* scan, Open Library details, collections, listening stats, bookmarks, search, sleep timer, MPRIS.
- **Export**: M4B with chapters and cover.
- **Exact word timing** through the optional local aligner.
- **Your own audiobook**: pair a DRM-free recording with its EPUB; local speech recognition (whisper.cpp, CPU or GPU) syncs it in the background and it plays with the same highlighting. Design, phases and measurements in [audiobook-sync.md](audiobook-sync.md).
- **Interface**: custom dropdowns, dialogs and toasts, book details page, settings with tabs, storage tab, interface zoom, keyboard shortcuts.

## Next

- More engine adapters.
- Audiobook sync, phases 2 and 3 (see [audiobook-sync.md](audiobook-sync.md#phases)): listening while it syncs, an OpenAI-style transcription adapter, exact word timing for recordings, EPUB 3 Media Overlays export.
- Release builds (AppImage, RPM, DEB) instead of build-from-source only.
