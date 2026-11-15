//! An audiobook's identity: the same files give the same fingerprint, wherever they are.
use std::path::PathBuf;

use aoide_recording::fingerprint;

fn file(dir: &std::path::Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

/// `len` bytes counting up, so every position has its own value.
fn counting(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|i| (i as u8).wrapping_add(seed)).collect()
}

#[test]
fn the_same_files_have_the_same_fingerprint_and_any_change_at_the_edges_or_order_shows() {
    let dir = tempfile::tempdir().unwrap();
    let big = counting(300_000, 1);
    let a = file(dir.path(), "a.mp3", &big);
    let b = file(dir.path(), "b.mp3", &counting(1_000, 7));
    let print = fingerprint(&[a.clone(), b.clone()]).unwrap();
    assert_eq!(print.len(), 64);
    assert!(print.chars().all(|c| c.is_ascii_hexdigit()));

    // The same files copied elsewhere.
    let other = tempfile::tempdir().unwrap();
    let a2 = file(other.path(), "first.mp3", &big);
    let b2 = file(other.path(), "second.mp3", &counting(1_000, 7));
    assert_eq!(fingerprint(&[a2.clone(), b2.clone()]).unwrap(), print);

    assert_ne!(
        fingerprint(&[b.clone(), a.clone()]).unwrap(),
        print,
        "order counts"
    );

    let mut longer = big.clone();
    longer.push(0);
    let grown = file(other.path(), "grown.mp3", &longer);
    assert_ne!(
        fingerprint(&[grown, b2.clone()]).unwrap(),
        print,
        "size counts"
    );

    let mut start = big.clone();
    start[10] ^= 0xff;
    let head = file(other.path(), "head.mp3", &start);
    assert_ne!(
        fingerprint(&[head, b2.clone()]).unwrap(),
        print,
        "the first 64 KiB count"
    );

    let mut end = big.clone();
    let last = end.len() - 10;
    end[last] ^= 0xff;
    let tail = file(other.path(), "tail.mp3", &end);
    assert_ne!(
        fingerprint(&[tail, b2.clone()]).unwrap(),
        print,
        "the last 64 KiB count"
    );

    // The middle of a long file is not read.
    let mut middle = big.clone();
    middle[150_000] ^= 0xff;
    let inner = file(other.path(), "inner.mp3", &middle);
    assert_eq!(fingerprint(&[inner, b2]).unwrap(), print);

    assert!(fingerprint(&[dir.path().join("gone.mp3")]).is_err());
}
