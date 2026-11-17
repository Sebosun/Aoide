//! Shortening long silences in a stream of audio, and mapping times through the cuts.

use aoide_audio::trim::{to_original, to_trimmed, Cut, TrimOptions, Trimmer};

const RATE: u32 = 1000; // 1 sample per millisecond: a 10 ms frame is 10 samples.

fn tone(ms: usize) -> Vec<f32> {
    (0..ms)
        .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
        .collect()
}

fn silence(ms: usize) -> Vec<f32> {
    vec![0.0; ms]
}

fn options() -> TrimOptions {
    TrimOptions {
        keep_ms: 500,
        ..TrimOptions::default()
    }
}

/// `audio` through a trimmer, pushed in pieces of `piece` samples.
fn trimmed(audio: &[f32], piece: usize, keep_from_ms: u64) -> (Vec<f32>, Vec<Cut>) {
    let mut trimmer = Trimmer::new(RATE, options(), keep_from_ms, 0.0);
    let mut out = Vec::new();
    for chunk in audio.chunks(piece) {
        out.extend(trimmer.push(chunk));
    }
    out.extend(trimmer.finish());
    (out, trimmer.cuts().to_vec())
}

fn speech_with_gap(gap: usize) -> Vec<f32> {
    [tone(1000), silence(gap), tone(1000)].concat()
}

#[test]
fn a_long_silence_is_shortened_and_speech_is_left_as_it_was() {
    let audio = speech_with_gap(2000);
    let (out, cuts) = trimmed(&audio, 5000, 0);
    assert_eq!(out.len(), 2500, "a 2 s gap keeps 0.5 s");
    assert_eq!(&out[..1000], &tone(1000)[..]);
    assert_eq!(&out[1500..], &tone(1000)[..]);
    assert_eq!(cuts.len(), 1);
    assert_eq!(cuts[0].removed_ms, 1500);
    // Cut in the middle of the silence.
    assert!((1200..1300).contains(&cuts[0].at_ms), "{:?}", cuts[0]);
}

#[test]
fn the_same_comes_out_whatever_the_pieces() {
    let audio = speech_with_gap(2000);
    let whole = trimmed(&audio, 5000, 0);
    for piece in [1, 7, 333, 1000, 1999] {
        assert_eq!(trimmed(&audio, piece, 0), whole, "pieces of {piece}");
    }
}

#[test]
fn short_pauses_stay() {
    let audio = speech_with_gap(400);
    let (out, cuts) = trimmed(&audio, 250, 0);
    assert_eq!(out, audio);
    assert!(cuts.is_empty());
}

#[test]
fn nothing_before_the_place_sought_is_cut() {
    let audio = speech_with_gap(2000);
    // Sought into the middle of the silence: before it, all is as it was.
    let (out, cuts) = trimmed(&audio, 300, 2000);
    assert_eq!(&out[..2000], &audio[..2000]);
    assert_eq!(out.len(), 2000 + 500 + 1000);
    assert!(cuts.iter().all(|c| c.at_ms >= 2000));
    // Sought to near its end: what is left of it is short, and stays.
    let (out, cuts) = trimmed(&audio, 300, 2800);
    assert_eq!(out, audio);
    assert!(cuts.is_empty());
}

#[test]
fn quiet_speech_is_not_silence_once_louder_speech_was_heard() {
    // Breath and room noise well under the speech, but over the floor.
    let noise: Vec<f32> = (0..2000)
        .map(|i| if i % 2 == 0 { 0.004 } else { -0.004 })
        .collect();
    let audio = [tone(1000), noise, tone(1000)].concat();
    let (out, _) = trimmed(&audio, 500, 0);
    assert_eq!(out.len(), 2500, "noise far under the speech is silence");
    // Speech at a tenth of the loudest is not.
    let soft: Vec<f32> = tone(2000).iter().map(|s| s / 10.0).collect();
    let audio = [tone(1000), soft, tone(1000)].concat();
    let (out, _) = trimmed(&audio, 500, 0);
    assert_eq!(out.len(), 4000);
}

#[test]
fn times_map_between_the_recording_and_what_is_played() {
    let cuts = [
        Cut {
            at_ms: 1250,
            removed_ms: 1500,
        },
        Cut {
            at_ms: 4000,
            removed_ms: 1000,
        },
    ];
    // Before a cut, the same; after, earlier by what was removed.
    assert_eq!(to_trimmed(&cuts, 1000), 1000);
    assert_eq!(to_trimmed(&cuts, 3000), 1500);
    assert_eq!(to_trimmed(&cuts, 6000), 3500);
    // A time inside what was removed plays where the cut is.
    assert_eq!(to_trimmed(&cuts, 2000), 1250);
    for original in [0, 1000, 1249, 2750, 3000, 3999, 5000, 9000] {
        assert_eq!(
            to_original(&cuts, to_trimmed(&cuts, original)),
            original,
            "{original}"
        );
    }
}
