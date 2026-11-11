//! Word timings refined against the generated audio's pauses.
use aoide_text::{
    align_to_pauses, segment_chapter, Block, BlockKind, SegmentOptions, WordEstimate,
};

const RATE: u32 = 24_000;

fn segment(text: &str) -> aoide_text::Segment {
    let blocks = [Block {
        kind: BlockKind::Paragraph,
        text: text.into(),
    }];
    segment_chapter(
        0,
        &blocks,
        &SegmentOptions {
            max_chars: 1000,
            ..Default::default()
        },
    )
    .remove(0)
}

/// Alternating silence and tone, each part given in milliseconds; tones start with sound.
fn audio(parts: &[(bool, u32)]) -> Vec<f32> {
    let mut samples = Vec::new();
    for &(sound, ms) in parts {
        let n = (RATE * ms / 1000) as usize;
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            samples.push(if sound {
                0.4 * (t * 2.0 * std::f32::consts::PI * 220.0).sin()
            } else {
                0.0
            });
        }
    }
    samples
}

fn near(actual: u32, expected: u32) {
    assert!(
        actual.abs_diff(expected) <= 20,
        "expected ~{expected} ms, got {actual} ms"
    );
}

#[test]
fn pauses_anchor_the_boundaries_after_punctuation() {
    let seg = segment("Hello there world, again and here.");
    let samples = audio(&[
        (false, 100),
        (true, 600),
        (false, 300),
        (true, 700),
        (false, 150),
    ]);
    let words = align_to_pauses(&seg, &samples, RATE);
    assert_eq!(words.len(), 6);
    near(words[0].start_ms, 100);
    near(words[2].end_ms, 700);
    near(words[3].start_ms, 1000);
    near(words[5].end_ms, 1700);
    for pair in words.windows(2) {
        assert!(pair[0].end_ms <= pair[1].start_ms);
        assert!(pair[0].start_ms < pair[0].end_ms);
    }
}

#[test]
fn without_pauses_words_span_only_the_voiced_audio() {
    let seg = segment("One two three four.");
    let samples = audio(&[(false, 250), (true, 1000), (false, 400)]);
    let words = align_to_pauses(&seg, &samples, RATE);
    near(words[0].start_ms, 250);
    near(words[3].end_ms, 1250);
}

#[test]
fn a_pause_without_punctuation_goes_to_the_nearest_boundary() {
    let seg = segment("aaaa bbbb cccc dddd");
    let samples = audio(&[(true, 500), (false, 200), (true, 500)]);
    let words = align_to_pauses(&seg, &samples, RATE);
    near(words[1].end_ms, 500);
    near(words[2].start_ms, 700);
}

#[test]
fn silent_or_empty_audio_falls_back_to_the_estimate() {
    let seg = segment("Quiet words here.");
    let silent = audio(&[(false, 900)]);
    let words = align_to_pauses(&seg, &silent, RATE);
    assert_eq!(words, aoide_text::estimate_word_timings(&seg, 900));
    assert_eq!(align_to_pauses(&seg, &[], RATE).len(), 3);
    let _: Vec<WordEstimate> = words;
}

fn w(start_ms: u32, end_ms: u32) -> WordEstimate {
    WordEstimate { start_ms, end_ms }
}

#[test]
fn aligned_spans_win_and_gaps_are_filled_in_order() {
    let fallback = [w(0, 300), w(300, 600), w(600, 900), w(900, 1200)];
    let aligned = [Some((120, 340)), None, Some((700, 820)), None];
    let merged = aoide_text::merge_alignment(&aligned, &fallback);
    assert_eq!(merged[0], w(120, 340));
    assert_eq!(merged[2], w(700, 820));
    // Unplaced words keep their estimate, clamped between their aligned neighbours.
    assert_eq!(merged[1], w(340, 600));
    assert_eq!(merged[3], w(900, 1200));
    for pair in merged.windows(2) {
        assert!(pair[0].start_ms <= pair[1].start_ms);
    }
}

#[test]
fn overlapping_alignments_are_made_monotonic() {
    let fallback = [w(0, 100), w(100, 200)];
    let merged = aoide_text::merge_alignment(&[Some((50, 150)), Some((120, 180))], &fallback);
    assert!(merged[1].start_ms >= merged[0].end_ms || merged[1].start_ms >= merged[0].start_ms);
    assert!(merged[0].start_ms <= merged[1].start_ms);
    assert_eq!(
        aoide_text::merge_alignment(&[None], &[w(5, 9)]),
        vec![w(5, 9)]
    );
}
