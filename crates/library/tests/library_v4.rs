//! Schema v4: files found by scanning the computer, remembered between scans.
use std::path::PathBuf;

use aoide_library::{DiscoveredFile, Library};

fn file(path: &str, seen_at: i64) -> DiscoveredFile {
    DiscoveredFile {
        path: PathBuf::from(path),
        size: 1234,
        modified_ns: 5_000_000_000,
        inode: 77,
        hash: Some("abc".into()),
        title: Some("Quiet Places".into()),
        authors: vec!["Ellis Reed".into()],
        language: Some("en".into()),
        isbn: None,
        cover_path: Some(PathBuf::from("/covers/discovered/abc.jpg")),
        error: None,
        seen_at,
    }
}

#[test]
fn discovered_files_round_trip_and_replace_by_path() {
    let lib = Library::open_in_memory().unwrap();
    assert_eq!(lib.discovered("/home/a.epub".as_ref()).unwrap(), None);
    let a = file("/home/a.epub", 1);
    lib.upsert_discovered(&a).unwrap();
    assert_eq!(
        lib.discovered("/home/a.epub".as_ref()).unwrap(),
        Some(a.clone())
    );
    let mut changed = a.clone();
    changed.hash = None;
    changed.error = Some("not an EPUB".into());
    changed.authors = vec![];
    lib.upsert_discovered(&changed).unwrap();
    assert_eq!(
        lib.discovered("/home/a.epub".as_ref()).unwrap(),
        Some(changed)
    );
}

#[test]
fn files_not_seen_in_the_latest_scan_are_forgotten() {
    let lib = Library::open_in_memory().unwrap();
    lib.upsert_discovered(&file("/home/b.epub", 10)).unwrap();
    lib.upsert_discovered(&file("/home/a.epub", 20)).unwrap();
    lib.upsert_discovered(&file("/home/c.epub", 20)).unwrap();
    assert_eq!(lib.forget_discovered_before(20).unwrap(), 1);
    let paths: Vec<PathBuf> = lib
        .discovered_files()
        .unwrap()
        .into_iter()
        .map(|f| f.path)
        .collect();
    assert_eq!(
        paths,
        vec![PathBuf::from("/home/a.epub"), PathBuf::from("/home/c.epub")]
    );
}

#[test]
fn seen_at_can_be_refreshed_for_unchanged_files() {
    let lib = Library::open_in_memory().unwrap();
    lib.upsert_discovered(&file("/home/a.epub", 1)).unwrap();
    lib.mark_discovered_seen(&[PathBuf::from("/home/a.epub")], 9)
        .unwrap();
    assert_eq!(
        lib.discovered("/home/a.epub".as_ref())
            .unwrap()
            .unwrap()
            .seen_at,
        9
    );
}
