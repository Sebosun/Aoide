//! Schema v6: a book's paired recording, its transcript in blocks, and its sync map.
use aoide_library::{BookId, Library, NewBook, RecordingRow, StoredSyncMap, TranscriptBlock};

fn book(lib: &Library, id: &str) -> BookId {
    let id = BookId(id.into());
    lib.upsert_book(
        &NewBook {
            id: id.clone(),
            path: format!("{}.epub", id.0).into(),
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

fn recording(book: &BookId, added_at: i64) -> RecordingRow {
    RecordingRow {
        book_id: book.clone(),
        files: r#"[{"path":"/audio/part1.mp3"}]"#.into(),
        total_ms: 3_600_000,
        stage: "checking".into(),
        error: None,
        accepted: false,
        added_at,
    }
}

fn block(start_ms: u64, end_ms: u64) -> TranscriptBlock {
    TranscriptBlock {
        start_ms,
        end_ms,
        transcriber: "whisper.cpp:ggml-base.en.bin".into(),
        words: format!(r#"[[{start_ms},{},"lamp"]]"#, start_ms + 300),
        levels: vec![0, 120, 255],
    }
}

#[test]
fn a_recording_is_saved_listed_in_the_order_added_and_its_stage_changed() {
    let lib = Library::open_in_memory().unwrap();
    let (a, b) = (book(&lib, "a"), book(&lib, "b"));
    lib.save_recording(&recording(&b, 20)).unwrap();
    lib.save_recording(&recording(&a, 30)).unwrap();
    assert_eq!(lib.recording(&a).unwrap(), Some(recording(&a, 30)));
    let order: Vec<BookId> = lib
        .recordings()
        .unwrap()
        .into_iter()
        .map(|r| r.book_id)
        .collect();
    assert_eq!(order, [b.clone(), a.clone()]);

    lib.set_recording_stage(&a, "failed", Some("no model"))
        .unwrap();
    lib.set_recording_accepted(&a, true).unwrap();
    let changed = lib.recording(&a).unwrap().unwrap();
    assert_eq!(
        (
            changed.stage.as_str(),
            changed.error.as_deref(),
            changed.accepted
        ),
        ("failed", Some("no model"), true)
    );
}

#[test]
fn transcript_blocks_and_the_sync_map_are_kept_until_cleared() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    lib.save_recording(&recording(&a, 1)).unwrap();
    lib.add_transcript_block(&a, &block(300_000, 600_000))
        .unwrap();
    lib.add_transcript_block(&a, &block(0, 300_000)).unwrap();
    assert_eq!(
        lib.transcript_blocks(&a).unwrap(),
        [block(0, 300_000), block(300_000, 600_000)]
    );

    let map = StoredSyncMap {
        text_hash: "abc".into(),
        map: vec![1, 2, 3],
        report: "{}".into(),
        created_at: 5,
    };
    lib.save_sync_map(&a, &map).unwrap();
    assert_eq!(lib.sync_map(&a).unwrap(), Some(map.clone()));
    assert!(lib.sync_data_bytes().unwrap() > 0);

    lib.clear_sync_map(&a).unwrap();
    assert_eq!(lib.sync_map(&a).unwrap(), None);
    lib.clear_transcript(&a).unwrap();
    assert!(lib.transcript_blocks(&a).unwrap().is_empty());
}

#[test]
fn removing_the_recording_or_its_book_removes_everything_stored_for_it() {
    let lib = Library::open_in_memory().unwrap();
    let (a, b) = (book(&lib, "a"), book(&lib, "b"));
    for id in [&a, &b] {
        lib.save_recording(&recording(id, 1)).unwrap();
        lib.add_transcript_block(id, &block(0, 1_000)).unwrap();
        lib.save_sync_map(
            id,
            &StoredSyncMap {
                text_hash: "h".into(),
                map: vec![],
                report: "{}".into(),
                created_at: 1,
            },
        )
        .unwrap();
    }
    lib.remove_recording(&a).unwrap();
    assert_eq!(lib.recording(&a).unwrap(), None);
    assert!(lib.transcript_blocks(&a).unwrap().is_empty());
    assert_eq!(lib.sync_map(&a).unwrap(), None);

    lib.remove_book(&b).unwrap();
    assert!(lib.recordings().unwrap().is_empty());
    assert!(lib.transcript_blocks(&b).unwrap().is_empty());
    assert_eq!(lib.sync_data_bytes().unwrap(), 0);
}

#[test]
fn a_version_5_library_is_upgraded_with_its_books_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    {
        let lib = Library::open(&path).unwrap();
        book(&lib, "a");
    }
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "DROP TABLE sync_maps; DROP TABLE recording_blocks; DROP TABLE recordings; \
         PRAGMA user_version = 5;",
    )
    .unwrap();
    drop(conn);
    let lib = Library::open(&path).unwrap();
    assert!(lib.book(&BookId("a".into())).unwrap().is_some());
    lib.save_recording(&recording(&BookId("a".into()), 1))
        .unwrap();
}
