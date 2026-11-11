# EPUB parser

`aoide-epub` is a standalone Rust crate. Its public interface is `parse_epub(bytes, &options) -> Result<ParsedBook, ParseError>`. It has no filesystem, Tauri, network, or speech-generation dependency.

## Behavior

- Reads the EPUB container, package metadata, manifest, and spine.
- Supports EPUB 2 NCX and EPUB 3 navigation, including URI decoding and relative references.
- Follows spine order; splits at chapter anchors within documents and joins continuation documents across chapter ranges.
- Uses root navigation entries as chapters. Nested entries remain subsections of their parent.
- When navigation is absent, uses peer headings at the highest available level. With incomplete navigation, infers missing peer headings only when chapter-level heading evidence exists. Each inferred chapter is reported.
- Extracts visible text, preserves Unicode and paragraph boundaries, and separates table cells. Removes scripts, styles, navigation, hidden attributes, `aria-hidden="true"`, and inline `display:none` / `visibility:hidden` content. External stylesheets are not evaluated.
- Filters structurally identified cover, title/copyright pages, contents, prefaces, introductions, dedications, and acknowledgments. Evidence comes from EPUB 2 guide references, EPUB 3 landmarks, `epub:type`, or document roles. Explicit body matter is preserved. Title matching alone never removes text.
- Returns excluded sections with their text and reason, plus warnings for missing/broken navigation, inferred chapters, ambiguous front-matter titles, and empty sections.
- Records source document paths and available navigation fragments. These are provenance locations, not exact character spans.

`ParseOptions::default()` excludes confirmed front matter. Set `exclude_front_matter: false` to retain it.

## Tests

From the repository root:

```sh
npm test
npm run check:parser

# One public-interface behavior test:
cargo test --manifest-path crates/epub-parser/Cargo.toml --test parser splits_chapters
```

Fixtures are tiny ZIPs generated in memory from original text. Tests exercise the public parser, not private helpers. Test-first changes proceed one behavior at a time.

### Optional checks against your own books

Ignored by default, because books are not distributed with the repository. Point them at any EPUB you own:

```sh
AOIDE_TEST_EPUB=/path/to/book.epub \
AOIDE_TEST_EPUB_CHAPTERS=12 \
cargo test -p aoide-epub --test local_books -- --ignored
```

`AOIDE_TEST_EPUB_CHAPTERS` is optional. The checks cover chapters with text, deterministic output, and front-matter exclusion only removing text. Nothing from the book is printed unless a check fails.

Inspect a local book without printing its body text:

```sh
cargo run --manifest-path crates/epub-parser/Cargo.toml --example inspect -- /path/to/book.epub
```

The example prints metadata, counts, and at most 40 chapter titles/lengths. It does not modify the EPUB.

## Safety and limitations

Default budgets: 64 MiB input ZIP, 10,000 members, 16 MiB per uncompressed member, and 128 MiB total declared uncompressed size. Member reads are also bounded. XML metadata/navigation has a 200,000-node limit; custom entity declarations are rejected. External DTD declarations are accepted without fetching resources. Archive traversal paths, external package references, encrypted reading content, corrupt ZIPs, missing required resources, and malformed metadata return errors. Obfuscated fonts do not prevent extracting unencrypted text.

This is an initial structural parser, not a universal EPUB repair tool:

- Inputs must have UTF-8 XML/HTML resources and supported ZIP compression (stored/deflate).
- Styling-only chapter labels (ordinary paragraphs rather than headings/anchors) are not inferred. Books whose NCX navigation is incomplete and whose chapter labels are unmarked Roman numerals cannot yet be fully split into chapters. Text is retained, but several chapters remain grouped.
- Nested parts are kept as root navigation groups; distinguishing parts from chapters within those groups needs a future explicit policy.
- Unmarked front matter is retained. A later review UI can expose exclusions/warnings and offer per-book overrides.
- Folder watching, Tauri commands, audio generation, caching, and playback are outside this crate and are not implemented by this work.
