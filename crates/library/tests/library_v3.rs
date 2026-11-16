//! Schema v3: cached online lookups.
use aoide_library::Library;

#[test]
fn lookups_are_cached_by_query_and_replaced() {
    let lib = Library::open_in_memory().unwrap();
    assert_eq!(lib.cached_lookup("isbn:123").unwrap(), None);
    lib.store_lookup("isbn:123", "[1]", 10).unwrap();
    lib.store_lookup("title:quiet", "[]", 11).unwrap();
    assert_eq!(
        lib.cached_lookup("isbn:123").unwrap(),
        Some(("[1]".into(), 10))
    );
    lib.store_lookup("isbn:123", "[2]", 20).unwrap();
    assert_eq!(
        lib.cached_lookup("isbn:123").unwrap(),
        Some(("[2]".into(), 20))
    );
    assert_eq!(
        lib.cached_lookup("title:quiet").unwrap(),
        Some(("[]".into(), 11))
    );
}

#[test]
fn version_two_databases_gain_the_cache_and_keep_their_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v2.db");
    {
        let lib = Library::open(&path).unwrap();
        lib.set_value(aoide_library::GlobalKey::IgnoredBooks, &["kept"])
            .unwrap();
    }
    // Simulate a database written by the v2 schema: drop the v3+ tables and version.
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("DROP TABLE IF EXISTS sites; DROP TABLE IF EXISTS feed_entries; DROP TABLE IF EXISTS feeds; ALTER TABLE books DROP COLUMN shelf; DROP TABLE IF EXISTS articles; ALTER TABLE books DROP COLUMN kind; ALTER TABLE progress DROP COLUMN position_ms; ALTER TABLE bookmarks DROP COLUMN position_ms; DROP TABLE IF EXISTS reading; ALTER TABLE books DROP COLUMN finished_at; ALTER TABLE books DROP COLUMN word_count; DROP TABLE IF EXISTS book_values; DROP TABLE IF EXISTS sync_maps; DROP TABLE IF EXISTS recording_blocks; DROP TABLE IF EXISTS recordings; DROP TABLE IF EXISTS book_collections; DROP TABLE IF EXISTS collections; DROP TABLE IF EXISTS discovered_files; DROP TABLE IF EXISTS metadata_cache; DROP TABLE IF EXISTS book_tags; DROP TABLE IF EXISTS tags; DROP TABLE IF EXISTS segment_takes; PRAGMA user_version = 2;")
        .unwrap();
    drop(conn);
    let lib = Library::open(&path).unwrap();
    assert_eq!(
        lib.value(aoide_library::GlobalKey::IgnoredBooks).unwrap(),
        Some(vec!["kept".to_owned()])
    );
    lib.store_lookup("k", "v", 1).unwrap();
    assert_eq!(lib.cached_lookup("k").unwrap(), Some(("v".into(), 1)));
}
