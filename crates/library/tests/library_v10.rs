//! Schema v10: articles saved from the web.
use aoide_library::{ArticleRow, BookId, ItemKind, Library, NewBook};

fn article(lib: &Library, id: &str) -> BookId {
    let id = BookId(id.into());
    lib.upsert_book(
        &NewBook {
            id: id.clone(),
            kind: ItemKind::Article,
            path: format!("articles/{}/snapshot.html", id.0).into(),
            title: Some("news.example".into()),
            authors: vec![],
            language: None,
            cover_path: None,
            error: None,
        },
        10,
    )
    .unwrap();
    id
}

fn row(id: &BookId) -> ArticleRow {
    ArticleRow {
        book_id: id.clone(),
        url: "https://news.example/ferry?utm_source=feed".into(),
        canonical_url: "https://news.example/ferry".into(),
        site_name: None,
        byline: None,
        published_at: None,
        stage: "queued".into(),
        reason: None,
        error: None,
        via: None,
        fetched_at: None,
        word_count: None,
        declared_words: None,
        lead: None,
    }
}

#[test]
fn a_version_9_library_opens_with_no_articles() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("DROP TABLE IF EXISTS sites; DROP TABLE IF EXISTS feed_entries; DROP TABLE IF EXISTS feeds; ALTER TABLE books DROP COLUMN shelf; DROP TABLE articles; DROP TABLE IF EXISTS book_tags; DROP TABLE IF EXISTS tags; DROP TABLE IF EXISTS segment_takes; PRAGMA user_version = 9;")
        .unwrap();
    drop(conn);

    let lib = Library::open(&path).unwrap();
    assert!(lib.articles().unwrap().is_empty());
    let a = article(&lib, "a");
    lib.save_article(&row(&a)).unwrap();
    assert_eq!(lib.article(&a).unwrap(), Some(row(&a)));
}

#[test]
fn an_article_round_trips_moves_through_its_stages_and_goes_with_its_item() {
    let lib = Library::open_in_memory().unwrap();
    let a = article(&lib, "a");
    assert_eq!(lib.book(&a).unwrap().unwrap().kind, ItemKind::Article);
    lib.save_article(&row(&a)).unwrap();

    lib.set_article_stage(&a, "fetching", None, None).unwrap();
    assert_eq!(lib.article(&a).unwrap().unwrap().stage, "fetching");

    let fetched = ArticleRow {
        site_name: Some("Harbour Weekly".into()),
        byline: Some("Ada Brightwater".into()),
        published_at: Some(1_772_600_767_000),
        stage: "needs_you".into(),
        reason: Some("cut_short".into()),
        via: Some("http".into()),
        fetched_at: Some(20),
        word_count: Some(180),
        declared_words: Some(1_000),
        lead: Some("The night ferry crosses the strait twice before dawn.".into()),
        ..row(&a)
    };
    lib.save_article(&fetched).unwrap();
    assert_eq!(lib.article(&a).unwrap(), Some(fetched.clone()));
    assert_eq!(lib.articles().unwrap(), [fetched]);

    lib.set_article_stage(
        &a,
        "failed",
        None,
        Some("The site answered with status 404."),
    )
    .unwrap();
    let failed = lib.article(&a).unwrap().unwrap();
    assert_eq!(
        (
            failed.stage.as_str(),
            failed.reason,
            failed.error.as_deref()
        ),
        ("failed", None, Some("The site answered with status 404."))
    );

    lib.remove_book(&a).unwrap();
    assert_eq!(lib.article(&a).unwrap(), None);
}
