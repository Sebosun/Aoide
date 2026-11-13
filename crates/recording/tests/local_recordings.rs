//! Optional checks against an audiobook of your own. No recording is stored in the repository,
//! and nothing but lengths and counts is printed.
//!
//! ```sh
//! AOIDE_TEST_AUDIOBOOK=/path/to/book.m4b \   # a file, or a folder of files
//! cargo test --release -p aoide-recording --test local_recordings -- --ignored
//! ```
use std::path::PathBuf;

use aoide_recording::{inspect, Reader};

fn local_recording() -> Vec<PathBuf> {
    let path = std::env::var_os("AOIDE_TEST_AUDIOBOOK")
        .expect("set AOIDE_TEST_AUDIOBOOK to an audiobook file or folder");
    vec![PathBuf::from(path)]
}

#[test]
#[ignore = "set AOIDE_TEST_AUDIOBOOK to an audiobook of your own"]
fn a_local_recording_is_inspected_and_reads_the_same_after_a_jump() {
    let found = inspect(&local_recording());
    assert!(found.problems.is_empty(), "left out: {:?}", found.problems);
    assert!(!found.files.is_empty(), "no audio found");
    let total = found.total_ms();
    assert!(total > 60_000, "shorter than a minute: {total} ms");
    println!(
        "{} files, {} min, {} chapters",
        found.files.len(),
        total / 60_000,
        found.chapters.len()
    );

    // Read a stretch in passing, then jump away and back: the samples agree.
    let middle = total / 2;
    let mut reader = Reader::new(found.files.clone());
    let passing = reader.read(middle - 2_000..middle + 2_000, 16_000).unwrap();
    reader.read(1_000..2_000, 16_000).unwrap();
    let again = reader.read(middle..middle + 1_000, 16_000).unwrap();
    let offset = 2_000 * 16;
    let worst = again
        .iter()
        .zip(&passing[offset..offset + again.len()])
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
    assert!(worst < 1e-3, "samples differ by {worst} after a jump");

    // The very end is reachable, even where the file cannot be sought that far, and promptly.
    let started = std::time::Instant::now();
    let tail = reader.read(total - 5_000..total, 16_000).unwrap();
    assert_eq!(tail.len(), 5_000 * 16);
    let took = started.elapsed();
    println!("reading the last 5 s after a jump took {took:.2?}");
    assert!(took.as_secs() < 60, "reading the end took {took:?}");
}
