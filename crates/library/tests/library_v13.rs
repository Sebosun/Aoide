//! Schema v13: audio per book is summed from an index.
use aoide_library::Library;

#[test]
fn a_version_12_library_sums_each_books_audio_from_an_index() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "DROP INDEX segments_book_audio;
         CREATE INDEX segments_book_id ON segments(book_id);
         PRAGMA user_version = 12;",
    )
    .unwrap();
    drop(conn);
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    let plan: Vec<String> = conn
        .prepare("EXPLAIN QUERY PLAN SELECT book_id, SUM(bytes), MAX(created_at) FROM segments GROUP BY book_id")
        .unwrap()
        .query_map([], |row| row.get(3))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(
        plan.iter()
            .any(|step| step.contains("COVERING INDEX segments_book_audio")),
        "{plan:?}"
    );
}
