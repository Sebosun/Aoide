//! `cargo run --release -p aoide-discovery --example scan -- <folder>`: times a scan and hashing.
use std::sync::atomic::AtomicBool;
use std::time::Instant;

fn main() {
    let root = std::env::args().nth(1).expect("folder to scan");
    let options = aoide_discovery::ScanOptions {
        roots: vec![root.into()],
        ..Default::default()
    };
    let started = Instant::now();
    let found = aoide_discovery::find_epubs(&options, &AtomicBool::new(false), &|_| {});
    let walked = started.elapsed();
    let bytes: u64 = found.iter().map(|f| f.size).sum();
    let paths = found.iter().map(|f| f.path.clone()).collect();
    let started = Instant::now();
    let hashed = aoide_discovery::hash_all(paths, 4, &AtomicBool::new(false), &|_| {});
    println!(
        "found {} EPUBs ({:.1} MB) in {walked:?}; hashed {} in {:?}",
        found.len(),
        bytes as f64 / 1e6,
        hashed.iter().filter(|h| h.1.is_ok()).count(),
        started.elapsed()
    );
}
