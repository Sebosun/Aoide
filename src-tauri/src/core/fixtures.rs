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
    chapters: &'static [&'static [&'static str]],
    /// Adds a copyright page (left out) and an empty section (skipped), for extraction notes.
    notes: bool,
}

/// The browser preview's books, with original text.
const BOOKS: [Book; 4] = [
    Book {
        title: "The Lantern Keeper",
        author: "Mira Vale",
        chapters: &[
            &[
                "The small light",
                "Every evening, Ada carried a lantern to the end of the pier. The sea collected its reflection and broke it into a hundred little stars.",
                "Tonight a paper boat waited beside the steps. Inside it lay a key, warm as if someone had held it all the way across the bay.",
            ],
            &[
                "Across the water",
                "At dawn she borrowed the blue rowing boat. Each pull of the oars opened a quiet doorway in the water.",
                "On the far shore, a gardener waved. He had been waiting for someone to bring back the light.",
            ],
        ],
        notes: false,
    },
    Book {
        title: "A Map of Quiet Places",
        author: "Ellis Reed",
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
        ],
        notes: true,
    },
    Book {
        title: "The Absent Orchard",
        author: "Ellis Reed",
        chapters: &[&[
            "The gate",
            "The orchard had been sold twice, and neither owner had ever seen it.",
        ]],
        notes: false,
    },
    Book {
        title: "An Unfinished Atlas",
        author: "Ellis Reed",
        chapters: &[&[
            "Blank pages",
            "Half the atlas was empty, and the cartographer liked it best that way.",
        ]],
        notes: false,
    },
];

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
        let body: String = std::iter::once(format!("<h2>{}</h2>", parts[0]))
            .chain(parts[1..].iter().map(|p| format!("<p>{p}</p>")))
            .collect();
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
            r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{}</dc:title><dc:creator>{}</dc:creator><dc:language>en</dc:language></metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>"#,
            book.title, book.author
        ),
    ));
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, body) in files {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
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

    write("player", &core.player_state(), &renames);
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

/// Subscriptions and their entries, which the mock serves on its Feeds screen: three feeds
/// that answer (one carrying whole articles, one summaries only) and one that stopped. Times
/// are written relative to now (the mock adds its own now).
#[tokio::test]
async fn feeds_for_the_ui_mock() {
    use super::article_tests::Pages;
    use super::feeds::{EntryFilter, EntryQuery, FeedStage};
    const HOUR: i64 = 60 * 60 * 1000;
    if !wanted() {
        return;
    }
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
