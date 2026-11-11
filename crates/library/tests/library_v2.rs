use aoide_library::BookDetails;
use aoide_library::{AudioUsage, CachedSegment, WordTiming};
use aoide_library::{
    BookId, BookListening, Bookmark, DayListening, Library, ListeningStats, NewBook,
};

#[test]
fn removing_a_book_cascades_v2_data_without_affecting_other_books() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    let b = book(&lib, "b");
    for id in [&a, &b] {
        lib.add_bookmark(id, 1, 2, None, "excerpt", "note", 3)
            .unwrap();
        lib.record_listening(id, "2026-11-10", 100).unwrap();
        lib.set_book_details(id, &BookDetails::default()).unwrap();
    }
    lib.remove_book(&a).unwrap();
    assert!(lib.bookmarks(&a).unwrap().is_empty());
    assert_eq!(lib.book_details(&a).unwrap(), None);
    assert_eq!(lib.bookmarks(&b).unwrap().len(), 1);
    assert_eq!(lib.book_details(&b).unwrap(), Some(BookDetails::default()));
    assert_eq!(
        lib.listening_stats().unwrap(),
        ListeningStats {
            total_ms: 100,
            by_book: vec![BookListening {
                book_id: b,
                ms: 100
            }],
            by_day: vec![DayListening {
                day: "2026-11-10".into(),
                ms: 100
            }],
        }
    );
    book(&lib, "a");
    assert!(lib.bookmarks(&a).unwrap().is_empty());
    assert_eq!(lib.book_details(&a).unwrap(), None);
    assert_eq!(lib.listening_stats().unwrap().total_ms, 100);
}

#[test]
fn v2_writes_require_an_existing_book() {
    let lib = Library::open_in_memory().unwrap();
    let missing = BookId("missing".into());
    assert!(lib.add_bookmark(&missing, 0, 0, None, "", "", 0).is_err());
    assert!(lib.record_listening(&missing, "2026-11-10", 1).is_err());
    assert!(lib
        .set_book_details(&missing, &BookDetails::default())
        .is_err());
}

#[test]
fn version_one_database_migrates_in_place_and_preserves_existing_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    // Standalone v1 schema: deliberately does not use Library to create the fixture.
    conn.execute_batch(
        "CREATE TABLE books (
            id TEXT PRIMARY KEY NOT NULL, path TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('present', 'missing', 'error')),
            error TEXT, title TEXT, authors TEXT NOT NULL, language TEXT, cover_path TEXT,
            added_at INTEGER NOT NULL, last_opened_at INTEGER,
            CHECK ((status = 'error') = (error IS NOT NULL))
        );
        CREATE TABLE progress (
            book_id TEXT PRIMARY KEY NOT NULL REFERENCES books(id) ON DELETE CASCADE,
            chapter INTEGER NOT NULL CHECK(chapter >= 0),
            char_offset INTEGER NOT NULL CHECK(char_offset >= 0),
            segment_id TEXT, updated_at INTEGER NOT NULL
        );
        CREATE TABLE segments (
            key TEXT PRIMARY KEY NOT NULL,
            book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
            file TEXT NOT NULL, duration_ms INTEGER NOT NULL CHECK(duration_ms >= 0),
            bytes INTEGER NOT NULL CHECK(bytes >= 0), word_timings TEXT, created_at INTEGER NOT NULL
        );
        CREATE INDEX segments_book_id ON segments(book_id);
        CREATE TABLE settings (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO books (id, path, status, title, authors, added_at, last_opened_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            "legacy",
            "old.epub",
            "present",
            "Old book",
            "[\"Author\"]",
            12,
            34
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO progress VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params!["legacy", 2, 30, "s", 40],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO segments VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            "old",
            "legacy",
            "old.opus",
            123,
            456,
            Option::<String>::None,
            50
        ],
    )
    .unwrap();
    conn.execute("INSERT INTO settings VALUES (?1, ?2)", ["theme", "dark"])
        .unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();
    drop(conn);

    let lib = Library::open(&path).unwrap();
    let id = BookId("legacy".into());
    let old = lib.book(&id).unwrap().unwrap();
    assert_eq!(old.path, std::path::PathBuf::from("old.epub"));
    assert_eq!(old.title.as_deref(), Some("Old book"));
    assert_eq!(old.authors, vec!["Author"]);
    assert_eq!(old.added_at, 12);
    assert_eq!(old.last_opened_at, Some(34));
    assert_eq!(
        lib.progress(&id).unwrap(),
        Some(aoide_library::Progress {
            chapter: 2,
            char_offset: 30,
            segment_id: Some("s".into()),
            updated_at: 40,
        })
    );
    let mut expected_segment = segment("old", &id, 456, 50);
    expected_segment.word_timings = None;
    assert_eq!(lib.segment("old").unwrap(), Some(expected_segment));
    assert_eq!(lib.setting("theme").unwrap().as_deref(), Some("dark"));
    let mark = lib
        .add_bookmark(&id, 2, 30, None, "old text", "note", 60)
        .unwrap();
    lib.record_listening(&id, "2026-11-10", 25).unwrap();
    lib.set_book_details(&id, &BookDetails::default()).unwrap();
    drop(lib);
    let lib = Library::open(&path).unwrap();
    assert_eq!(lib.bookmarks(&id).unwrap()[0].id, mark);
    assert_eq!(lib.listening_stats().unwrap().total_ms, 25);
    assert_eq!(lib.book_details(&id).unwrap(), Some(BookDetails::default()));
    assert_eq!(lib.total_audio_bytes().unwrap(), 456);
}

fn segment(key: &str, book_id: &BookId, bytes: u64, created_at: i64) -> CachedSegment {
    CachedSegment {
        key: key.into(),
        book_id: book_id.clone(),
        file: format!("{key}.opus").into(),
        duration_ms: 123,
        bytes,
        created_at,
        word_timings: Some(vec![WordTiming {
            start_ms: 0,
            end_ms: 123,
        }]),
    }
}

#[test]
fn deleting_selected_segments_ignores_unknown_and_duplicate_keys() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    let b = book(&lib, "b");
    for s in [
        segment("a'", &a, 10, 0),
        segment("b", &b, 20, 0),
        segment("keep", &a, 30, 0),
    ] {
        lib.record_segment(&s).unwrap();
    }
    lib.delete_segments(&[]).unwrap();
    assert_eq!(lib.total_audio_bytes().unwrap(), 60);
    lib.delete_segments(&["a'".into(), "missing".into(), "b".into(), "a'".into()])
        .unwrap();
    assert!(lib.segment("a'").unwrap().is_none());
    assert!(lib.segment("b").unwrap().is_none());
    assert_eq!(
        lib.segment("keep").unwrap(),
        Some(segment("keep", &a, 30, 0))
    );
    assert_eq!(lib.total_audio_bytes().unwrap(), 30);
    assert!(lib.book(&a).unwrap().is_some());
    assert!(lib.book(&b).unwrap().is_some());
}

#[test]
fn cache_totals_and_oldest_segments_respect_order_exclusions_and_limits() {
    let lib = Library::open_in_memory().unwrap();
    assert_eq!(lib.total_audio_bytes().unwrap(), 0);
    assert!(lib.oldest_segments(None, 5).unwrap().is_empty());
    let a = book(&lib, "a");
    let b = book(&lib, "b");
    let latest = segment("c", &a, 300, 20);
    let tied_b = segment("b", &b, 20, 10);
    let tied_a = segment("a'", &a, 100, 10);
    let oldest = segment("z", &b, 7, -1);
    for s in [&latest, &tied_b, &tied_a, &oldest] {
        lib.record_segment(s).unwrap();
    }
    assert_eq!(lib.total_audio_bytes().unwrap(), 427);
    assert_eq!(
        lib.oldest_segments(None, 99).unwrap(),
        vec![
            oldest.clone(),
            tied_a.clone(),
            tied_b.clone(),
            latest.clone()
        ]
    );
    assert_eq!(
        lib.oldest_segments(None, 2).unwrap(),
        vec![oldest.clone(), tied_a.clone()]
    );
    assert_eq!(lib.oldest_segments(Some(&b), 1).unwrap(), vec![tied_a]);
    assert_eq!(
        lib.oldest_segments(Some(&a), usize::MAX).unwrap(),
        vec![oldest, tied_b]
    );
    assert!(lib.oldest_segments(None, 0).unwrap().is_empty());
    let mut replaced = latest;
    replaced.bytes = 1;
    lib.record_segment(&replaced).unwrap();
    assert_eq!(lib.total_audio_bytes().unwrap(), 128);
}

#[test]
fn details_round_trip_replace_and_persist() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let lib = Library::open(&path).unwrap();
    let a = book(&lib, "a");
    let b = book(&lib, "b");
    assert_eq!(lib.book_details(&a).unwrap(), None);
    let details = BookDetails {
        description: Some("A 'description' 雪\nwith lines".into()),
        publisher: Some("Publisher".into()),
        isbn: Some("123".into()),
        published: Some("2026".into()),
        subjects: vec!["Fiction".into(), "Adventure".into()],
        open_library_key: Some("/works/OL1W".into()),
    };
    lib.set_book_details(&a, &details).unwrap();
    assert_eq!(lib.book_details(&a).unwrap(), Some(details.clone()));
    lib.set_book_details(&b, &details).unwrap();
    lib.set_book_details(&a, &BookDetails::default()).unwrap();
    drop(lib);
    let lib = Library::open(&path).unwrap();
    assert_eq!(lib.book_details(&a).unwrap(), Some(BookDetails::default()));
    assert_eq!(lib.book_details(&b).unwrap(), Some(details));
    assert_eq!(lib.book_details(&BookId("absent".into())).unwrap(), None);
}

#[test]
fn listening_accumulates_and_sorts_across_books_and_days() {
    let lib = Library::open_in_memory().unwrap();
    assert_eq!(
        lib.listening_stats().unwrap(),
        ListeningStats {
            total_ms: 0,
            by_book: vec![],
            by_day: vec![]
        }
    );
    let a = book(&lib, "a");
    let b = book(&lib, "b");
    let c = book(&lib, "c");
    lib.record_listening(&b, "2026-11-02", 300).unwrap();
    lib.record_listening(&a, "2026-11-02", 100).unwrap();
    lib.record_listening(&a, "2026-11-02", 50).unwrap();
    lib.record_listening(&a, "2026-11-01", 150).unwrap();
    lib.record_listening(&c, "2026-11-01", 500).unwrap();
    lib.record_listening(&c, "2026-11-01", 0).unwrap();
    assert_eq!(
        lib.listening_stats().unwrap(),
        ListeningStats {
            total_ms: 1100,
            by_book: vec![
                BookListening {
                    book_id: c,
                    ms: 500
                },
                BookListening {
                    book_id: a,
                    ms: 300
                },
                BookListening {
                    book_id: b,
                    ms: 300
                }
            ],
            by_day: vec![
                DayListening {
                    day: "2026-11-01".into(),
                    ms: 650
                },
                DayListening {
                    day: "2026-11-02".into(),
                    ms: 450
                }
            ],
        }
    );
}

#[test]
fn bookmark_notes_can_be_updated_and_bookmarks_deleted() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    let id = lib.add_bookmark(&a, 0, 1, None, "text", "old", 4).unwrap();
    let original = lib.bookmarks(&a).unwrap().remove(0);
    assert!(lib.update_bookmark_note(id, "new ' 雪").unwrap());
    assert_eq!(
        lib.bookmarks(&a).unwrap(),
        vec![Bookmark {
            note: "new ' 雪".into(),
            ..original
        }]
    );
    assert!(!lib.update_bookmark_note(id + 100, "missing").unwrap());
    assert!(!lib.delete_bookmark(id + 100).unwrap());
    assert!(lib.delete_bookmark(id).unwrap());
    assert!(!lib.delete_bookmark(id).unwrap());
    assert!(!lib.update_bookmark_note(id, "deleted").unwrap());
    assert!(lib.bookmarks(&a).unwrap().is_empty());
    assert!(lib.add_bookmark(&a, 0, 0, None, "", "", 5).unwrap() > id);
}

fn book(lib: &Library, id: &str) -> BookId {
    let id = BookId(id.into());
    lib.upsert_book(
        &NewBook {
            id: id.clone(),
            path: "synthetic.epub".into(),
            title: Some("Synthetic book".into()),
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

#[test]
fn audio_usage_is_summed_per_book_and_lists_every_indexed_file() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    let b = book(&lib, "b");
    book(&lib, "silent");
    for s in [
        segment("a1", &a, 100, 5),
        segment("a2", &a, 250, 9),
        segment("b1", &b, 40, 7),
    ] {
        lib.record_segment(&s).unwrap();
    }
    let usage = lib.audio_usage().unwrap();
    assert_eq!(
        usage,
        vec![
            AudioUsage {
                book_id: a.clone(),
                bytes: 350,
                segments: 2,
                newest_at: 9
            },
            AudioUsage {
                book_id: b.clone(),
                bytes: 40,
                segments: 1,
                newest_at: 7
            },
        ]
    );
    let files = lib.segment_files().unwrap();
    assert_eq!(files.len(), 3);
    assert!(files.contains(std::path::Path::new("a2.opus")));
}

#[test]
fn bookmarks_round_trip_and_sort_by_position_then_creation() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a'?");
    let b = book(&lib, "b");
    assert!(lib.bookmarks(&a).unwrap().is_empty());
    lib.add_bookmark(&b, 0, 0, None, "other", "", 0).unwrap();
    let last = lib.add_bookmark(&a, 2, 0, None, "last", "", 0).unwrap();
    let later = lib.add_bookmark(&a, 1, 9, None, "later", "", 30).unwrap();
    let first = lib
        .add_bookmark(&a, 1, 8, Some("seg'"), "Excerpt 雪", "note'", 99)
        .unwrap();
    let earlier = lib.add_bookmark(&a, 1, 9, None, "earlier", "", 20).unwrap();
    let marks = lib.bookmarks(&a).unwrap();
    assert_eq!(
        marks.iter().map(|m| m.id).collect::<Vec<_>>(),
        vec![first, earlier, later, last]
    );
    assert_eq!(
        marks[0],
        Bookmark {
            id: first,
            book_id: a,
            chapter: 1,
            char_offset: 8,
            segment_id: Some("seg'".into()),
            excerpt: "Excerpt 雪".into(),
            note: "note'".into(),
            created_at: 99,
        }
    );
}

#[test]
fn a_books_listening_comes_back_by_day_oldest_first() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    let b = book(&lib, "b");
    lib.record_listening(&a, "2026-11-10", 300).unwrap();
    lib.record_listening(&a, "2026-11-03", 100).unwrap();
    lib.record_listening(&a, "2026-11-10", 50).unwrap();
    lib.record_listening(&b, "2026-11-04", 999).unwrap();
    let day = |day: &str, ms| DayListening {
        day: day.into(),
        ms,
    };
    assert_eq!(
        lib.book_listening(&a).unwrap(),
        vec![day("2026-11-03", 100), day("2026-11-10", 350)]
    );
    assert!(lib
        .book_listening(&BookId("none".into()))
        .unwrap()
        .is_empty());
}
