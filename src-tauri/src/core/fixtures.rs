//! Fixtures for the UI's mock: the views the core makes for the browser preview's library,
//! written to `src/tests/fixtures/` by `AOIDE_WRITE_FIXTURES=1 cargo test -p aoide fixtures`.
//! The core's ids become the preview's (`book-0`…) and temporary paths become `~`.

use std::io::Write as _;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use aoide_tts::testing::FakeEngine;
use serde::Serialize;
use serde_json::Value;

use super::tests::{start_with, until};
use super::Core;

/// Whether fixtures are being written.
pub(super) fn wanted() -> bool {
    std::env::var_os("AOIDE_WRITE_FIXTURES").is_some()
}

/// Writes `value` as `src/tests/fixtures/{name}.json`, each `(from, to)` of `renames` replaced.
pub(super) fn write(name: &str, value: &impl Serialize, renames: &[(String, String)]) {
    let mut text = serde_json::to_string_pretty(value).unwrap();
    for (from, to) in renames {
        text = text.replace(from.as_str(), to);
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/tests/fixtures");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.json")), text + "\n").unwrap();
}

/// A preview book: its title, author and chapters (a heading, then paragraphs).
struct Book {
    title: &'static str,
    author: &'static str,
    /// Its `dc:subject` entries (they become its tags).
    subjects: &'static [&'static str],
    chapters: &'static [&'static [&'static str]],
    /// Adds a copyright page (left out) and an empty section (skipped), for extraction notes.
    notes: bool,
    /// The text of footnote `n1`, kept at the end of the first chapter (its marker is in the
    /// chapter's text).
    footnote: Option<&'static str>,
}

/// The browser preview's books, with original text.
const BOOKS: [Book; 4] = [
    Book {
        title: "The Lantern Keeper",
        author: "Mira Vale",
        subjects: &["Fiction / Sea Stories / General", "Lighthouses"],
        chapters: &[
            &[
                "The small light",
                "Every evening, Ada<a epub:type=\"noteref\" href=\"#n1\">1</a> carried a lantern to the end of the pier. The sea collected its reflection and broke it into a hundred little stars.",
                "Tonight a paper boat waited beside the steps. Inside it lay a key, warm as if someone had held it all the way across the bay.",
            ],
            &[
                "Across the water",
                "At dawn she borrowed the blue rowing boat. Each pull of the oars opened a quiet doorway in the water.",
                "On the far shore, a gardener waved. He had been waiting for someone to bring back the light.",
            ],
        ],
        notes: false,
        footnote: Some("A short original note shown in the browser preview."),
    },
    Book {
        title: "A Map of Quiet Places",
        author: "Ellis Reed",
        subjects: &["Fiction / Literary", "Cities"],
        chapters: &[
            &[
                "An ordinary discovery",
                "No one had marked the bench on any map. It stood between a bakery and a clock shop, where the morning smelled of bread and rain.",
                "Ren sat down and listened. Beneath the traffic, a sparrow was rehearsing a song with no ending.",
            ],
            &[
                "The long way home",
                "Ren drew a circle where the bench should be. Then came the bridge, the cool stone steps, and the lane where the wind always rested.",
                "The map did not make the city smaller. It made room for all the things Ren had hurried past.",
            ],
            &[
                "A place to begin",
                "The next morning, a stranger asked for directions. Ren folded the map and offered to walk beside her.",
                "They took the long way. There was plenty of time.",
            ],
            &[
                "About the author",
                "Ellis Reed walks a different street every morning and writes down what was quiet there.",
            ],
        ],
        notes: true,
        footnote: None,
    },
    Book {
        title: "The Absent Orchard",
        author: "Ellis Reed",
        subjects: &["Fiction / Literary"],
        chapters: &[&[
            "The gate",
            "The orchard had been sold twice, and neither owner had ever seen it.",
        ]],
        notes: false,
        footnote: None,
    },
    Book {
        title: "An Unfinished Atlas",
        author: "Ellis Reed",
        subjects: &["Travel"],
        chapters: &[&[
            "Blank pages",
            "Half the atlas was empty, and the cartographer liked it best that way.",
        ]],
        notes: false,
        footnote: None,
    },
];

/// Books the preview's "Find books" finds: one new with two copies and its audiobook beside it,
/// one new on its own.
const FOUND: [Book; 2] = [
    Book {
        title: "Notes on the Tide",
        author: "Ines Morrow",
        subjects: &["Sea Stories"],
        chapters: &[&[
            "High water",
            "The tide came in twice a day, and twice a day she wrote it down.",
        ]],
        notes: false,
        footnote: None,
    },
    Book {
        title: "The Glass Meridian",
        author: "Oren Vale",
        subjects: &["Science Fiction"],
        chapters: &[&[
            "Noon",
            "At noon the meridian line burned white across the floor.",
        ]],
        notes: false,
        footnote: None,
    },
];

/// The preview's audiobooks folder, in real audio (the recording tests' files): parts named for
/// the second book (a folder of 15, and one file), the fourth (two files, a name only close to
/// the book's), and one that comes
/// close to no book.
const AUDIOBOOKS: [(&str, usize, &str); 4] = [
    (
        "Ellis Reed - A Map of Quiet Places/{n} - A Map of Quiet Places.m4b",
        15,
        "book.m4b",
    ),
    ("A Map of Quiet Places (Unabridged).m4b", 1, "book.m4b"),
    ("Unfinished Atlas Radio Play {n}.mp3", 2, "set/b-first.mp3"),
    (
        "Ines Morrow - Winter Radio/0{n} Part.mp3",
        2,
        "set/b-first.mp3",
    ),
];

/// Lays out the preview's folders to search: `Downloads` and `Documents` with EPUBs (one already
/// in the library, with its audiobook beside it, and one unreadable), `Audiobooks` with audio. Returns the found books' ids.
fn preview_folders(root: &Path) -> [String; 2] {
    let write = |path: &Path, bytes: &[u8]| {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    };
    let audio = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/recording/tests/fixtures");
    let copy = |to: &Path, from: &str| {
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(audio.join(from), to).unwrap();
    };
    let tide = epub(&FOUND[0]);
    write(&root.join("Downloads/notes-on-the-tide.epub"), &tide);
    write(&root.join("Documents/Books/tide.epub"), &tide);
    copy(
        &root.join("Downloads/Notes on the Tide (Unabridged).m4b"),
        "book.m4b",
    );
    let glass = epub(&FOUND[1]);
    write(&root.join("Downloads/glass-meridian.epub"), &glass);
    write(&root.join("Documents/lantern.epub"), &epub(&BOOKS[0]));
    copy(&root.join("Documents/The Lantern Keeper.m4b"), "book.m4b");
    write(&root.join("Documents/broken.epub"), b"not an EPUB");
    for (pattern, parts, from) in AUDIOBOOKS {
        for n in 1..=parts {
            let name = if parts > 9 {
                pattern.replace("{n}", &format!("{n:02}"))
            } else {
                pattern.replace("{n}", &n.to_string())
            };
            copy(&root.join("Audiobooks").join(name), from);
        }
    }
    [tide, glass].map(|bytes| aoide_library::content_hash(&bytes).0)
}

/// The preview's unreadable book.
const BROKEN: &str = "The Torn Ledger";

fn epub(book: &Book) -> Vec<u8> {
    let xhtml = |body: String| {
        format!(
            r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>x</title></head><body>{body}</body></html>"#
        )
    };
    let mut files: Vec<(String, String)> = vec![
        ("mimetype".into(), "application/epub+zip".into()),
        ("META-INF/container.xml".into(), r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.into()),
    ];
    let mut manifest = String::from(
        r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
    );
    let mut spine = String::new();
    let mut toc = String::new();
    if book.notes {
        manifest.push_str(
            r#"<item id="rights" href="rights.xhtml" media-type="application/xhtml+xml"/>"#,
        );
        spine.push_str(r#"<itemref idref="rights"/>"#);
        toc.push_str(r#"<li><a href="rights.xhtml">Copyright</a></li>"#);
        files.push((
            "OPS/rights.xhtml".into(),
            xhtml(r#"<section epub:type="copyright-page"><h1>Copyright</h1><p>All rights kept by the preview.</p></section>"#.into()),
        ));
    }
    for (index, parts) in book.chapters.iter().enumerate() {
        let name = format!("c{index}.xhtml");
        manifest.push_str(&format!(
            r#"<item id="c{index}" href="{name}" media-type="application/xhtml+xml"/>"#
        ));
        spine.push_str(&format!(r#"<itemref idref="c{index}"/>"#));
        toc.push_str(&format!(r#"<li><a href="{name}">{}</a></li>"#, parts[0]));
        let mut body: String = std::iter::once(format!("<h2>{}</h2>", parts[0]))
            .chain(parts[1..].iter().map(|p| format!("<p>{p}</p>")))
            .collect();
        if let (0, Some(note)) = (index, book.footnote) {
            body.push_str(&format!(
                r#"<aside epub:type="footnote" id="n1"><p>{note}</p></aside>"#
            ));
        }
        files.push((format!("OPS/{name}"), xhtml(body)));
    }
    if book.notes {
        manifest.push_str(
            r#"<item id="blank" href="blank.xhtml" media-type="application/xhtml+xml"/>"#,
        );
        spine.push_str(r#"<itemref idref="blank"/>"#);
        toc.push_str(r#"<li><a href="blank.xhtml">Blank</a></li>"#);
        files.push(("OPS/blank.xhtml".into(), xhtml(String::new())));
    }
    files.push((
        "OPS/nav.xhtml".into(),
        xhtml(format!(r#"<nav epub:type="toc"><ol>{toc}</ol></nav>"#)),
    ));
    files.push((
        "OPS/book.opf".into(),
        format!(
            r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{}</dc:title><dc:creator>{}</dc:creator><dc:language>en</dc:language>{}</metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>"#,
            book.title,
            book.author,
            book.subjects
                .iter()
                .map(|s| format!("<dc:subject>{s}</dc:subject>"))
                .collect::<String>()
        ),
    ));
    // Stored and dated 1980, so the bytes (and the ids made from them) are the same everywhere.
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .last_modified_time(zip::DateTime::default());
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, body) in files {
        zip.start_file(name, options).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// The core over the preview's books, one of them gone missing since it was scanned and one
/// unreadable. Returns the renames from the core's ids and folders to the preview's.
async fn preview_library(root: &Path) -> (Arc<Core>, Vec<(String, String)>) {
    std::fs::create_dir_all(root.join("Books")).unwrap();
    std::fs::create_dir_all(root.join("config")).unwrap();
    std::fs::write(
        root.join("config/settings.toml"),
        "[engine]\nid = \"test\"\nname = \"Local Chatterbox\"\nkind = \"fake\"\n\
         baseUrl = \"http://127.0.0.1:9\"\nnetwork = \"loopback\"\nvoiceId = \"Emily.wav\"\n",
    )
    .unwrap();
    let mut renames = Vec::new();
    for (index, book) in BOOKS.iter().enumerate() {
        let bytes = epub(book);
        std::fs::write(root.join(format!("Books/{}.epub", book.title)), &bytes).unwrap();
        renames.push((
            aoide_library::content_hash(&bytes).0,
            format!("book-{index}"),
        ));
    }
    let broken = b"not an EPUB".to_vec();
    std::fs::write(root.join(format!("Books/{BROKEN}.epub")), &broken).unwrap();
    renames.push((aoide_library::content_hash(&broken).0, "book-4".into()));
    let (core, _events) = start_with(
        root,
        &FakeEngine::new(),
        Arc::new(|_| Err("no speech recognition in the preview".into())),
    );
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 5)
    })
    .await;
    if wanted() {
        // Every preview book's chapters as the core reads them, the third's before it goes.
        let mut ids: Vec<String> = core
            .list_books()
            .unwrap()
            .into_iter()
            .map(|b| b.id)
            .collect();
        let index = |id: &String| renames.iter().position(|(hash, _)| hash == id);
        ids.retain(|id| index(id).is_some_and(|i| i < BOOKS.len()));
        ids.sort_by_key(index);
        let chapters: Vec<Vec<crate::book::ChapterView>> = ids
            .iter()
            .map(|id| {
                let count = core.book_overview(id).unwrap().chapters.len();
                (0..count).map(|n| core.chapter(id, n).unwrap()).collect()
            })
            .collect();
        write("chapters", &chapters, &renames);
    }
    std::fs::remove_file(root.join(format!("Books/{}.epub", BOOKS[2].title))).unwrap();
    core.rescan().unwrap();
    renames.push((root.to_string_lossy().into_owned(), "~".into()));
    (core, renames)
}

#[tokio::test]
async fn fixtures_for_the_ui_mock() {
    if !wanted() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (core, renames) = preview_library(dir.path()).await;
    let preview = |id: &str| {
        renames
            .iter()
            .find(|(from, _)| from == id)
            .map_or_else(|| id.to_owned(), |(_, to)| to.clone())
    };

    // The first book is listened to; the others are read silently, as new books are.
    let mut ids: Vec<String> = core
        .list_books()
        .unwrap()
        .into_iter()
        .map(|b| b.id)
        .collect();
    ids.sort_by_key(|id| preview(id));
    core.set_mode(&ids[0], super::ReadingMode::Listen).unwrap();
    // Each readable book has been opened once, so its length is known.
    for id in &ids {
        let _ = core.book_overview(id);
    }
    // The library as first scanned, in the preview's order, added one after another.
    let mut books = core.list_books().unwrap();
    books.sort_by_key(|book| preview(&book.id));
    for (index, book) in books.iter_mut().enumerate() {
        book.added_at = 100 + index as i64;
        book.last_opened_at = None;
    }
    // The reader has started the first book, and some of its audio is made.
    let started = &mut books[0];
    started.progress = Some(super::BookProgressView {
        chapter: 0,
        segment_id: Some("0:1:0".into()),
        fraction: 0.12,
        position_ms: None,
    });
    started.audio_bytes = 2_457_600;
    started.last_opened_at = Some(500);
    started.read_state = super::ReadState::InProgress;
    started.audio = super::facets::audio_source(super::facets::AudioFacts {
        cached_speech: true,
        ..Default::default()
    });
    write("library", &books, &renames);
    write("home", &super::facets::home(&books), &renames);

    let views: serde_json::Map<String, Value> = books
        .iter()
        .filter(|book| matches!(book.status, aoide_library::BookStatus::Present))
        .map(|book| {
            let view = core.book_overview(&book.id).unwrap();
            (preview(&book.id), serde_json::to_value(view).unwrap())
        })
        .collect();
    write("book-views", &views, &renames);

    write(
        "player",
        &serde_json::json!({
            "stopped": core.player_state(),
            "at": listening_times(&core, &books),
        }),
        &renames,
    );
    write("generation", &core.generation_state(), &renames);

    let first = &books[0].id;
    core.play(first, Some(0), None).await.unwrap();
    until("the speech near the playhead", || {
        let state = core.generation_state();
        state.book_id.is_some() && state.pending == 0
    })
    .await;
    write("generation-playing", &core.generation_state(), &renames);
    core.stop();

    // An audiobook added on its own, which the mock adds when asked.
    let folder = super::audiobook::tests::tagged_set(dir.path());
    let id = core
        .add_audiobook(vec![folder.to_string_lossy().into()])
        .book_id
        .unwrap();
    let mut renames = renames.clone();
    renames.insert(0, (id.clone(), "book-5".into()));
    // Its tags name a preview book; the preview tells them apart.
    for (from, to) in [
        ("The Lantern Keeper", "Winter Radio"),
        ("Lantern_Keeper", "Winter_Radio"),
        ("A. Writer", "Ines Morrow"),
    ] {
        renames.push((from.into(), to.into()));
    }
    let mut summary = core
        .list_books()
        .unwrap()
        .into_iter()
        .find(|b| b.id == id)
        .unwrap();
    summary.added_at = 200;
    write(
        "audiobook",
        &serde_json::json!({
            "summary": summary,
            "recording": core.recording(&id),
            "view": core.book_overview(&id).unwrap(),
        }),
        &renames,
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
}

/// The times left the player's state gives at the start of each sentence of each book read by
/// the speech engine, at 1x and before any speech is made: `{book: {segment: {chapterLeftMs,
/// bookLeftMs, chapterMs}}}`. The mock plays books with them instead of estimating.
fn listening_times(core: &Core, books: &[super::BookSummary]) -> Value {
    let mut times = serde_json::Map::new();
    for summary in books {
        let Ok(book) = core.load_book(&summary.id) else {
            continue;
        };
        let Ok(target) = core.speech_target(Arc::clone(&book)) else {
            continue;
        };
        let playback = super::playback::Playback::new(aoide_audio::PlayerCore::new(24_000));
        let _ = playback.start(target, 0);
        let mut at = serde_json::Map::new();
        for index in 0..book.total_entries() {
            let _ = playback.seek(index, Some(0));
            let state = playback.state();
            let Some(segment) = state.segment_id else {
                continue;
            };
            at.insert(
                segment,
                serde_json::json!({
                    "chapterLeftMs": state.chapter_left_ms,
                    "bookLeftMs": state.book_left_ms,
                    "chapterMs": state.chapter_ms,
                }),
            );
        }
        times.insert(summary.id.clone(), Value::Object(at));
    }
    Value::Object(times)
}

/// Chapters with their display text, for the UI's tests of what a selection on the page covers:
/// every preview book's chapters (footnotes read aloud) and a chapter of every shape the page
/// draws (`book::tests::every_shape`).
#[test]
fn chapter_texts_for_the_ui_tests() {
    use crate::book::{BookModel, Layout};
    if !wanted() {
        return;
    }
    let layout = Layout {
        read_notes: true,
        ..Layout::default()
    };
    let mut models: Vec<BookModel> = BOOKS
        .iter()
        .map(|book| BookModel::parse("b", &epub(book), &layout).unwrap())
        .collect();
    models.push(crate::book::tests::every_shape());
    let chapters: Vec<Value> = models
        .iter()
        .flat_map(|model| {
            (0..model.chapters.len()).map(|n| {
                serde_json::json!({
                    "text": model.chapters[n].text,
                    "chapter": model.chapter_view(n).unwrap(),
                })
            })
        })
        .collect();
    write("chapter-texts", &chapters, &[]);
}

/// An article saved and fetched, which the mock adds when a link is saved: its summary, its
/// view, its overview and its text, as `book-6`.
#[tokio::test]
async fn an_article_for_the_ui_mock() {
    use super::article_tests::{Pages, LIGHTS, LIGHTS_URL};
    if !wanted() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    super::tests::library_folder(dir.path(), "");
    let (core, _) = super::tests::start_fetching(
        dir.path(),
        &FakeEngine::new(),
        Pages::with(&[(LIGHTS_URL, LIGHTS)]),
    );
    let id = core.save_article(LIGHTS_URL).unwrap();
    until("the article is fetched", || {
        core.article(&id)
            .is_ok_and(|a| a.stage == super::fetching::Stage::Ready)
    })
    .await;
    let mut summary = core
        .list_books()
        .unwrap()
        .into_iter()
        .find(|b| b.id == id)
        .unwrap();
    summary.added_at = 210;
    let mut article = core.article(&id).unwrap();
    article.fetched_at = Some(205);
    let renames = vec![
        (id.clone(), "book-6".to_owned()),
        (dir.path().to_string_lossy().into_owned(), "~".to_owned()),
    ];
    write(
        "article",
        &serde_json::json!({
            "summary": summary,
            "article": article,
            "view": core.book_overview(&id).unwrap(),
            "chapter": core.chapter(&id, 0).unwrap(),
        }),
        &renames,
    );
}

/// The preview's web beyond its feeds, as `pages` serves it: a site with a new feed, which its
/// front page names with a second one, a page naming none, and the feed of the preview
/// article's site. Written as `web.json`, so the mock knows what is there.
const WEB_FEEDS: [(&str, &str); 3] = [
    ("https://lanterns.example/feed.xml", "Lantern Notes"),
    (
        "https://lanterns.example/comments/feed.xml",
        "Lantern Notes comments",
    ),
    ("https://quay.example/feed.xml", "Quay Weekly"),
];

fn preview_web(pages: &super::article_tests::Pages) {
    for (url, title) in WEB_FEEDS {
        let site = url::Url::parse(url).unwrap().join("/").unwrap();
        pages.feed(
            url,
            &format!(
                "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>{title}</title>\
                 <link>{site}</link></channel></rss>"
            ),
            "v1",
        );
    }
    pages.set(
        "https://lanterns.example/",
        Ok("<!doctype html><html><head><title>Lanterns</title>\
            <link rel=\"alternate\" type=\"application/rss+xml\" title=\"Articles\" href=\"/feed.xml\">\
            <link rel=\"alternate\" type=\"application/rss+xml\" title=\"Comments\" href=\"/comments/feed.xml\">\
            </head><body><p>Lanterns, kept and lit.</p></body></html>"
            .into()),
    );
    pages.set(
        "https://lanterns.example/about",
        Ok("<!doctype html><html><head><title>About</title></head><body><p>About the lanterns.</p></body></html>".into()),
    );
}

/// The core the mock's feeds come from: four subscriptions, checked (the fourth failing), two
/// entries read. Returns it with its folder and when "now" was.
async fn feeds_core() -> (
    Arc<Core>,
    tempfile::TempDir,
    i64,
    Arc<super::article_tests::Pages>,
) {
    use super::article_tests::Pages;
    use super::feeds::{EntryFilter, EntryQuery};
    const HOUR: i64 = 60 * 60 * 1000;
    let now = super::now_ms() / 1000 * 1000;
    let date = |hours: i64| {
        chrono::DateTime::from_timestamp_millis(now - hours * HOUR)
            .unwrap()
            .to_rfc2822()
    };
    let long = |opening: &str| {
        let more = "The tide sets the hour here, and the harbour keeps the record of it. ";
        format!("<p>{opening}</p><p>{}</p>", more.repeat(20))
    };
    // (feed address, title, site, entries: (title, hours ago, summary, whole text))
    type Item<'a> = (&'a str, i64, &'a str, bool);
    let feeds: [(&str, &str, &str, &[Item]); 4] = [
        (
            "https://tidewatch.example/feed.xml",
            "Tidewatch",
            "https://tidewatch.example/",
            &[
                ("The ferry that runs on the tide table", 2, "There is no fixed departure. The skipper reads the water at the slip each morning and chalks a time on the board.", true),
                ("How lighthouses kept their own time", 26, "Before the telegraph reached the headland, the keeper set the clock by the mail boat.", true),
                ("Charts drawn by the harbour pilots", 50, "Every sandbank in the estuary has a name, and most of them have two.", true),
            ],
        ),
        (
            "https://smallcities.example/rss",
            "Small Cities",
            "https://smallcities.example/",
            &[
                ("Who decides where the bus shelter goes", 5, "A committee of four, a spreadsheet from 2011, and a rule about sight lines.", false),
                ("The last public clock in town", 28, "It has been four minutes slow for a decade, and the council has voted twice to leave it alone.", false),
            ],
        ),
        (
            "https://fieldnotes.example/feed.xml",
            "Field Notes",
            "https://fieldnotes.example/",
            &[("The quiet return of the night train", 7, "Sleeper carriages are back on the coast line, and they are full.", true)],
        ),
        (
            "https://harbourletters.example/feed.xml",
            "Harbour Letters",
            "https://harbourletters.example/",
            &[("Letters from the breakwater", 80, "A reader writes about the winter storms.", false)],
        ),
    ];
    let pages = Pages::with(&[]);
    for (url, title, site, items) in feeds {
        let mut xml = format!(
            "<?xml version=\"1.0\"?><rss version=\"2.0\" \
             xmlns:content=\"http://purl.org/rss/1.0/modules/content/\"><channel>\
             <title>{title}</title><link>{site}</link>"
        );
        for (i, (headline, hours, summary, whole)) in items.iter().enumerate() {
            let content = if *whole {
                format!(
                    "<content:encoded><![CDATA[{}]]></content:encoded>",
                    long(summary)
                )
            } else {
                String::new()
            };
            xml.push_str(&format!(
                "<item><title>{headline}</title><guid>{site}{i}</guid><link>{site}{i}</link>\
                 <pubDate>{}</pubDate><description>{summary}</description>{content}</item>",
                date(*hours)
            ));
        }
        xml.push_str("</channel></rss>");
        pages.feed(url, &xml, "v1");
    }
    let dir = tempfile::tempdir().unwrap();
    super::tests::library_folder(dir.path(), "");
    let (core, _) = super::tests::start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
    for (url, ..) in feeds {
        core.subscribe(url).await.unwrap();
    }
    pages.feed_fails(feeds[3].0, aoide_fetch::FetchError::Status(503));
    core.check_feeds().await;
    let all = |core: &Core| {
        core.feed_entries(&EntryQuery {
            filter: EntryFilter::All,
            ..EntryQuery::default()
        })
        .unwrap()
    };
    let lighthouses = all(&core)
        .into_iter()
        .find(|e| e.title.as_deref() == Some("How lighthouses kept their own time"))
        .unwrap();
    core.mark_entry_read(lighthouses.id, true).unwrap();
    let old = all(&core).into_iter().find(|e| e.feed_id == 4).unwrap();
    core.mark_entry_read(old.id, true).unwrap();
    (core, dir, now, pages)
}

/// Subscriptions and their entries, which the mock serves on its Feeds screen: three feeds
/// that answer (one carrying whole articles, one summaries only) and one that stopped. Times
/// are written relative to now (the mock adds its own now).
#[tokio::test]
async fn feeds_for_the_ui_mock() {
    use super::feeds::{EntryFilter, EntryQuery, FeedStage};
    const HOUR: i64 = 60 * 60 * 1000;
    if !wanted() {
        return;
    }
    let (core, _dir, now, _) = feeds_core().await;
    let all = |core: &Core| {
        core.feed_entries(&EntryQuery {
            filter: EntryFilter::All,
            ..EntryQuery::default()
        })
        .unwrap()
    };
    let mut views = core.feeds().unwrap();
    for view in &mut views {
        view.last_ok_at = Some(if view.id == 4 {
            -3 * 24 * HOUR
        } else {
            -20 * 60_000
        });
        if let FeedStage::Failing { since_ms } = &mut view.stage {
            *since_ms = -2 * 24 * HOUR;
        }
    }
    let mut entries = all(&core);
    for entry in &mut entries {
        entry.at -= now;
    }
    write(
        "feeds",
        &serde_json::json!({ "feeds": views, "entries": entries }),
        &[],
    );
}

/// The stage machines the mock plays out: for each stage, where each event the UI can cause
/// leads, and what each stage offers. Written from [`super::syncing`] and
/// [`super::preparation`], so the mock never keeps its own copy of their rules.
/// The themes that come with Aoide, as the preferences list them.
#[test]
fn themes_for_the_ui_mock() {
    if !wanted() {
        return;
    }
    let none = tempfile::tempdir().unwrap();
    write("themes", &aoide_config::themes(none.path()), &[]);
}

#[test]
fn stage_machines_for_the_ui_mock() {
    if !wanted() {
        return;
    }
    use super::preparation::{self, Event as PrepareEvent, Stage as PrepareStage};
    use super::syncing::{self, Event as SyncEvent, Stage as SyncStage};
    fn name(value: impl Serialize) -> String {
        serde_json::to_value(value)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    }
    let sync_stages = [
        SyncStage::Checking,
        SyncStage::Waiting,
        SyncStage::NeedsModel,
        SyncStage::Transcribing,
        SyncStage::Matching,
        SyncStage::Ready,
        SyncStage::SampleMismatch,
        SyncStage::PoorMatch,
        SyncStage::Paused,
        SyncStage::Missing,
        SyncStage::Failed,
        SyncStage::Standalone,
    ];
    let sync_events = [
        ("pause", SyncEvent::Pause),
        ("resume", SyncEvent::Resume),
        ("retry", SyncEvent::Retry),
        ("accept", SyncEvent::Accept),
        ("resync", SyncEvent::Resync),
        ("promote", SyncEvent::Promote),
    ];
    let mut sync = serde_json::Map::new();
    for stage in sync_stages {
        let next: serde_json::Map<String, Value> = sync_events
            .iter()
            .map(|(event, e)| ((*event).to_owned(), name(syncing::next(stage, *e)).into()))
            .collect();
        sync.insert(
            name(stage),
            serde_json::json!({
                "next": next,
                "actions": syncing::actions(stage, false),
                "listenableActions": syncing::actions(stage, true),
                // Whether it can be listened to with none of it transcribed, and with some.
                "listenable": [syncing::can_listen(stage, 0), syncing::can_listen(stage, 1)],
            }),
        );
    }
    let prepare_stages = [
        PrepareStage::Waiting,
        PrepareStage::Queued,
        PrepareStage::Preparing,
        PrepareStage::Paused,
        PrepareStage::Done,
    ];
    let prepare_events = [
        ("planned", PrepareEvent::Planned),
        ("pause", PrepareEvent::Pause),
        ("resume", PrepareEvent::Resume),
        ("promote", PrepareEvent::Promote),
        ("demote", PrepareEvent::Demote),
    ];
    let mut prepare = serde_json::Map::new();
    for stage in prepare_stages {
        let next: serde_json::Map<String, Value> = prepare_events
            .iter()
            .map(|(event, e)| {
                (
                    (*event).to_owned(),
                    name(preparation::next(stage, *e)).into(),
                )
            })
            .collect();
        prepare.insert(
            name(stage),
            serde_json::json!({
                "next": next,
                "actions": preparation::actions(stage, false),
                "someChaptersActions": preparation::actions(stage, true),
            }),
        );
    }
    write(
        "stages",
        &serde_json::json!({ "sync": sync, "prepare": prepare }),
        &[],
    );
}

/// A scenario run on the real core: each call the window could make (an `AoideApi` method and
/// its arguments) and what the core answered, for the UI mock to replay
/// (`src/tests/mock.test.ts`).
struct Scenario {
    name: &'static str,
    /// Where the mock starts: the preview's address query (`""` for the preview as it opens),
    /// matching the state the core was in.
    start: &'static str,
    steps: Vec<Value>,
}

impl Scenario {
    fn new(name: &'static str, start: &'static str) -> Self {
        Self {
            name,
            start,
            steps: Vec::new(),
        }
    }

    /// `call(args)` (its arguments named as the window sends them) answered `result`; the replay compares the values at `check`, paths into
    /// the result (see [`pick`]), or the refusal's message.
    fn step<T: Serialize>(
        &mut self,
        call: &str,
        args: Value,
        result: &Result<T, String>,
        check: &[&str],
    ) {
        let (expect, error) = match result {
            Ok(value) => {
                let value = serde_json::to_value(value).unwrap();
                let expect: serde_json::Map<String, Value> = check
                    .iter()
                    .map(|path| ((*path).to_owned(), pick(&value, path)))
                    .collect();
                (expect, None)
            }
            Err(error) => (serde_json::Map::new(), Some(error.clone())),
        };
        self.steps.push(serde_json::json!({
            "call": call, "args": args, "expect": expect, "error": error,
        }));
    }

    /// After the last step the window is sent event `name` (an `AoideApi` `on…` method): the
    /// replay compares its last payload at `check`.
    fn then<T: Serialize>(&mut self, name: &str, payload: &T, check: &[&str]) {
        let value = serde_json::to_value(payload).unwrap();
        let expect: serde_json::Map<String, Value> = check
            .iter()
            .map(|path| ((*path).to_owned(), pick(&value, path)))
            .collect();
        let step = self.steps.last_mut().unwrap();
        step["event"] = serde_json::json!({ "name": name, "expect": expect });
    }

    fn done(self) -> Value {
        serde_json::json!({ "name": self.name, "start": self.start, "steps": self.steps })
    }
}

/// The value at `path` in `value`: dot-separated keys, array indexes, `length` (an array's),
/// `[k=x]` (the element whose field `k` is `x`) and `has=x` (whether an array holds the text `x`);
/// an empty path is the whole value. The mock's
/// replay reads paths the same way.
fn pick(value: &Value, path: &str) -> Value {
    let mut at = value.clone();
    // Dots inside `[…]` (a web address) do not split.
    let mut parts = Vec::new();
    let (mut depth, mut from) = (0, 0);
    for (i, c) in path.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            '.' if depth == 0 => {
                parts.push(&path[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&path[from..]);
    for part in parts.into_iter().filter(|p| !p.is_empty()) {
        let field = part
            .strip_prefix('[')
            .and_then(|p| p.strip_suffix(']'))
            .and_then(|p| p.split_once('='));
        at = if let Some((key, id)) = field {
            at.as_array()
                .and_then(|items| {
                    items.iter().find(|item| match &item[key] {
                        Value::String(text) => text == id,
                        other => other.as_i64().is_some_and(|n| id.parse() == Ok(n)),
                    })
                })
                .cloned()
                .unwrap_or(Value::Null)
        } else if let Some(text) = part.strip_prefix("has=") {
            at.as_array()
                .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(text)))
                .into()
        } else if part == "length" && at.is_array() {
            at.as_array().unwrap().len().into()
        } else if let (Ok(index), Some(items)) = (part.parse::<usize>(), at.as_array()) {
            items.get(index).cloned().unwrap_or(Value::Null)
        } else {
            at.get(part).cloned().unwrap_or(Value::Null)
        };
    }
    at
}

/// Scenarios run on the real core, which the mock replays and must answer alike: every rule
/// the mock plays out (reading state, places, modes, collections, search, removal, links,
/// feeds and their entries) is held to what the core did.
#[tokio::test]
async fn scenarios_for_the_ui_mock() {
    use serde_json::json;
    if !wanted() {
        return;
    }
    let mut scenarios = Vec::new();

    // ---- on the preview library, as the mock starts
    let dir = tempfile::tempdir().unwrap();
    let (core, renames) = preview_library(dir.path()).await;
    let id_of = |preview: &str| {
        renames
            .iter()
            .find(|(_, to)| to == preview)
            .map(|(from, _)| from.clone())
            .unwrap()
    };
    let (first, second, fourth) = (id_of("book-0"), id_of("book-1"), id_of("book-3"));
    core.set_mode(&first, super::ReadingMode::Listen).unwrap();
    let books = |core: &Core| core.list_books();

    let mut s = Scenario::new(
        "a book is finished, read again and read in places",
        "library",
    );
    let book = |field: &str| format!("[id={second}].{field}");
    let (state, progress) = (book("readState"), book("progress"));
    // Where Home lists it: membership, which the preview's other books do not change.
    let listed = [
        format!("continueIds.has={second}"),
        format!("upNextIds.has={second}"),
        format!("recentIds.has={second}"),
    ];
    let listed: Vec<&str> = listed.iter().map(String::as_str).collect();
    s.step("home", json!({}), &core.home(), &listed);
    let finished = core.mark_finished(&second);
    s.step("markFinished", json!({"bookId": second}), &finished, &[]);
    s.step("listBooks", json!({}), &books(&core), &[&state]);
    s.step("home", json!({}), &core.home(), &listed);
    s.step(
        "markUnread",
        json!({"bookId": second}),
        &core.mark_unread(&second),
        &[],
    );
    s.step("listBooks", json!({}), &books(&core), &[&state, &progress]);
    s.step("home", json!({}), &core.home(), &listed);
    let chapters = core.book_overview(&second).unwrap().chapters.len();
    let spoken: Vec<(usize, String)> = (0..chapters)
        .flat_map(|n| {
            let chapter = core.chapter(&second, n).unwrap();
            chapter
                .blocks
                .into_iter()
                .flat_map(|b| b.segments)
                .filter(|s| s.speakable)
                .map(move |s| (n, s.id))
        })
        .collect();
    let places = [
        spoken[0].clone(),
        spoken[spoken.len() / 2].clone(),
        spoken[spoken.len() - 1].clone(),
        (spoken[0].0 + 1, spoken[0].1.clone()),
    ];
    for (chapter, segment) in places {
        let placed = core.set_reading_place(&second, chapter, &segment);
        s.step(
            "setReadingPlace",
            json!({"bookId": second, "chapter": chapter, "segmentId": segment}),
            &placed,
            &["fraction"],
        );
        s.step("listBooks", json!({}), &books(&core), &[&state]);
        s.step("home", json!({}), &core.home(), &listed);
    }
    s.step(
        "chapter",
        json!({"bookId": second, "chapter": 99}),
        &core.chapter(&second, 99),
        &[],
    );
    s.step(
        "chapter",
        json!({"bookId": second, "chapter": 1}),
        &core.chapter(&second, 1),
        &["title", "blocks.length"],
    );
    scenarios.push(s.done());

    let mut s = Scenario::new("a book is read or listened to", "");
    for mode in [super::ReadingMode::Listen, super::ReadingMode::Read] {
        let set = core.set_mode(&second, mode);
        s.step(
            "setMode",
            json!({"bookId": second, "mode": mode}),
            &set,
            &["mode", "shape"],
        );
        s.step(
            "listBooks",
            json!({}),
            &books(&core),
            &[&book("mode"), &book("shape")],
        );
    }
    scenarios.push(s.done());

    let mut s = Scenario::new("chapters are left out of the narration", "");
    let narrated = [
        "chapters.0.leftOut",
        "chapters.3.leftOut",
        "chapters.3.suggested",
        "chaptersLeftOut",
        "narratedWordCount",
        "wordCount",
    ];
    s.step(
        "bookOverview",
        json!({"bookId": second}),
        &core.book_overview(&second),
        &narrated,
    );
    s.step(
        "setLeftOut",
        json!({"bookId": second, "chapters": [3, 3, 9]}),
        &core.set_left_out(&second, vec![3, 3, 9]).await,
        &narrated,
    );
    s.step(
        "bookOverview",
        json!({"bookId": second}),
        &core.book_overview(&second),
        &narrated,
    );
    let older = super::BookSettings {
        left_out: vec![],
        ..core.book_settings(&second)
    };
    s.step(
        "saveBookSettings",
        json!({"bookId": second, "settings": older}),
        &core.save_book_settings(&second, older.clone()).await,
        &[],
    );
    s.step(
        "bookSettings",
        json!({"bookId": second}),
        &Ok::<_, String>(core.book_settings(&second)),
        &["leftOut"],
    );
    s.step(
        "setLeftOut",
        json!({"bookId": second, "chapters": []}),
        &core.set_left_out(&second, vec![]).await,
        &narrated,
    );
    scenarios.push(s.done());

    let mut s = Scenario::new("a sentence the speech engine says can be said again", "");
    s.step(
        "chapter",
        json!({"bookId": second, "chapter": 1}),
        &core.chapter(&second, 1),
        &[
            "blocks.0.segments.0.canSayAgain",
            "blocks.1.segments.0.canSayAgain",
        ],
    );
    s.step(
        "sayAgain",
        json!({"bookId": second, "segmentId": "99:0:0"}),
        &core.say_again(&second, "99:0:0").await,
        &[],
    );
    let spoken: Vec<String> = core
        .chapter(&second, 1)
        .unwrap()
        .blocks
        .into_iter()
        .flat_map(|b| b.segments)
        .filter(|s| s.can_say_again)
        .map(|s| s.id)
        .collect();
    // A book not listened to is listened to from the sentence.
    let said = core.say_again(&second, &spoken[1]).await;
    s.step(
        "sayAgain",
        json!({"bookId": second, "segmentId": spoken[1]}),
        &said,
        &[],
    );
    s.then(
        "onPlayerState",
        &core.player_state(),
        &["bookId", "chapter", "segmentId"],
    );
    core.pause();
    s.step("pause", json!({}), &Ok(()), &[]);
    // Paused, the playhead moves to the sentence and stays paused.
    let said = core.say_again(&second, &spoken[0]).await;
    s.step(
        "sayAgain",
        json!({"bookId": second, "segmentId": spoken[0]}),
        &said,
        &[],
    );
    s.then(
        "onPlayerState",
        &core.player_state(),
        &["bookId", "status", "segmentId"],
    );
    core.stop();
    scenarios.push(s.done());

    let mut s = Scenario::new(
        "a sentence chosen in the reader is bookmarked, not the playhead",
        "",
    );
    let chosen = core.chapter(&second, 1).unwrap().blocks[1].segments[0]
        .id
        .clone();
    s.step(
        "addBookmark",
        json!({"bookId": second, "note": "", "segmentId": chosen}),
        &core.add_bookmark_at(&second, "", Some(&chosen)),
        &["chapter", "segmentId", "note"],
    );
    s.step(
        "addBookmark",
        json!({"bookId": second, "note": "", "segmentId": "9:9:9"}),
        &core.add_bookmark_at(&second, "", Some("9:9:9")),
        &[],
    );
    scenarios.push(s.done());

    let mut s = Scenario::new("highlight, then it is listed", "");
    let segments: Vec<crate::book::SegmentView> = core
        .chapter(&second, 1)
        .unwrap()
        .blocks
        .into_iter()
        .skip(1)
        .flat_map(|b| b.segments)
        .collect();
    let words = &segments[0];
    let end_of = |i: usize| words.starts[i] + words.words[i].chars().count();
    let (start, middle, end) = (words.starts[0], end_of(1), end_of(3));
    let check = &[
        "length",
        "0.text",
        "0.chapterTitle",
        "0.start",
        "0.end",
        "0.note",
    ];
    s.step(
        "highlights",
        json!({"bookId": second}),
        &core.highlights(&second),
        &["length"],
    );
    s.step(
        "addHighlight",
        json!({"bookId": second, "chapter": 1, "start": start, "end": middle}),
        &core.add_highlight(&second, 1, start, middle),
        check,
    );
    let first_id = core.highlights(&second).unwrap()[0].id;
    s.step(
        "updateHighlightNote",
        json!({"id": first_id, "note": " A note "}),
        &core.update_highlight_note(first_id, " A note "),
        check,
    );
    // Touching the first: joined with it, its note kept.
    s.step(
        "addHighlight",
        json!({"bookId": second, "chapter": 1, "start": middle, "end": end}),
        &core.add_highlight(&second, 1, middle, end),
        check,
    );
    // Across into the next paragraph, the paragraph break kept in the text.
    let next = &segments
        .iter()
        .find(|s| s.id.split(':').nth(1) != words.id.split(':').nth(1))
        .unwrap();
    let (across, past) = (
        words.starts[words.words.len() - 1],
        next.starts[0] + next.words[0].chars().count(),
    );
    s.step(
        "addHighlight",
        json!({"bookId": second, "chapter": 1, "start": across, "end": past}),
        &core.add_highlight(&second, 1, across, past),
        &["length", "1.text", "1.start", "1.end"],
    );
    s.step(
        "highlights",
        json!({"bookId": second}),
        &core.highlights(&second),
        check,
    );
    s.step(
        "addHighlight",
        json!({"bookId": second, "chapter": 1, "start": 0, "end": 99_999}),
        &core.add_highlight(&second, 1, 0, 99_999),
        &[],
    );
    let joined = core.highlights(&second).unwrap()[0].id;
    s.step(
        "deleteHighlight",
        json!({"id": joined}),
        &core.delete_highlight(joined),
        &["length", "0.start"],
    );
    s.step(
        "deleteHighlight",
        json!({"id": joined}),
        &core.delete_highlight(joined),
        &[],
    );
    scenarios.push(s.done());

    let mut s = Scenario::new(
        "a word is said another way in the sentences that have it, and only there",
        "",
    );
    let ask = |written: &str, spoken: &str, match_case: bool| {
        (
            json!({"bookId": second, "written": written, "spoken": spoken, "matchCase": match_case}),
            core.pronunciation_affected(&second, written, spoken, match_case),
        )
    };
    for (written, spoken, match_case) in [
        ("Ren", "wren", false),
        ("ren", "wren", true),
        ("bench", "seat", false),
        ("zeppelin", "zep", false),
        ("  ", "wren", false),
        ("Ren", " … ", false),
    ] {
        let (args, affected) = ask(written, spoken, match_case);
        s.step(
            "pronunciationAffected",
            args,
            &affected,
            &["sentences", "cached"],
        );
    }
    let entry = |written: &str, spoken: &str| json!({"bookId": second, "written": written, "spoken": spoken, "matchCase": false});
    let saved = core
        .save_pronunciation(Some(&second), " Ren ", "wren", false)
        .await;
    s.step(
        "savePronunciation",
        entry(" Ren ", "wren"),
        &saved,
        &["written", "spoken", "matchCase", "bookId"],
    );
    let ren = saved.unwrap().id;
    s.step(
        "savePronunciation",
        entry("Ren", "  "),
        &core
            .save_pronunciation(Some(&second), "Ren", "  ", false)
            .await,
        &[],
    );
    s.step(
        "pronunciations",
        json!({"bookId": second}),
        &core.pronunciations(Some(&second)),
        &["length", "0.written", "0.spoken"],
    );
    // Saved as it is, it changes nothing more.
    let (args, affected) = ask("Ren", "wren", false);
    s.step("pronunciationAffected", args, &affected, &["sentences"]);
    let all = core.save_pronunciation(None, "bench", "seat", true).await;
    s.step(
        "savePronunciation",
        json!({"bookId": null, "written": "bench", "spoken": "seat", "matchCase": true}),
        &all,
        &["bookId", "matchCase"],
    );
    s.step(
        "pronunciations",
        json!({"bookId": null}),
        &core.pronunciations(None),
        &["length", "0.written"],
    );
    s.step(
        "deletePronunciation",
        json!({"id": ren}),
        &core.delete_pronunciation(ren).await,
        &[],
    );
    s.step(
        "deletePronunciation",
        json!({"id": ren}),
        &core.delete_pronunciation(ren).await,
        &[],
    );
    s.step(
        "pronunciations",
        json!({"bookId": second}),
        &core.pronunciations(Some(&second)),
        &["length"],
    );
    scenarios.push(s.done());

    let mut s = Scenario::new("collections are named once and hold books", "");
    let made = core.create_collection("Harbour");
    s.step("createCollection", json!({"name": "Harbour"}), &made, &[""]);
    let made = made.unwrap();
    for name in [" harbour ", "   "] {
        s.step(
            "createCollection",
            json!({"name": name}),
            &core.create_collection(name),
            &[],
        );
    }
    s.step(
        "collections",
        json!({}),
        &core.collections(),
        &["length", "0.name"],
    );
    s.step(
        "setBookCollections",
        json!({"bookId": second, "ids": [made]}),
        &core.set_book_collections(&second, &[made]),
        &[],
    );
    s.step(
        "listBooks",
        json!({}),
        &books(&core),
        &[&book("collections")],
    );
    s.step(
        "renameCollection",
        json!({"id": made, "name": "Quay"}),
        &core.rename_collection(made, "Quay"),
        &[],
    );
    s.step("collections", json!({}), &core.collections(), &["0.name"]);
    s.step(
        "deleteCollection",
        json!({"id": made}),
        &core.delete_collection(made),
        &[],
    );
    s.step("collections", json!({}), &core.collections(), &["length"]);
    s.step(
        "listBooks",
        json!({}),
        &books(&core),
        &[&book("collections")],
    );
    scenarios.push(s.done());

    let mut s = Scenario::new("a book is searched", "");
    for query in ["bench", "BENCH", "zeppelin"] {
        let hits = core.search_book(&second, query);
        s.step(
            "searchBook",
            json!({"bookId": second, "query": query}),
            &hits,
            &["length", "0.segmentId", "0.chapter"],
        );
    }
    scenarios.push(s.done());

    let mut s = Scenario::new("a book is removed", "");
    s.step(
        "removeBook",
        json!({"bookId": fourth}),
        &core.remove_book(&fourth),
        &[],
    );
    s.step(
        "listBooks",
        json!({}),
        &books(&core),
        &["length", &format!("[id={fourth}]")],
    );
    scenarios.push(s.done());

    let mut s = Scenario::new("an audiobook is added and played from its parts", "");
    let folder = super::audiobook::tests::tagged_set(dir.path());
    let folder = folder.to_string_lossy().into_owned();
    let added = core.add_audiobook(vec![folder.clone()]);
    s.step(
        "addAudiobook",
        json!({"paths": [folder]}),
        &Ok(added.clone()),
        &["bookId", "error"],
    );
    let audiobook = added.book_id.unwrap();
    let mut renames = renames.clone();
    renames.insert(0, (audiobook.clone(), "book-5".into()));
    let played = core.play(&audiobook, Some(1), None).await;
    s.step(
        "play",
        json!({"bookId": audiobook, "chapter": 1, "segmentId": null}),
        &played,
        &[],
    );
    s.then(
        "onPlayerState",
        &core.player_state(),
        &["bookId", "chapter"],
    );
    core.pause();
    s.step("pause", json!({}), &Ok(()), &[]);
    s.then(
        "onPlayerState",
        &core.player_state(),
        &["bookId", "status", "chapter"],
    );
    core.stop();
    s.step("stop", json!({}), &Ok(()), &[]);
    s.then("onPlayerState", &core.player_state(), &["status"]);
    let text_only = core.set_mode(&audiobook, super::ReadingMode::Read);
    s.step(
        "setMode",
        json!({"bookId": audiobook, "mode": "read"}),
        &text_only,
        &[],
    );
    s.step(
        "chapter",
        json!({"bookId": audiobook, "chapter": 0}),
        &core.chapter(&audiobook, 0),
        &[],
    );
    scenarios.push(s.done());
    let mut s = Scenario::new("an audiobook on its own is merged into its book", "");
    let added = core.add_audiobook(vec![folder.clone()]);
    s.step(
        "addAudiobook",
        json!({"paths": [folder]}),
        &Ok(added),
        &["bookId"],
    );
    let merged = core.merge_audiobook(&audiobook, &first);
    s.step(
        "mergeAudiobook",
        json!({"audiobookId": audiobook, "bookId": first}),
        &merged,
        &[],
    );
    // As the core tells it right after: the recording, checking first.
    let gone = format!("[id={audiobook}]");
    let paired = format!("[id={first}].recording");
    s.step("listBooks", json!({}), &books(&core), &[&gone, &paired]);
    let recording = Ok(core.recording(&first));
    s.step(
        "recording",
        json!({"bookId": first}),
        &recording,
        &["stage", "bookId", "listenable"],
    );
    scenarios.push(s.done());
    let renames_with_audiobook = renames;

    let mut s = Scenario::new("links given to Save", "");
    for link in [
        "harbour.example/notices/1",
        "  https://harbour.example/a  ",
        "http://harbour.example",
        "ftp://harbour.example/file",
        "mailto:keeper@harbour.example",
        "harbour notes",
        "",
        "https://",
    ] {
        let parsed = super::articles::parse_link(link).map(drop);
        s.step("saveArticle", json!({"url": link}), &parsed, &[]);
    }
    scenarios.push(s.done());
    let library_renames = renames_with_audiobook;

    // ---- on the feeds, as the mock starts
    let (core, _feeds_dir, _, _) = feeds_core().await;
    let mut s = Scenario::new("entries are read, saved and a feed left", "");
    let entries = |core: &Core, filter: super::feeds::EntryFilter| {
        core.feed_entries(&super::feeds::EntryQuery {
            filter,
            ..super::feeds::EntryQuery::default()
        })
    };
    use super::feeds::EntryFilter::{All, Saved, Unread};
    s.step(
        "feeds",
        json!({}),
        &core.feeds(),
        &["length", "[id=1].unread", "[id=2].unread"],
    );
    s.step(
        "feedEntries",
        json!({"feedId": null, "filter": "unread"}),
        &entries(&core, Unread),
        &["length"],
    );
    s.step(
        "markEntryRead",
        json!({"entryId": 1, "read": true}),
        &core.mark_entry_read(1, true),
        &[],
    );
    s.step("feeds", json!({}), &core.feeds(), &["[id=1].unread"]);
    s.step(
        "markEntryRead",
        json!({"entryId": 1, "read": false}),
        &core.mark_entry_read(1, false),
        &[],
    );
    s.step(
        "markAllRead",
        json!({"feedId": 2}),
        &core.mark_all_read(Some(2)),
        &[],
    );
    s.step(
        "feeds",
        json!({}),
        &core.feeds(),
        &["[id=1].unread", "[id=2].unread"],
    );
    s.step(
        "feedEntries",
        json!({"feedId": null, "filter": "unread"}),
        &entries(&core, Unread),
        &["length"],
    );
    let saved = core.save_feed_entry(6);
    s.step(
        "saveFeedEntry",
        json!({"entryId": 6}),
        &saved.as_ref().map(drop).map_err(Clone::clone),
        &[],
    );
    // The item the entry opens is the preview's article in the mock.
    let item = saved.unwrap();
    let mut renames = library_renames.clone();
    renames.insert(0, (item.clone(), "book-6".into()));
    s.step(
        "feedEntries",
        json!({"feedId": null, "filter": "saved"}),
        &entries(&core, Saved),
        &["length", "0.id"],
    );
    s.step(
        "feedEntries",
        json!({"feedId": null, "filter": "all"}),
        &entries(&core, All),
        &["length", "[id=6].inLibrary", "[id=6].read"],
    );
    let again = core.add_link("https://tidewatch.example/feed.xml").await;
    s.step(
        "addLink",
        json!({"url": "https://tidewatch.example/feed.xml"}),
        &again,
        &["kind", "value.id"],
    );
    s.step(
        "unsubscribe",
        json!({"feedId": 2}),
        &core.unsubscribe(2),
        &[],
    );
    s.step("feeds", json!({}), &core.feeds(), &["length", "[id=2]"]);
    s.step(
        "feedEntries",
        json!({"feedId": null, "filter": "all"}),
        &entries(&core, All),
        &["length"],
    );
    // A saved item an entry still opens goes back to the feed shelf when removed.
    let books = core.list_books();
    s.step(
        "listBooks",
        json!({}),
        &books,
        &[&format!("[id={item}].kind")],
    );
    s.step(
        "removeBook",
        json!({"bookId": item}),
        &core.remove_book(&item),
        &[],
    );
    let books = core.list_books();
    s.step("listBooks", json!({}), &books, &[&format!("[id={item}]")]);
    let saved = entries(&core, Saved);
    s.step(
        "feedEntries",
        json!({"feedId": null, "filter": "saved"}),
        &saved,
        &["length"],
    );
    let all = entries(&core, All);
    s.step(
        "feedEntries",
        json!({"feedId": null, "filter": "all"}),
        &all,
        &["[id=6].inLibrary"],
    );
    scenarios.push(s.done());

    // ---- the preview's web, from the feeds as the mock starts
    use super::article_tests::{LIGHTS, LIGHTS_URL};
    let (core, feeds_dir, _, pages) = feeds_core().await;
    preview_web(&pages);
    let mut s = Scenario::new(
        "feeds are found on the preview's web and imported from a list",
        "",
    );
    for link in [
        "https://lanterns.example/about",
        "https://lanterns.example/",
    ] {
        let found = core.subscribe(link).await;
        let check = [
            "kind",
            "value.length",
            "value.0.title",
            "value.0.url",
            "value.1.title",
        ];
        s.step("subscribe", json!({"url": link}), &found, &check);
    }
    let link = WEB_FEEDS[0].0;
    let found = core.subscribe(link).await;
    let check = [
        "kind",
        "value.title",
        "value.url",
        "value.unread",
        "value.stage.kind",
    ];
    s.step("subscribe", json!({"url": link}), &found, &check);
    let opml_feeds = [
        ("Tidewatch", "https://tidewatch.example/feed.xml"),
        (WEB_FEEDS[0].1, WEB_FEEDS[0].0),
        (WEB_FEEDS[2].1, WEB_FEEDS[2].0),
    ];
    let outlines: String = opml_feeds
        .iter()
        .map(|(title, url)| format!(r#"<outline type="rss" text="{title}" xmlUrl="{url}"/>"#))
        .collect();
    let opml = feeds_dir.path().join("Subscriptions.opml");
    std::fs::write(
        &opml,
        format!(r#"<?xml version="1.0"?><opml version="2.0"><body>{outlines}</body></opml>"#),
    )
    .unwrap();
    let opml = opml.to_string_lossy().into_owned();
    let imported = core.import_opml(&opml);
    s.step(
        "importOpml",
        json!({"path": opml}),
        &imported,
        &["added", "skipped"],
    );
    let quay = format!("[url={}]", WEB_FEEDS[2].0);
    let check = [
        "length".to_owned(),
        format!("{quay}.title"),
        format!("{quay}.unread"),
        format!("[url={}].title", WEB_FEEDS[0].0),
    ];
    let check: Vec<&str> = check.iter().map(String::as_str).collect();
    s.step("feeds", json!({}), &core.feeds(), &check);
    scenarios.push(s.done());
    renames.push((feeds_dir.path().to_string_lossy().into_owned(), "~".into()));
    let web = serde_json::json!({
        "feeds": WEB_FEEDS.iter().map(|(url, title)| json!({"url": url, "title": title})).collect::<Vec<_>>(),
        "opml": opml_feeds.iter().map(|(title, url)| json!({"url": url, "title": title})).collect::<Vec<_>>(),
    });
    write("web", &web, &[]);

    // ---- an article saved and fetched
    use super::browsing::BrowserUse;
    let fetching = super::browser_tests::setup(BrowserUse::Off, "");
    fetching.pages.set(LIGHTS_URL, Ok(LIGHTS.into()));
    let mut s = Scenario::new("an article is saved and fetched", "");
    let saved = fetching.core.save_article(LIGHTS_URL);
    s.step("saveArticle", json!({"url": LIGHTS_URL}), &saved, &[""]);
    let article = saved.unwrap();
    renames.insert(0, (article.clone(), "book-6".into()));
    until("the article is fetched", || {
        fetching
            .core
            .article(&article)
            .is_ok_and(|a| a.stage == super::fetching::Stage::Ready)
    })
    .await;
    let check = [
        "stage.kind",
        "via",
        "siteName",
        "byline",
        "capturedWords",
        "inLibrary",
    ];
    s.then(
        "onArticleState",
        &fetching.core.article(&article).unwrap(),
        &check,
    );
    let kind = format!("[id={article}].kind");
    s.step(
        "listBooks",
        json!({}),
        &fetching.core.list_books(),
        &[&kind],
    );
    scenarios.push(s.done());

    // ---- a site that cut an article short, signed in to
    use super::signin::SignInStage;
    let (web, id) = super::browser_tests::needing_sign_in().await;
    let mut s = Scenario::new(
        "a site that cut an article short is signed in to",
        "?article=needsyou-cutshort&browser=ready",
    );
    let site = "[host=quay.example]";
    let site_check = [
        format!("{site}.session.kind"),
        format!("{site}.session.reason"),
        format!("{site}.waitingArticle"),
    ];
    let site_check: Vec<&str> = site_check.iter().map(String::as_str).collect();
    let check = ["stage.kind", "stage.reason"];
    s.step(
        "article",
        json!({"bookId": id}),
        &web.core.article(&id),
        &check,
    );
    s.step("sites", json!({}), &web.core.sites(), &site_check);
    *web.browser.window.page.lock().unwrap() = super::browser_tests::SIGN_IN_PAGE.into();
    let started = web.core.start_sign_in(&id);
    let check = ["site", "reason", "staySignedIn"];
    s.step("startSignIn", json!({"bookId": id}), &started, &check);
    until("the window is open", || {
        web.core.sign_in().unwrap().stage == SignInStage::Waiting
    })
    .await;
    let check = ["stage.kind", "site", "reason"];
    s.then("onSignInState", &web.core.sign_in().unwrap(), &check);
    web.browser.set(LIGHTS_URL, Ok(LIGHTS));
    s.step("finishSignIn", json!({}), &web.core.finish_sign_in(), &[]);
    let done = super::browser_tests::sign_in_ends(&web.core).await;
    s.then("onSignInState", &done, &["stage.kind", "result.kind"]);
    let check = ["stage.kind", "via"];
    s.step(
        "article",
        json!({"bookId": id}),
        &web.core.article(&id),
        &check,
    );
    s.step("sites", json!({}), &web.core.sites(), &site_check);
    scenarios.push(s.done());

    // ---- the browser downloaded
    use super::browser_download::tests::{chrome_zip, host, sha256};
    let pinned = super::browser_download::Source::pinned().unwrap();
    let zip = chrome_zip();
    let source = super::browser_download::Source {
        version: pinned.version.clone(),
        url: host(zip.clone(), Arc::default()).await,
        bytes: zip.len() as u64,
        sha256: sha256(&zip),
        program: "chrome-test/chrome".into(),
    };
    let downloads = tempfile::tempdir().unwrap();
    super::tests::library_folder(downloads.path(), "");
    let (browser_core, _) = super::tests::start_downloading(
        downloads.path(),
        &FakeEngine::new(),
        Arc::new(|_| Err("no speech recognition here".into())),
        super::article_tests::Pages::with(&[]),
        Arc::new(super::browsing::testing::ScriptedBrowser::default()),
        Some(source),
    );
    browser_core
        .update_app_settings(super::AppSettingsPatch {
            browser: Some(super::browsing::BrowserSettings {
                use_: BrowserUse::Fallback,
                path: None,
                idle_seconds: 60,
            }),
            ..Default::default()
        })
        .unwrap();
    let mut s = Scenario::new("the browser is downloaded", "?browser=missing");
    let status = Ok(browser_core.browser_status());
    s.step(
        "browserStatus",
        json!({}),
        &status,
        &["use", "offeredVersion", "downloaded"],
    );
    let downloaded = browser_core.download_browser().await;
    s.step("downloadBrowser", json!({}), &downloaded, &[]);
    let last = browser_core.browser_status().download.unwrap();
    s.then("onBrowserDownload", &last, &["stage", "version"]);
    let status = Ok(browser_core.browser_status());
    let check = [
        "downloaded",
        "binary.kind",
        "download.stage",
        "offeredVersion",
    ];
    s.step("browserStatus", json!({}), &status, &check);
    scenarios.push(s.done());
    write(
        "browser",
        &json!({ "version": pinned.version, "bytes": pinned.bytes }),
        &[],
    );

    // ---- books found in the preview's folders, looked up and imported; audiobooks paired
    let found_dir = tempfile::tempdir().unwrap();
    let (core, found_renames) = preview_library(found_dir.path()).await;
    let [tide, glass] = preview_folders(found_dir.path());
    let folder = |name: &str| found_dir.path().join(name).to_string_lossy().into_owned();
    // The preview's audiobooks are short real recordings, far below an audiobook's size.
    core.audiobooks
        .any_length
        .store(true, std::sync::atomic::Ordering::Relaxed);
    core.update_app_settings(super::AppSettingsPatch {
        metadata_lookup: Some(true),
        scan: Some(super::discovery::ScanSettings {
            roots: vec![folder("Downloads"), folder("Documents")],
            ..Default::default()
        }),
        audiobooks: Some(super::audiobooks::AudiobookSearchSettings {
            roots: vec![folder("Audiobooks")],
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    let book_id = |index: usize| found_renames[index].0.clone();
    let mut s = Scenario::new("books are found, looked up and imported", "");
    s.step(
        "discoveredBooks",
        json!({}),
        &core.discovered_books(),
        &["length"],
    );
    let scanned = core.scan_for_books().await;
    let (t, g) = (format!("[id={tide}]"), format!("[id={glass}]"));
    let check = [
        "length".to_owned(),
        format!("{t}.status"),
        format!("{t}.title"),
        format!("{t}.paths.length"),
        format!("{t}.audiobook.length"),
        format!("{t}.suggestion"),
        format!("{g}.status"),
        format!("{g}.audiobook.length"),
        format!("[id={}].status", book_id(0)),
        format!("[id={}].audiobook.0", book_id(0)),
        "[status=unreadable].error".to_owned(),
    ];
    let check: Vec<&str> = check.iter().map(String::as_str).collect();
    s.step("scanForBooks", json!({}), &scanned, &check);
    // Open Library, as its answers are kept: the stand-in for the network.
    let new = scanned.as_ref().unwrap().iter();
    let new = new.filter(|c| c.status == super::discovery::CandidateStatus::New);
    for (candidate, published) in new.zip(["2018", "2021"]) {
        let found = vec![aoide_metadata::Match {
            key: format!("/works/{}", candidate.id),
            title: candidate.title.clone(),
            authors: candidate.authors.clone(),
            published: Some(published.into()),
            publisher: Some("Preview Press".into()),
            isbn: None,
            subjects: vec!["Fiction".into()],
            cover_url: None,
        }];
        let query = aoide_metadata::Query {
            isbn: candidate.isbn.clone(),
            title: candidate.title.clone(),
            authors: candidate.authors.clone(),
        };
        super::lock(&core.library)
            .store_lookup(
                &super::metadata::cache_key(&query),
                &serde_json::to_string(&found).unwrap(),
                super::now_ms(),
            )
            .unwrap();
    }
    let ids = vec![tide.clone(), glass.clone()];
    let fetched = core.fetch_discovered_details(ids.clone()).await;
    s.step("fetchDiscoveredDetails", json!({"ids": ids}), &fetched, &[]);
    let check = [
        format!("{t}.suggestion.publisher"),
        format!("{t}.suggestion.title"),
        format!("{g}.suggestion.publisher"),
    ];
    let check: Vec<&str> = check.iter().map(String::as_str).collect();
    let looked_up = core.discovered_books();
    s.step("discoveredBooks", json!({}), &looked_up, &check);
    let details: std::collections::BTreeMap<_, _> = looked_up
        .unwrap()
        .into_iter()
        .filter_map(|c| Some((c.id, c.suggestion?)))
        .collect();
    let choices = json!([{"id": tide, "applySuggestion": true, "pairAudiobook": true}]);
    let imported = core
        .import_discovered(serde_json::from_value(choices.clone()).unwrap())
        .await;
    let check = ["length", "0.bookId", "0.error"];
    s.step(
        "importDiscovered",
        json!({"choices": choices}),
        &imported,
        &check,
    );
    let check = [format!("{t}.title"), format!("{t}.recording")];
    let check: Vec<&str> = check.iter().map(String::as_str).collect();
    s.step("listBooks", json!({}), &core.list_books(), &check);
    let check = [
        "length".to_owned(),
        format!("{t}.status"),
        format!("{g}.status"),
    ];
    let check: Vec<&str> = check.iter().map(String::as_str).collect();
    s.step(
        "discoveredBooks",
        json!({}),
        &core.discovered_books(),
        &check,
    );
    scenarios.push(s.done());

    let mut s = Scenario::new("audiobooks are found for books and paired", "");
    let (map, atlas) = (
        format!("[bookId={}]", book_id(1)),
        format!("[bookId={}]", book_id(3)),
    );
    let matches = core.scan_for_audiobooks().await;
    let mut check = vec!["length".to_owned()];
    for at in [&map, &atlas] {
        for field in ["candidates.length", "stage.kind"] {
            check.push(format!("{at}.{field}"));
        }
        for field in ["exact", "folder", "bytes", "files.length", "paths.0"] {
            check.push(format!("{at}.candidates.0.{field}"));
        }
    }
    check.push(format!("{map}.candidates.1.paths.0"));
    let check: Vec<&str> = check.iter().map(String::as_str).collect();
    s.step("scanForAudiobooks", json!({}), &matches, &check);
    let unmatched = core.unmatched_audiobooks();
    let check = [
        "length",
        "0.paths.0",
        "0.folder",
        "0.files.length",
        "0.bytes",
    ];
    s.step(
        "unmatchedAudiobooks",
        json!({}),
        &Ok(unmatched.clone()),
        &check,
    );
    let choices = json!([{"bookId": book_id(1), "candidate": 1}]);
    let paired = Ok(core.pair_audiobooks(serde_json::from_value(choices.clone()).unwrap()));
    s.step(
        "pairAudiobooks",
        json!({"choices": choices}),
        &paired,
        &["length", "0.bookId", "0.error"],
    );
    let check = [
        "length".to_owned(),
        map.clone(),
        format!("{atlas}.stage.kind"),
    ];
    let check: Vec<&str> = check.iter().map(String::as_str).collect();
    s.step(
        "audiobookMatches",
        json!({}),
        &Ok(core.audiobook_matches()),
        &check,
    );
    let recording = Ok(core.recording(&book_id(1)));
    s.step(
        "recording",
        json!({"bookId": book_id(1)}),
        &recording,
        &["stage", "files.length"],
    );
    scenarios.push(s.done());

    // The preview's "Find books" and "Find audiobooks", as the core first answers them.
    let mut found = found_renames.clone();
    found.insert(0, (tide.clone(), "h-tide".into()));
    found.insert(0, (glass.clone(), "h-glass".into()));
    write(
        "found",
        &json!({
            "books": scanned.unwrap(),
            "details": details,
            "audiobooks": matches.unwrap(),
            "unmatched": unmatched,
        }),
        &found,
    );
    renames.insert(0, (tide, "h-tide".into()));
    renames.insert(0, (glass, "h-glass".into()));
    renames.push((found_dir.path().to_string_lossy().into_owned(), "~".into()));

    write("scenarios", &scenarios, &renames);
}

/// The engine's status with a server Aoide starts, as the mock shows it with
/// `server=starting|running|stopped|failed`: written as `speech-server.json`.
#[tokio::test]
async fn speech_server_states_for_the_ui_mock() {
    use super::engine_tests::{start, Scripted};
    use aoide_tts::EngineStatus;
    if !wanted() {
        return;
    }
    let script = |dir: &Path, body: &str| {
        let engine = Scripted::new("chatterbox", EngineStatus::Offline, &["Emily.wav"], false);
        let launch = format!(
            "voiceId = \"Emily.wav\"\n[engine.launch]\nprogram = \"sh\"\nargs = [\"-c\", \"{body}\"]"
        );
        (engine, dir.to_owned(), launch)
    };
    let renames = vec![(
        "Engine chatterbox".to_owned(),
        "Local Chatterbox".to_owned(),
    )];
    let mut views = serde_json::Map::new();

    let dir = tempfile::tempdir().unwrap();
    let (engine, root, launch) = script(dir.path(), "echo Loading the model; exec sleep 300");
    let (core, _events, id) = start(&root, &[&engine], &launch).await;
    core.prepare_book(&id).await.unwrap();
    until("starting", || {
        core.engine_status().detail.as_deref() == Some("Starting the voice…")
    })
    .await;
    views.insert(
        "starting".into(),
        serde_json::to_value(core.engine_status()).unwrap(),
    );
    engine.set_status(EngineStatus::Ready(Some("chatterbox-turbo".into())));
    until("running", || {
        core.engine_status()
            .server
            .is_some_and(|s| s.state == super::engine_process::ServerState::Running)
    })
    .await;
    core.cancel_prepare(&id);
    let mut running = core.engine_status();
    running.state = super::EngineState::Ready;
    views.insert("running".into(), serde_json::to_value(running).unwrap());
    core.stop_speech_server();
    until("stopped", || {
        core.engine_status()
            .server
            .is_some_and(|s| s.state == super::engine_process::ServerState::Stopped)
    })
    .await;
    views.insert(
        "stopped".into(),
        serde_json::to_value(core.engine_status()).unwrap(),
    );
    core.shutdown();

    let dir = tempfile::tempdir().unwrap();
    let (engine, root, launch) = script(
        dir.path(),
        "echo Starting the server >&2; echo \\\"python: can't open file 'server.py': [Errno 2] No such file or directory\\\" >&2; exit 2",
    );
    let (core, _events, id) = start(&root, &[&engine], &launch).await;
    core.prepare_book(&id).await.unwrap();
    until("failed", || {
        core.engine_status().state == super::EngineState::Error
    })
    .await;
    views.insert(
        "failed".into(),
        serde_json::to_value(core.engine_status()).unwrap(),
    );
    write("speech-server", &views, &renames);
}

/// The preview's EPUBs are the same bytes on every machine and in every build: their ids are
/// in the fixtures, so they must not depend on the compressor `flate2` happens to be built
/// with (another crate can switch it) or on the time they are written.
#[test]
fn the_preview_books_are_the_same_bytes_everywhere() {
    for book in BOOKS.iter().chain(&FOUND) {
        let bytes = epub(book);
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        for index in 0..zip.len() {
            let entry = zip.by_index(index).unwrap();
            assert_eq!(
                entry.compression(),
                zip::CompressionMethod::Stored,
                "{} in {}",
                entry.name(),
                book.title
            );
            assert_eq!(entry.last_modified(), Some(zip::DateTime::default()));
        }
    }
}
