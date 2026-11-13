//! Prints what Aoide finds in an audiobook, and how fast it decodes.
//!
//! `cargo run --release -p aoide-recording --example inspect -- <files or folders>…`

use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let started = Instant::now();
    let found = aoide_recording::inspect(&paths);
    println!("inspected in {:.1?}", started.elapsed());
    println!("title: {:?}, author: {:?}", found.title, found.author);
    for file in &found.files {
        println!(
            "  {} {} Hz × {}, {:.1} s",
            file.codec,
            file.sample_rate,
            file.channels,
            file.duration_us() as f64 / 1e6
        );
    }
    for problem in &found.problems {
        println!("  left out: {:?}", problem.problem);
    }
    println!(
        "{} chapters, {:.1} h in all",
        found.chapters.len(),
        found.total_ms() as f64 / 3.6e6
    );
    let mut reader = aoide_recording::Reader::new(found.files.clone());
    let span = 600_000.min(reader.total_ms());
    let started = Instant::now();
    let samples = reader.read(0..span, 16_000).expect("decodes");
    let took = started.elapsed().as_secs_f64();
    println!(
        "decoded {} s at 16 kHz in {:.2} s ({:.0}× real time)",
        span / 1000,
        took,
        span as f64 / 1000.0 / took
    );
    let middle = reader.total_ms() / 2;
    let started = Instant::now();
    let _ = reader.read(middle..middle + 5_000, 48_000).expect("seeks");
    println!(
        "jumped to the middle and read 5 s at 48 kHz in {:.0?}",
        started.elapsed()
    );
    let _ = samples;
}
