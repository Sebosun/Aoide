//! Merging an audiobook on its own into the book it reads: everything the audiobook has moves
//! to the book in one transaction, and the audiobook is gone.
use aoide_library::{
    BookId, BookKey, ItemKind, Library, NewBook, Progress, RecordingRow, TranscriptBlock,
};

fn item(lib: &Library, id: &str, kind: ItemKind, added_at: i64) -> BookId {
    let id = BookId(id.into());
    lib.upsert_book(
        &NewBook {
            id: id.clone(),
            kind,
            path: format!("{}.item", id.0).into(),
            title: Some("Synthetic item".into()),
            authors: vec![],
            language: None,
            cover_path: None,
            error: None,
        },
        added_at,
    )
    .unwrap();
    id
}

#[test]
fn the_audiobook_gives_the_book_everything_it_has_and_is_removed() {
    let lib = Library::open_in_memory().unwrap();
    let audio = item(&lib, "audio", ItemKind::Audiobook, 10);
    let book = item(&lib, "book", ItemKind::Book, 50);
    lib.save_recording(&RecordingRow {
        book_id: audio.clone(),
        files: "[]".into(),
        total_ms: 90_000,
        stage: "standalone".into(),
        error: None,
        accepted: false,
        added_at: 11,
    })
    .unwrap();
    lib.add_transcript_block(
        &audio,
        &TranscriptBlock {
            start_ms: 0,
            end_ms: 1_000,
            transcriber: "t".into(),
            words: "[]".into(),
            levels: vec![1, 2],
        },
    )
    .unwrap();
    lib.save_progress(
        &audio,
        &Progress {
            chapter: 2,
            char_offset: 0,
            segment_id: None,
            updated_at: 20,
            position_ms: Some(42_000),
        },
    )
    .unwrap();
    lib.add_time_bookmark(&audio, 2, 41_000, "Part Three", 21)
        .unwrap();
    let night = lib.create_collection("Night", 1).unwrap();
    let walks = lib.create_collection("Walks", 1).unwrap();
    lib.set_book_collections(&audio, &[night, walks]).unwrap();
    lib.set_book_collections(&book, &[night]).unwrap();
    lib.record_listening(&audio, "2026-11-14", 5_000).unwrap();
    lib.record_listening(&book, "2026-11-14", 1_000).unwrap();
    lib.record_listening(&audio, "2026-11-15", 2_000).unwrap();

    lib.merge_audiobook(&audio, &book).unwrap();

    assert!(lib.book(&audio).unwrap().is_none(), "the audiobook is gone");
    assert!(lib.recording(&audio).unwrap().is_none());
    let recording = lib.recording(&book).unwrap().unwrap();
    assert_eq!((recording.total_ms, recording.added_at), (90_000, 11));
    assert_eq!(lib.transcript_blocks(&book).unwrap().len(), 1);
    let marks = lib.bookmarks(&book).unwrap();
    assert_eq!(marks.len(), 1);
    assert_eq!(
        (
            marks[0].chapter,
            marks[0].position_ms,
            marks[0].excerpt.as_str()
        ),
        (2, Some(41_000), "Part Three")
    );
    let mut shelves = lib.book_collections().unwrap().remove(&book).unwrap();
    shelves.sort();
    assert_eq!(shelves, [night, walks]);
    let days: Vec<(String, u64)> = lib
        .book_listening(&book)
        .unwrap()
        .into_iter()
        .map(|d| (d.day, d.ms))
        .collect();
    assert_eq!(
        days,
        [("2026-11-14".into(), 6_000), ("2026-11-15".into(), 2_000)]
    );
    assert_eq!(
        lib.book(&book).unwrap().unwrap().added_at,
        10,
        "the earlier"
    );
    assert_eq!(
        lib.book_value::<u64>(&book, BookKey::PendingPlace).unwrap(),
        Some(42_000)
    );
    assert!(
        lib.progress(&book).unwrap().is_none(),
        "the book's place waits for the sync"
    );
}

#[test]
fn an_audiobook_never_played_leaves_no_pending_place() {
    let lib = Library::open_in_memory().unwrap();
    let audio = item(&lib, "audio", ItemKind::Audiobook, 10);
    let book = item(&lib, "book", ItemKind::Book, 5);
    lib.merge_audiobook(&audio, &book).unwrap();
    assert_eq!(
        lib.book_value::<u64>(&book, BookKey::PendingPlace).unwrap(),
        None
    );
    assert_eq!(lib.book(&book).unwrap().unwrap().added_at, 5);
}
