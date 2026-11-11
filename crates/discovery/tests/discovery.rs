//! Finding and identifying EPUBs on disk, against temporary folders.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use aoide_discovery::{find_epubs, hash_all, hash_file, ScanOptions};
use sha2::{Digest, Sha256};

fn touch(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, bytes).unwrap();
    path
}

fn options(root: &Path) -> ScanOptions {
    ScanOptions {
        roots: vec![root.to_path_buf()],
        ..ScanOptions::default()
    }
}

fn names(root: &Path, found: &[aoide_discovery::Found]) -> Vec<String> {
    found
        .iter()
        .map(|f| {
            f.path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

#[test]
fn finds_epubs_and_skips_hidden_build_and_excluded_folders() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, "Books/one.epub", b"one");
    touch(root, "Downloads/Two.EPUB", b"two!");
    touch(root, "Downloads/notes.txt", b"x");
    touch(root, ".cache/hidden.epub", b"h");
    touch(root, "code/node_modules/pkg/dep.epub", b"d");
    touch(root, "code/target/out.epub", b"t");
    touch(root, "Aoide/Books/already.epub", b"a");
    let mut opts = options(root);
    opts.exclude.push(root.join("Aoide/Books"));
    let found = find_epubs(&opts, &AtomicBool::new(false), &|_| {});
    assert_eq!(
        names(root, &found),
        vec!["Books/one.epub", "Downloads/Two.EPUB"]
    );
    assert_eq!(found[1].size, 4);
    assert!(found[0].inode > 0 && found[0].modified_ns > 0);
}

#[test]
fn allowed_hidden_folders_are_searched() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, ".local/share/reader/book.epub", b"r");
    let mut opts = options(root);
    opts.include_hidden.push(root.join(".local/share/reader"));
    let found = find_epubs(&opts, &AtomicBool::new(false), &|_| {});
    assert_eq!(names(root, &found), vec![".local/share/reader/book.epub"]);
}

#[cfg(unix)]
#[test]
fn symlinks_are_not_followed_so_loops_cannot_hang() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, "a/book.epub", b"b");
    std::os::unix::fs::symlink(root, root.join("a/loop")).unwrap();
    std::os::unix::fs::symlink(root.join("a/book.epub"), root.join("link.epub")).unwrap();
    let found = find_epubs(&options(root), &AtomicBool::new(false), &|_| {});
    assert_eq!(names(root, &found), vec!["a/book.epub"]);
}

#[test]
fn cancelling_stops_the_walk_and_progress_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for i in 0..50 {
        touch(root, &format!("d{i}/b.epub"), b"x");
    }
    let calls = AtomicU64::new(0);
    let last_found = AtomicU64::new(0);
    let found = find_epubs(&options(root), &AtomicBool::new(false), &|p| {
        calls.fetch_add(1, Ordering::SeqCst);
        last_found.store(p.found, Ordering::SeqCst);
    });
    assert_eq!(found.len(), 50);
    assert!(calls.load(Ordering::SeqCst) >= 1);
    assert_eq!(
        last_found.load(Ordering::SeqCst),
        50,
        "final progress has the totals"
    );
    assert!(find_epubs(&options(root), &AtomicBool::new(true), &|_| {}).is_empty());
}

#[test]
fn hashes_match_the_library_content_hash() {
    let dir = tempfile::tempdir().unwrap();
    let big: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
    let path = touch(dir.path(), "big.epub", &big);
    assert_eq!(
        hash_file(&path).unwrap(),
        format!("{:x}", Sha256::digest(&big))
    );
    let missing = dir.path().join("missing.epub");
    let results = hash_all(
        vec![path.clone(), missing.clone()],
        4,
        &AtomicBool::new(false),
        &|_| {},
    );
    assert_eq!(results[0].0, path);
    assert!(results[0].1.is_ok());
    assert_eq!(results[1].0, missing);
    assert!(results[1].1.is_err());
}
