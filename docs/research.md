# Research notes

Findings that inform future work. Nothing here is implemented yet unless it says so.

## Other book formats

Aoide's pipeline after parsing does not depend on the format. Segmenting, speech, highlighting, the
audio cache, bookmarks, search and export all consume `aoide_epub::ParsedBook`: chapters made of
typed blocks (heading, paragraph, list item, preformatted), plus metadata and notes. A new format
needs only a parser crate that produces the same structure. Book identity (content hash), the
library, the folder watcher and *Find books* work as they are once the file extension is accepted.

Measured on the developer's machine (2026-11-11): 53 EPUB, 203 PDF (120 over 1 MB, many of them
papers or documents rather than books), 4 MOBI and 2 AZW3 files.

Suggested order: PDF v1 (most files), then MOBI/AZW3.

### PDF: feasible, quality depends on layout reconstruction

A PDF stores positioned glyphs, not structured text, so reading order and structure must be
reconstructed.

| Problem | Approach | Difficulty |
| --- | --- | --- |
| Chapters | The PDF outline (bookmarks), which most real books have; otherwise guess from large or bold headings | Easy with an outline, unreliable without |
| Running headers, footers, page numbers | Drop lines that repeat at the same position across pages | Medium, reliable |
| Line-end hyphenation, lines broken mid-paragraph | Join them using line spacing and indentation | Medium |
| Multi-column layouts, sidebars, captions | Layout analysis by position | Hard; mostly papers and textbooks |
| Footnotes | Small text at the page bottom, matched to superscript markers | Hard; v1 skips or flags them |
| Tables, maths, code | Preformatted blocks or a warning | Partial |
| Scanned PDFs (images only) | OCR (e.g. Tesseract) | Out of scope for v1: detect and reject with a clear message |

Libraries (Rust):

- **pdfium-render** (Google's PDFium): the best text and glyph-position extraction, and a permissive
  licence. Needs the PDFium shared library shipped with the app (about 5–10 MB), loaded at runtime.
  **Recommended.**
- `lopdf` / `pdf-extract` (pure Rust): no extra library, but clearly weaker on real books.
- MuPDF: excellent, but AGPL, which would cover the whole app. Avoid.
- poppler (`pdftotext`) is handy for spot checks, but is not a good
  dependency for the app.

Plan:

- **v1 (about 1–2 days):** a new crate `crates/pdf-parser` producing `ParsedBook`:
  - PDFium text with positions;
  - chapters from the outline;
  - header, footer and page-number removal;
  - joined lines and de-hyphenation;
  - headings guessed from font size;
  - metadata (title, author) from the document info and XMP;
  - the first page rendered as the cover;
  - scanned PDFs detected (no text layer) and reported.
- **v2 (open-ended):** multi-column layouts, footnotes. Optionally, a page view that shows the
  original page with the highlight drawn over it (needs per-glyph boxes), for PDFs with figures or
  maths.
- **Find books:** show PDFs separately and flag likely books (page count, outline, title metadata),
  since most PDFs on a typical disk are not books.
- **Tests:** small synthetic PDFs generated in the tests (AGENTS.md rules). Optional local
  regressions on real PDFs inspect only metadata, the outline and bounded excerpts.

### MOBI / AZW3: feasible and easier than PDF; DRM is the limit

- **MOBI (MOBI 6):** simplified HTML compressed inside a PalmDB container. Chapters come from
  `<mbp:pagebreak/>` markers, the guide, or an NCX index (`filepos` links).
- **AZW3 (KF8):** essentially EPUB-like HTML/CSS split into skeleton and fragment records that must
  be reassembled. Many files contain both a MOBI 6 and a KF8 version.
- **AZW and KFX from the Kindle store** are usually DRM-protected. Aoide will not remove DRM: detect
  it and report "this book is DRM-protected".

Two approaches:

| | How | Pros | Cons |
| --- | --- | --- | --- |
| **A. Native Rust (recommended)** | New `crates/mobi-parser`: container and decompression (the `mobi` crate covers MOBI 6 and metadata), KF8 reassembly, then HTML into blocks | No extra installs; deterministic, like the EPUB parser | Kindle HTML is loose: needs a forgiving HTML parser (e.g. html5ever), not the strict XHTML path the EPUB parser uses |
| B. Convert with Calibre | `ebook-convert` into a temporary EPUB in Aoide's cache, then the EPUB parser; the source is never touched | Least code; Calibre's conversion is excellent | Requires Calibre (about 400 MB; not installed here), an external process, slower imports |

Effort for A: about 1 day for MOBI 6, metadata and cover; about 1 more for AZW3 reassembly and
chapters from the contents index. DRM detection is trivial. *Find books* would show a format badge
and flag DRM-protected files.

## Packaging and storage (decided)

- **SQLite stays embedded.** `rusqlite` with the `bundled` feature compiles SQLite into the app. The
  database is one file (`~/.local/share/com.aoide.audiobooks/library.sqlite3`) and schema
  migrations run on startup. There is no server, so there is nothing to containerise. Caches
  (Open Library lookups, files found by *Find books*), settings and the preparation queue live
  there. Generated audio stays as Opus files on disk, indexed by SQLite.
- **The app should not ship in Docker:** it needs the display, PipeWire audio, D-Bus (MPRIS) and the
  user's documents. Ship native packages instead: Tauri can produce AppImage, RPM and DEB
  (bundling is currently off; turning it on is a config change), with Flatpak possible later.
- **Docker would make sense for:**
  - an optional `docker-compose.yml` that runs the speech servers (Chatterbox on `127.0.0.1:8004`,
    the aligner on `127.0.0.1:8890`) with GPU access through the NVIDIA container toolkit;
  - a build container for reproducible release builds in CI.
