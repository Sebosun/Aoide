//! Which units the recording lacks: what the speech engine may fill in when asked.
#![allow(clippy::single_range_in_vec_init)] // Off-text stretches are lists of ranges.
use std::ops::Range;

use aoide_sync::{lacking, Clip};

/// A unit spoken from `text` within `audio`, with narration that matched no text in `off_text`.
fn timed(audio: Range<u64>, text: Range<u64>, off_text: Vec<Range<u64>>) -> Clip {
    Clip {
        audio,
        text: Some(text),
        words: vec![],
        off_text,
    }
}

/// A unit with no timed word, at `at` in the recording.
fn untimed(at: u64) -> Clip {
    Clip {
        audio: at..at,
        text: None,
        words: vec![],
        off_text: vec![],
    }
}

#[test]
fn a_run_the_recording_moves_straight_past_is_lacking() {
    let clips = [
        timed(0..4_000, 0..3_800, vec![]),
        untimed(4_000),
        untimed(4_000),
        timed(4_000..8_000, 4_300..7_900, vec![]),
    ];
    assert_eq!(
        lacking(&clips, &[3_000, 2_000, 2_000, 3_000]),
        [false, true, true, false]
    );
}

#[test]
fn a_run_whose_narration_was_heard_but_not_matched_is_not_lacking() {
    // The narrator said the run: the next unit's lead-in holds 3.5 s that matched no text.
    let clips = [
        timed(0..4_000, 0..3_800, vec![]),
        untimed(4_000),
        timed(4_000..11_000, 7_600..10_900, vec![4_100..7_600]),
    ];
    assert_eq!(
        lacking(&clips, &[3_000, 4_000, 3_000]),
        [false, false, false]
    );
    // A short stretch (a cough, a title card) does not account for a long run.
    let clips = [
        timed(0..4_000, 0..3_800, vec![]),
        untimed(4_000),
        timed(4_000..9_000, 5_000..8_900, vec![4_100..4_900]),
    ];
    assert_eq!(
        lacking(&clips, &[3_000, 4_000, 3_000]),
        [false, true, false]
    );
}

#[test]
fn runs_at_either_end_are_judged_by_the_narration_beside_them() {
    let clips = [
        untimed(0),
        timed(0..5_000, 200..4_900, vec![]),
        untimed(5_000),
    ];
    assert_eq!(lacking(&clips, &[2_000, 3_000, 2_000]), [true, false, true]);
    // Closing narration that matched nothing may be the last unit, read aloud.
    let clips = [
        timed(0..9_000, 0..4_000, vec![4_200..8_800]),
        untimed(9_000),
    ];
    assert_eq!(lacking(&clips, &[3_000, 4_000]), [false, false]);
}

#[test]
fn nothing_is_lacking_in_a_recording_with_no_timed_unit() {
    // Not matched at all: it says nothing about which text it holds.
    assert_eq!(
        lacking(&[untimed(0), untimed(0)], &[1_000, 1_000]),
        [false, false]
    );
}
