use std::path::PathBuf;

use aoide_library::{
    content_hash, scan_folder, BookId, BookStatus, CachedSegment, Library, NewBook, Progress,
    WordTiming,
};

fn book(id: &str, path: &str) -> NewBook {
    NewBook {
        id: BookId(id.into()),
        path: PathBuf::from(path),
        title: Some(format!("Title {id}")),
        authors: vec!["A. Author".into(), "B. Author".into()],
        language: Some("en".into()),
        cover_path: None,
        error: None,
    }
}

fn segment(key: &str, book: &str, bytes: u64) -> CachedSegment {
    CachedSegment {
        key: key.into(),
        book_id: BookId(book.into()),
        file: PathBuf::from(format!("{book}/{key}.opus")),
        duration_ms: 1500,
        bytes,
        word_timings: None,
        created_at: 10,
    }
}

#[test]
fn content_hash_is_lowercase_hex_sha256() {
    assert_eq!(
        content_hash(b"abc").0,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn upserted_books_round_trip_with_status_and_authors() {
    let lib = Library::open_in_memory().unwrap();
    lib.upsert_book(&book("a", "/books/a.epub"), 100).unwrap();
    let record = lib.book(&BookId("a".into())).unwrap().unwrap();
    assert_eq!(record.path, PathBuf::from("/books/a.epub"));
    assert_eq!(record.status, BookStatus::Present);
    assert_eq!(record.authors, vec!["A. Author", "B. Author"]);
    assert_eq!(record.title.as_deref(), Some("Title a"));
    assert_eq!(record.language.as_deref(), Some("en"));
    assert_eq!(record.added_at, 100);
    assert_eq!(record.last_opened_at, None);
    assert!(lib.book(&BookId("zzz".into())).unwrap().is_none());
}

#[test]
fn upsert_updates_path_and_keeps_added_at() {
    let lib = Library::open_in_memory().unwrap();
    lib.upsert_book(&book("a", "/books/a.epub"), 100).unwrap();
    lib.upsert_book(&book("a", "/books/moved/a.epub"), 999)
        .unwrap();
    let records = lib.books().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].path, PathBuf::from("/books/moved/a.epub"));
    assert_eq!(records[0].added_at, 100);
}

#[test]
fn error_books_carry_their_message() {
    let lib = Library::open_in_memory().unwrap();
    let mut broken = book("b", "/books/b.epub");
    broken.error = Some("invalid EPUB: no container".into());
    lib.upsert_book(&broken, 1).unwrap();
    assert_eq!(
        lib.book(&BookId("b".into())).unwrap().unwrap().status,
        BookStatus::Error("invalid EPUB: no container".into())
    );
}

#[test]
fn books_are_ordered_by_last_opened_then_added() {
    let lib = Library::open_in_memory().unwrap();
    lib.upsert_book(&book("old", "/o.epub"), 1).unwrap();
    lib.upsert_book(&book("new", "/n.epub"), 2).unwrap();
    lib.upsert_book(&book("opened", "/p.epub"), 0).unwrap();
    lib.set_last_opened(&BookId("opened".into()), 50).unwrap();
    let order: Vec<String> = lib.books().unwrap().into_iter().map(|b| b.id.0).collect();
    assert_eq!(order, vec!["opened", "new", "old"]);
}

#[test]
fn missing_books_are_marked_and_reconnect_on_upsert() {
    let lib = Library::open_in_memory().unwrap();
    lib.upsert_book(&book("a", "/a.epub"), 1).unwrap();
    lib.upsert_book(&book("b", "/b.epub"), 1).unwrap();
    let mut broken = book("c", "/c.epub");
    broken.error = Some("bad".into());
    lib.upsert_book(&broken, 1).unwrap();

    let changed = lib.mark_missing_except(&[BookId("a".into())]).unwrap();
    assert_eq!(changed, vec![BookId("b".into())]);
    assert_eq!(
        lib.book(&BookId("b".into())).unwrap().unwrap().status,
        BookStatus::Missing
    );
    assert_eq!(
        lib.book(&BookId("c".into())).unwrap().unwrap().status,
        BookStatus::Error("bad".into())
    );
    // Same content reappears elsewhere.
    lib.upsert_book(&book("b", "/elsewhere/b.epub"), 7).unwrap();
    let b = lib.book(&BookId("b".into())).unwrap().unwrap();
    assert_eq!(b.status, BookStatus::Present);
    assert_eq!(b.path, PathBuf::from("/elsewhere/b.epub"));
    assert!(lib
        .mark_missing_except(&[BookId("a".into()), BookId("b".into())])
        .unwrap()
        .is_empty());
}

#[test]
fn progress_round_trips_and_is_replaced() {
    let lib = Library::open_in_memory().unwrap();
    lib.upsert_book(&book("a", "/a.epub"), 1).unwrap();
    let id = BookId("a".into());
    assert!(lib.progress(&id).unwrap().is_none());
    let first = Progress {
        chapter: 2,
        char_offset: 40,
        segment_id: Some("2:3".into()),
        updated_at: 5,
    };
    lib.save_progress(&id, &first).unwrap();
    assert_eq!(lib.progress(&id).unwrap(), Some(first));
    let second = Progress {
        chapter: 3,
        char_offset: 0,
        segment_id: None,
        updated_at: 6,
    };
    lib.save_progress(&id, &second).unwrap();
    assert_eq!(lib.progress(&id).unwrap(), Some(second));
}

#[test]
fn segments_round_trip_including_word_timings() {
    let lib = Library::open_in_memory().unwrap();
    lib.upsert_book(&book("a", "/a.epub"), 1).unwrap();
    let mut seg = segment("k1", "a", 300);
    seg.word_timings = Some(vec![
        WordTiming {
            start_ms: 0,
            end_ms: 400,
        },
        WordTiming {
            start_ms: 400,
            end_ms: 1500,
        },
    ]);
    lib.record_segment(&seg).unwrap();
    assert_eq!(lib.segment("k1").unwrap(), Some(seg.clone()));
    assert!(lib.segment("nope").unwrap().is_none());

    let mut replaced = seg.clone();
    replaced.bytes = 999;
    replaced.word_timings = None;
    lib.record_segment(&replaced).unwrap();
    assert_eq!(lib.segment("k1").unwrap(), Some(replaced));
}

#[test]
fn cached_keys_preserves_query_order_and_filters_missing() {
    let lib = Library::open_in_memory().unwrap();
    lib.upsert_book(&book("a", "/a.epub"), 1).unwrap();
    lib.record_segment(&segment("k1", "a", 1)).unwrap();
    lib.record_segment(&segment("k3", "a", 1)).unwrap();
    let keys: Vec<String> = ["k3", "k2", "k1"].iter().map(|s| s.to_string()).collect();
    assert_eq!(lib.cached_keys(&keys).unwrap(), vec!["k3", "k1"]);
    assert!(lib.cached_keys(&[]).unwrap().is_empty());
}

#[test]
fn audio_bytes_and_segment_deletion_are_per_book() {
    let lib = Library::open_in_memory().unwrap();
    lib.upsert_book(&book("a", "/a.epub"), 1).unwrap();
    lib.upsert_book(&book("b", "/b.epub"), 1).unwrap();
    lib.record_segment(&segment("k1", "a", 100)).unwrap();
    lib.record_segment(&segment("k2", "a", 50)).unwrap();
    lib.record_segment(&segment("k3", "b", 7)).unwrap();
    assert_eq!(lib.book_audio_bytes(&BookId("a".into())).unwrap(), 150);
    assert_eq!(lib.book_audio_bytes(&BookId("zzz".into())).unwrap(), 0);

    let mut files = lib.delete_book_segments(&BookId("a".into())).unwrap();
    files.sort();
    assert_eq!(
        files,
        vec![PathBuf::from("a/k1.opus"), PathBuf::from("a/k2.opus")]
    );
    assert_eq!(lib.book_audio_bytes(&BookId("a".into())).unwrap(), 0);
    assert!(lib.segment("k3").unwrap().is_some());
}

#[test]
fn removing_a_book_drops_progress_and_segments_and_returns_files() {
    let lib = Library::open_in_memory().unwrap();
    lib.upsert_book(&book("a", "/a.epub"), 1).unwrap();
    let id = BookId("a".into());
    lib.save_progress(
        &id,
        &Progress {
            chapter: 0,
            char_offset: 1,
            segment_id: None,
            updated_at: 1,
        },
    )
    .unwrap();
    lib.record_segment(&segment("k1", "a", 1)).unwrap();
    assert_eq!(
        lib.remove_book(&id).unwrap(),
        vec![PathBuf::from("a/k1.opus")]
    );
    assert!(lib.book(&id).unwrap().is_none());
    assert!(lib.progress(&id).unwrap().is_none());
    assert!(lib.segment("k1").unwrap().is_none());
}

#[test]
fn settings_are_key_value_and_overwritable() {
    let lib = Library::open_in_memory().unwrap();
    assert_eq!(lib.setting("theme").unwrap(), None);
    lib.set_setting("theme", "dark").unwrap();
    lib.set_setting("theme", "light").unwrap();
    assert_eq!(lib.setting("theme").unwrap().as_deref(), Some("light"));
}

#[test]
fn database_file_persists_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite3");
    {
        let lib = Library::open(&path).unwrap();
        lib.upsert_book(&book("a", "/a.epub"), 1).unwrap();
        lib.set_setting("k", "v").unwrap();
    }
    let lib = Library::open(&path).unwrap();
    assert_eq!(lib.books().unwrap().len(), 1);
    assert_eq!(lib.setting("k").unwrap().as_deref(), Some("v"));
}

#[test]
fn scan_finds_epubs_recursively_skipping_hidden_and_other_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::create_dir_all(root.join(".hidden")).unwrap();
    std::fs::write(root.join("b.epub"), b"bbb").unwrap();
    std::fs::write(root.join("sub/A.EPUB"), b"abc").unwrap();
    std::fs::write(root.join("notes.txt"), b"x").unwrap();
    std::fs::write(root.join(".hidden/c.epub"), b"c").unwrap();
    std::fs::write(root.join(".d.epub"), b"d").unwrap();

    let found = scan_folder(root).unwrap();
    let paths: Vec<PathBuf> = found
        .iter()
        .map(|f| f.path.strip_prefix(root).unwrap().to_path_buf())
        .collect();
    assert_eq!(
        paths,
        vec![PathBuf::from("b.epub"), PathBuf::from("sub/A.EPUB")]
    );
    assert_eq!(found[1].id, content_hash(b"abc"));
}

#[test]
fn scanning_a_missing_folder_yields_nothing() {
    let dir = tempfile::tempdir().unwrap();
    assert!(scan_folder(&dir.path().join("absent")).unwrap().is_empty());
}
