//! Phase-0 prototype: syncs an audiobook with its EPUB and reports how well it matched.
//!
//! ```sh
//! cargo run --release -p aoide-transcribe --features whisper --example sync_book -- \
//!     --model path/to/ggml-base.en.bin --transcript book.transcript \
//!     book.epub audiobook.m4b
//! ```
//!
//! The transcript file is appended to block by block, so an interrupted run carries on where it
//! stopped, and matching can be rerun without transcribing again. Options:
//! `--vad MODEL` (Silero VAD), `--threads N`, `--gpu`, `--language xx`, `--minutes N` (transcribe
//! only the first N minutes), `--clips DIR` (write sample sentence clips to listen to).

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use aoide_recording::{inspect, Reader};
use aoide_sync::{match_book, HeardWord, Timing};
use aoide_transcribe::{
    quietest_cut, Request, Transcriber, WhisperCpp, WhisperOptions, SAMPLE_RATE,
};

const BLOCK_MS: u64 = 300_000;
const CUT_SLACK_MS: u64 = 30_000;

struct Args {
    model: PathBuf,
    vad: Option<PathBuf>,
    threads: usize,
    gpu: bool,
    language: Option<String>,
    minutes: Option<u64>,
    transcript: PathBuf,
    clips: Option<PathBuf>,
    epub: PathBuf,
    audio: Vec<PathBuf>,
}

fn args() -> Args {
    let mut it = std::env::args_os().skip(1);
    let mut a = Args {
        model: PathBuf::new(),
        vad: None,
        threads: 0,
        gpu: false,
        language: None,
        minutes: None,
        transcript: PathBuf::from("book.transcript"),
        clips: None,
        epub: PathBuf::new(),
        audio: Vec::new(),
    };
    let mut rest = Vec::new();
    while let Some(arg) = it.next() {
        let mut value = || PathBuf::from(it.next().expect("option needs a value"));
        match arg.to_str() {
            Some("--model") => a.model = value(),
            Some("--vad") => a.vad = Some(value()),
            Some("--threads") => a.threads = value().to_str().unwrap().parse().unwrap(),
            Some("--gpu") => a.gpu = true,
            Some("--language") => a.language = Some(value().to_string_lossy().into_owned()),
            Some("--minutes") => a.minutes = Some(value().to_str().unwrap().parse().unwrap()),
            Some("--transcript") => a.transcript = value(),
            Some("--clips") => a.clips = Some(value()),
            _ => rest.push(PathBuf::from(arg)),
        }
    }
    assert!(
        rest.len() >= 2,
        "usage: sync_book --model M [options] BOOK.epub AUDIO…"
    );
    a.epub = rest.remove(0);
    a.audio = rest;
    a
}

fn clock(ms: u64) -> String {
    let s = ms / 1000;
    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

fn main() {
    let args = args();
    let book = aoide_epub::parse_epub(
        &std::fs::read(&args.epub).expect("read EPUB"),
        &aoide_epub::ParseOptions::default(),
    )
    .expect("parse EPUB");
    let language = args.language.clone().or_else(|| {
        book.language
            .as_deref()
            .map(|l| l[..2.min(l.len())].to_lowercase())
    });
    println!(
        "book: {} chapters, language {:?}",
        book.chapters.len(),
        language
    );

    let found = inspect(&args.audio);
    for problem in &found.problems {
        println!(
            "left out {:?}: {:?}",
            problem.path.file_name(),
            problem.problem
        );
    }
    let total_ms = found.total_ms();
    println!(
        "recording: {} files, {}, {} chapters",
        found.files.len(),
        clock(total_ms),
        found.chapters.len()
    );
    let mut reader = Reader::new(found.files.clone());

    // ---- transcription, resumable
    let mut heard: Vec<HeardWord> = Vec::new();
    let mut done_ms = 0;
    if let Ok(text) = std::fs::read_to_string(&args.transcript) {
        for line in text.lines() {
            let fields: Vec<&str> = line.splitn(4, '\t').collect();
            match fields.as_slice() {
                ["block", _, end] => done_ms = end.parse().unwrap(),
                [start, end, _p, word] => heard.push(HeardWord {
                    text: (*word).to_owned(),
                    start_ms: start.parse().unwrap(),
                    end_ms: end.parse().unwrap(),
                }),
                _ => {}
            }
        }
        println!("transcript: {} words up to {}", heard.len(), clock(done_ms));
    }
    let limit_ms = args
        .minutes
        .map_or(total_ms, |m| (m * 60_000).min(total_ms));
    if done_ms < limit_ms {
        let whisper = WhisperCpp::new(WhisperOptions {
            model: args.model.clone(),
            threads: args.threads,
            use_gpu: args.gpu,
            vad_model: args.vad.clone(),
        })
        .expect("load model");
        println!("transcribing with {}", whisper.id());
        let mut out = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&args.transcript)
            .expect("open transcript");
        let started = Instant::now();
        let first_ms = done_ms;
        while done_ms < limit_ms {
            // Each block ends near `target`, at the quietest point within the slack around it.
            let target = (done_ms + BLOCK_MS).min(limit_ms);
            let read_to = (target + CUT_SLACK_MS).min(total_ms);
            let samples = reader.read(done_ms..read_to, SAMPLE_RATE).expect("decode");
            let per_ms = SAMPLE_RATE as usize / 1000;
            let cut = if read_to == total_ms {
                samples.len()
            } else {
                let from = (target - done_ms).saturating_sub(CUT_SLACK_MS).max(1_000);
                let window = from as usize * per_ms..samples.len();
                quietest_cut(&samples, SAMPLE_RATE, window) / per_ms * per_ms
            };
            let words = whisper
                .transcribe(&Request {
                    samples: &samples[..cut],
                    language: language.as_deref(),
                    abort: None::<Arc<_>>,
                })
                .expect("transcribe");
            let end_ms = done_ms + (cut / per_ms) as u64;
            for w in &words {
                let (start, end) = (done_ms + w.start_ms, done_ms + w.end_ms);
                writeln!(out, "{start}\t{end}\t{:.3}\t{}", w.probability, w.text).unwrap();
                heard.push(HeardWord {
                    text: w.text.clone(),
                    start_ms: start,
                    end_ms: end,
                });
            }
            writeln!(out, "block\t{done_ms}\t{end_ms}").unwrap();
            out.flush().unwrap();
            done_ms = end_ms;
            let elapsed = started.elapsed().as_secs_f64();
            println!(
                "  {} / {}: {} words, {:.1}× real time",
                clock(done_ms),
                clock(limit_ms),
                words.len(),
                (done_ms - first_ms) as f64 / 1000.0 / elapsed
            );
        }
    }

    // ---- matching
    let texts: Vec<&str> = book.chapters.iter().map(|c| c.text.as_str()).collect();
    let started = Instant::now();
    let map = match_book(&texts, &heard);
    println!("matched in {:.2?}", started.elapsed());
    let r = &map.report;
    let pct = |a: usize, b: usize| 100.0 * a as f64 / b.max(1) as f64;
    println!(
        "text: {:.1}% matched, {:.1}% timed of {} words; audio: {:.1}% of {} heard words placed",
        pct(r.matched_words, r.words),
        pct(r.timed_words, r.words),
        r.words,
        pct(r.matched_heard_words, r.heard_words),
        r.heard_words
    );
    for (i, (chapter, c)) in book.chapters.iter().zip(&r.chapters).enumerate() {
        let title: String = chapter.title.chars().take(36).collect();
        let span = c.span.map_or("not in recording".into(), |(s, e)| {
            format!("{} – {}", clock(s), clock(e))
        });
        println!(
            "  {i:>3} {title:<36} {:>6} words {:>5.1}% matched {:>5.1}% timed  {span}",
            c.words,
            pct(c.matched_words, c.words),
            pct(c.timed_words, c.words)
        );
    }
    if std::env::var_os("AOIDE_DEBUG").is_some() {
        for (c, chapter) in map.chapters.iter().enumerate() {
            let n = chapter.words.len();
            if n == 0 || r.chapters[c].timed_words == 0 {
                continue;
            }
            let row: String = (0..40)
                .map(|k| {
                    let part = &chapter.words[k * n / 40..((k + 1) * n / 40).max(k * n / 40 + 1)];
                    let heard = part
                        .iter()
                        .filter(|w| matches!(w.timing, Timing::Heard { .. }))
                        .count();
                    let timed = part.iter().filter(|w| w.timing.span().is_some()).count();
                    if heard * 2 > part.len() {
                        '#'
                    } else if timed * 2 > part.len() {
                        'e'
                    } else {
                        '.'
                    }
                })
                .collect();
            let last = chapter
                .words
                .iter()
                .filter_map(|w| w.timing.span())
                .map(|s| s.1)
                .max();
            println!("  {c:>3} [{row}] last {:?}", last.map(clock));
            let text: Vec<char> = book.chapters[c].text.chars().collect();
            let mut shown = 0;
            for (k, w) in chapter.words.iter().enumerate().skip(n / 2) {
                if let Timing::Heard { start_ms, .. } = w.timing {
                    if shown < 12 {
                        let word: String = text[w.range.clone()].iter().collect();
                        println!(
                            "      heard late: token {k}/{n} {word:?} at {}",
                            clock(start_ms)
                        );
                    }
                    shown += 1;
                }
            }
            println!("      {shown} heard words in the second half");
        }
    }
    for gap in &r.unmatched_audio {
        let excerpt: String = gap.excerpt.chars().take(60).collect();
        println!(
            "  not in text: {} ({} s) “{excerpt}…”",
            clock(gap.start_ms),
            (gap.end_ms - gap.start_ms) / 1000
        );
    }

    // ---- sentence starts against where speech resumes after a pause
    let options = aoide_text::SegmentOptions::default();
    let mut starts: Vec<(usize, u64, String)> = Vec::new();
    for (c, chapter) in book.chapters.iter().enumerate() {
        let words = &map.chapters[c].words;
        for segment in aoide_text::segment_chapter(c as u32, &chapter.blocks, &options) {
            let at = words.partition_point(|w| w.range.start < segment.range.start);
            if let Some(w) = words.get(at).filter(|w| w.range.start < segment.range.end) {
                if let Timing::Heard { start_ms, .. } = w.timing {
                    let first: String = segment.display.chars().take(50).collect();
                    starts.push((c, start_ms, first));
                }
            }
        }
    }
    let step = (starts.len() / 400).max(1);
    let mut offsets: Vec<i64> = Vec::new();
    for (_, start, _) in starts.iter().step_by(step) {
        if *start < 1_000 || start + 1_000 > done_ms.min(total_ms) {
            continue;
        }
        let samples = reader
            .read(start - 1_000..start + 1_000, SAMPLE_RATE)
            .unwrap();
        if let Some(onset) = nearest_onset(&samples) {
            offsets.push(1_000 - onset as i64);
        }
    }
    if !offsets.is_empty() {
        let mut absolute: Vec<i64> = offsets.iter().map(|o| o.abs()).collect();
        absolute.sort_unstable();
        let mut signed = offsets.clone();
        signed.sort_unstable();
        let within = absolute.iter().filter(|o| **o <= 150).count();
        println!(
            "sentence starts vs. speech onsets ({} checked): median error {} ms, median bias {:+} ms, {:.0}% within 150 ms",
            offsets.len(),
            absolute[absolute.len() / 2],
            signed[signed.len() / 2],
            pct(within, absolute.len())
        );
    }

    if let Some(dir) = &args.clips {
        std::fs::create_dir_all(dir).unwrap();
        let mut list = String::new();
        let step = (starts.len() / 8).max(1);
        for (n, (c, start, first)) in starts.iter().step_by(step).take(8).enumerate() {
            let end = (start + 4_000).min(total_ms);
            let samples = reader.read(*start..end, 16_000).unwrap();
            let name = format!("{n:02}-chapter{c}.wav");
            write_wav(&dir.join(&name), &samples, 16_000);
            list.push_str(&format!("{name}\t{}\t{first}\n", clock(*start)));
        }
        std::fs::write(dir.join("clips.txt"), list).unwrap();
        println!("clips written to {}", dir.display());
    }
}

/// Milliseconds into the window (around its middle) where speech resumes after a pause of at
/// least 150 ms, nearest the middle.
fn nearest_onset(samples: &[f32]) -> Option<usize> {
    let frame = 160; // 10 ms at 16 kHz
    let energy: Vec<f32> = samples
        .chunks(frame)
        .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32).sqrt())
        .collect();
    let peak = energy.iter().copied().fold(0.0, f32::max);
    if peak <= 0.0 {
        return None;
    }
    let quiet: Vec<bool> = energy.iter().map(|e| *e < peak * 0.1).collect();
    let mut onsets = Vec::new();
    let mut run = 0;
    for (i, q) in quiet.iter().enumerate() {
        if *q {
            run += 1;
        } else {
            if run >= 15 {
                onsets.push(i * 10);
            }
            run = 0;
        }
    }
    onsets.into_iter().min_by_key(|o| (*o as i64 - 1_000).abs())
}

fn write_wav(path: &std::path::Path, samples: &[f32], rate: u32) {
    let mut bytes = Vec::new();
    let data = (samples.len() * 2) as u32;
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        bytes.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, bytes).unwrap();
}
