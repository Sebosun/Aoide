# Roadmap

## Done

- **Read and listen**: read-along with sentence and word highlight, playback in Rust, generation just ahead of the playhead, whole-book preparation, a speech queue panel.
- **Voices**: engine, voice and settings pickers with per-book overrides; Chatterbox and OpenAI-compatible adapters; launch commands; voice preview.
- **Library**: content-hash identity, missing-file handling, *Find books* scan, Open Library details, collections, listening stats, bookmarks, search, sleep timer, MPRIS.
- **Export**: M4B with chapters and cover.
- **Exact word timing** through the optional local aligner.
- **Interface**: custom dropdowns, dialogs and toasts, book details page, settings with tabs, storage tab, interface zoom, keyboard shortcuts.

## Next

- More engine adapters.
- Release builds (AppImage, RPM, DEB) instead of build-from-source only.

## Idea: pair a real audiobook with its EPUB

Import DRM-free audio (one M4B or a folder of MP3s) and play it against the book text with the same sentence and word highlight, instead of generated speech. Judged feasible; not started.

- Transcribe the audio with Whisper (word timestamps, voice-activity filtering). A small model is enough because the book supplies the real words.
- Anchor the transcript to the EPUB text: word runs that occur exactly once in both and appear in the same order become sync points; a word-level diff fills the gaps between them. Unmatched stretches (narrator credits, spoken chapter titles, skipped footnotes) are flagged, not forced.
- Word-level timing: refine each matched sentence with the existing MMS_FA aligner in `tools/aligner`.
- Detect a low match rate (abridged or dramatised recording, different translation or edition) and tell the user instead of showing a bad sync.
- Start with a command-line prototype (audio + EPUB in, timing file out) on two or three owned books, one with footnotes or a long intro. Read how the open-source Storyteller project does this before writing the aligner.
- Rough effort: prototype 2–4 days; import flow, background job and playback 1–2 weeks; hardening (multi-file books, mismatch report, resume, more languages) 1–3 weeks.
