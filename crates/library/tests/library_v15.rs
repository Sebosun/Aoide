//! Schema v15: columns added by editing shipped migrations (v10 `articles.lead`, v11
//! `feeds.failing_since`): a library that ran those steps before the edit lacks them, a newer
//! one has them. Both must open.
use aoide_library::Library;

fn columns(conn: &rusqlite::Connection, table: &str) -> Vec<String> {
    conn.prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn a_version_14_library_whose_articles_lack_a_lead_gains_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "ALTER TABLE articles DROP COLUMN lead;
         DROP TABLE IF EXISTS book_tags; DROP TABLE IF EXISTS tags; DROP TABLE IF EXISTS segment_takes; DROP TABLE IF EXISTS pronunciations; DROP TABLE IF EXISTS highlights; PRAGMA user_version = 14;",
    )
    .unwrap();
    drop(conn);
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    assert!(columns(&conn, "articles").contains(&"lead".to_owned()));
}

#[test]
fn a_version_14_library_that_already_has_a_lead_opens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "DROP TABLE IF EXISTS book_tags; DROP TABLE IF EXISTS tags; DROP TABLE IF EXISTS segment_takes; DROP TABLE IF EXISTS pronunciations; DROP TABLE IF EXISTS highlights; PRAGMA user_version = 14;",
    )
    .unwrap();
    drop(conn);
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 26);
    assert!(columns(&conn, "articles").contains(&"lead".to_owned()));
}

/// `feeds.failing_since` was added the same way, by editing the v11 migration.
#[test]
fn a_version_14_library_whose_feeds_lack_failing_since_gains_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "ALTER TABLE feeds DROP COLUMN failing_since;
         DROP TABLE IF EXISTS book_tags; DROP TABLE IF EXISTS tags; DROP TABLE IF EXISTS segment_takes; DROP TABLE IF EXISTS pronunciations; DROP TABLE IF EXISTS highlights; PRAGMA user_version = 14;",
    )
    .unwrap();
    drop(conn);
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    assert!(columns(&conn, "feeds").contains(&"failing_since".to_owned()));
}
