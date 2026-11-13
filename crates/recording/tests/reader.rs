mod support;

use aoide_recording::{inspect, Reader};
use support::{fixture, ramp, ramp_value};

fn zero_crossings(samples: &[f32]) -> usize {
    samples
        .windows(2)
        .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
        .count()
}

#[test]
fn a_read_at_the_files_own_rate_returns_its_samples_across_files() {
    let dir = tempfile::tempdir().unwrap();
    ramp(&dir.path().join("1.wav"), 8_000, 4_000, 0);
    ramp(&dir.path().join("2.wav"), 8_000, 2_400, 4_000);
    let found = inspect(&[dir.path().to_owned()]);
    let mut reader = Reader::new(found.files);
    assert_eq!(reader.total_ms(), 800);

    let samples = reader.read(450..550, 8_000).unwrap();
    assert_eq!(samples.len(), 800);
    for (k, s) in samples.iter().enumerate() {
        assert_eq!(*s, ramp_value(3_600 + k), "sample {k}");
    }
}

#[test]
fn reading_in_pieces_or_jumping_back_gives_the_same_samples() {
    let found = inspect(&[fixture("set"), fixture("book.m4b")]);
    let mut reader = Reader::new(found.files.clone());
    let whole = reader.read(0..3_400, 16_000).unwrap();
    assert_eq!(whole.len(), 3_400 * 16);

    let mut pieces = Reader::new(found.files.clone());
    let mut joined = Vec::new();
    for range in [0..123, 123..999, 999..1_700, 1_700..3_400] {
        joined.extend(pieces.read(range, 16_000).unwrap());
    }
    assert_eq!(joined, whole);

    // Back to the middle of the second file, then into the M4B.
    for range in [1_200..1_300, 2_000..2_100, 100..200] {
        let again = pieces.read(range.clone(), 16_000).unwrap();
        let from = range.start as usize * 16;
        let expected = &whole[from..from + again.len()];
        let worst = again
            .iter()
            .zip(expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(worst < 1e-3, "{range:?} differs by {worst}");
    }
}

#[test]
fn resampling_keeps_the_pitch() {
    let dir = tempfile::tempdir().unwrap();
    support::sine(&dir.path().join("tone.wav"), 44_100, 1_000.0, 1.0);
    let found = inspect(&[dir.path().to_owned()]);
    let mut reader = Reader::new(found.files);
    for rate in [16_000, 48_000] {
        let samples = reader.read(0..1_000, rate).unwrap();
        let crossings = zero_crossings(&samples[100..samples.len() - 100]);
        let expected = 2_000.0 * (samples.len() - 200) as f32 / rate as f32;
        assert!(
            (crossings as f32 - expected).abs() <= 2.0,
            "{rate}: {crossings}"
        );
        let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((peak - 0.5).abs() < 0.02, "{rate}: peak {peak}");
    }
}

#[test]
fn stereo_is_mixed_down_to_mono() {
    let dir = tempfile::tempdir().unwrap();
    let frames: Vec<Vec<f32>> = (0..800).map(|_| vec![0.4, 0.2]).collect();
    support::wav(&dir.path().join("stereo.wav"), 8_000, 2, &frames);
    let found = inspect(&[dir.path().to_owned()]);
    let mut reader = Reader::new(found.files);
    let samples = reader.read(0..100, 8_000).unwrap();
    assert!(samples.iter().all(|s| (s - 0.3).abs() < 1e-3));
}

#[test]
fn reads_past_the_end_are_refused() {
    let found = inspect(&[fixture("tone.flac")]);
    let mut reader = Reader::new(found.files);
    assert!(reader.read(700..800, 16_000).is_err());
    assert_eq!(reader.read(700..750, 16_000).unwrap().len(), 800);
}

/// An M4B's chapter titles are a track of their own that ends where its last chapter starts;
/// symphonia refuses to seek past it, so the reader must reach the rest by decoding forward.
#[test]
fn a_jump_past_where_seeking_stops_working_still_reads_the_audio() {
    let found = inspect(&[fixture("short-chapters.m4b")]);
    let files = found.files.clone();
    let (sent, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = Reader::new(files);
        reader.read(0..100, 8_000).unwrap();
        let near_the_end = reader.read(27_000..28_000, 8_000).unwrap();
        let back = reader.read(26_000..26_500, 8_000).unwrap();
        sent.send((near_the_end, back)).unwrap();
    });
    let (near_the_end, back) = received
        .recv_timeout(std::time::Duration::from_secs(20))
        .expect("the reader got stuck");

    let whole = Reader::new(found.files).read(0..28_000, 8_000).unwrap();
    assert_eq!(near_the_end, whole[27_000 * 8..]);
    assert_eq!(back, whole[26_000 * 8..26_500 * 8]);
}
