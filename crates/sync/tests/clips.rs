mod support;

use aoide_sync::{clips, match_book, Clip, Loudness, Unit};
use support::{chapters, read, read_all, strs};

/// One unit per sentence-ish piece: splits each chapter's text after ". ".
fn units(chapters: &[String]) -> Vec<Unit> {
    let mut out = Vec::new();
    for (c, text) in chapters.iter().enumerate() {
        let chars: Vec<char> = text.chars().collect();
        let mut start = 0;
        for i in 0..=chars.len() {
            let end_here = i == chars.len()
                || (i > 0 && chars[i - 1] == '.' && chars[i].is_whitespace())
                || chars[i] == '\n';
            if i > start && end_here {
                let range = start..i;
                let mut words = Vec::new();
                let mut w = None;
                for k in range.clone() {
                    match (chars[k].is_whitespace(), w) {
                        (false, None) => w = Some(k),
                        (true, Some(s)) => {
                            words.push(s..k);
                            w = None;
                        }
                        _ => {}
                    }
                }
                if let Some(s) = w {
                    words.push(s..i);
                }
                if !words.is_empty() {
                    out.push(Unit {
                        chapter: c,
                        range,
                        words,
                    });
                }
                start = i + 1;
            }
        }
    }
    out
}

fn timed(clips: &[Clip]) -> Vec<&Clip> {
    let mut timed: Vec<&Clip> = clips.iter().filter(|c| c.text.is_some()).collect();
    timed.sort_by_key(|c| c.audio.start);
    timed
}

#[test]
fn clips_cover_the_recording_end_to_end_without_gaps_or_overlaps() {
    let chapters = chapters();
    let heard = read_all(&strs(&chapters), 0);
    let total = heard.last().unwrap().end_ms + 3_000;
    let map = match_book(&strs(&chapters), &heard);
    let units = units(&chapters);
    let clips = clips(&map, &units, total, None);

    assert_eq!(clips.len(), units.len());
    let timed = timed(&clips);
    assert_eq!(timed.len(), units.len(), "every unit is narrated");
    assert_eq!(timed[0].audio.start, 0);
    assert_eq!(timed.last().unwrap().audio.end, total);
    for pair in timed.windows(2) {
        assert_eq!(pair[0].audio.end, pair[1].audio.start);
    }
    for clip in &clips {
        let text = clip.text.clone().unwrap();
        assert!(clip.audio.start <= text.start && text.end <= clip.audio.end);
    }
}

#[test]
fn cuts_fall_in_the_pause_between_sentences_at_its_quietest() {
    let chapters = chapters();
    let heard = read_all(&strs(&chapters), 0);
    let total = heard.last().unwrap().end_ms;
    let map = match_book(&strs(&chapters), &heard);
    let units = units(&chapters);

    let plain = clips(&map, &units, total, None);
    let first = &plain[1];
    let before = plain[0].text.clone().unwrap();
    assert!(
        before.end <= first.audio.start && first.audio.start <= first.text.clone().unwrap().start
    );

    // A loudness envelope with one quiet frame inside that pause moves the cut onto it.
    let pause = before.end..first.text.clone().unwrap().start;
    let quiet_at = pause.start + (pause.end - pause.start) / 4;
    let mut levels = vec![200u8; (total / 10 + 1) as usize];
    levels[(quiet_at / 10) as usize] = 0;
    let loudness = Loudness {
        frame_ms: 10,
        levels,
    };
    let shaped = clips(&map, &units, total, Some(&loudness));
    assert_eq!(shaped[1].audio.start, quiet_at / 10 * 10 + 5);
}

#[test]
fn narration_missing_from_the_text_leads_into_the_next_sentence() {
    let chapters = chapters();
    let text = strs(&chapters);
    let dedication = "For my sister Agathe, who always kept a light burning in the window for me.";
    let heard = read_all(&[text[0], dedication, text[1], text[2], text[3]], 0);
    let total = heard.last().unwrap().end_ms;
    let map = match_book(&text, &heard);
    let units = units(&chapters);
    let clips = clips(&map, &units, total, None);

    let said = heard.iter().find(|w| w.text == "For").unwrap().start_ms;
    let second = units.iter().position(|u| u.chapter == 1).unwrap();
    let lead = &clips[second];
    assert!(
        lead.audio.start <= said,
        "the dedication plays with chapter two's first clip"
    );
    assert_eq!(lead.off_text.len(), 1);
    assert_eq!(lead.off_text[0].start, said);
    assert!(lead.off_text[0].end <= lead.text.clone().unwrap().start);
    assert!(clips[second - 1].off_text.is_empty());
}

#[test]
fn credits_lead_into_the_first_sentence_and_the_rest_is_a_tail() {
    let chapters = chapters();
    let credits = read(
        "This is Gull Point, read for you by a narrator in a small studio by the sea.",
        0,
    );
    let from = credits.last().unwrap().end_ms + 1_000;
    let mut heard = credits;
    heard.extend(read_all(&strs(&chapters), from));
    let total = heard.last().unwrap().end_ms + 20_000;
    let map = match_book(&strs(&chapters), &heard);
    let clips = clips(&map, &units(&chapters), total, None);

    assert_eq!(clips[0].audio.start, 0);
    assert_eq!(clips[0].off_text.len(), 1);
    assert_eq!(clips[0].off_text[0].start, 0);
    assert_eq!(clips.last().unwrap().audio.end, total);
}

#[test]
fn units_not_in_the_recording_get_empty_clips() {
    let chapters = chapters();
    let text = strs(&chapters);
    let heard = read_all(&[text[0], text[1], text[3]], 0);
    let total = heard.last().unwrap().end_ms;
    let map = match_book(&text, &heard);
    let units = units(&chapters);
    let clips = clips(&map, &units, total, None);

    for (unit, clip) in units.iter().zip(&clips) {
        if unit.chapter == 2 {
            assert!(clip.text.is_none());
            assert!(clip.audio.is_empty());
            assert!(clip.words.iter().all(Option::is_none));
        } else {
            assert!(clip.text.is_some());
        }
    }
    let timed = timed(&clips);
    for pair in timed.windows(2) {
        assert_eq!(pair[0].audio.end, pair[1].audio.start);
    }
}

#[test]
fn word_times_are_relative_to_the_clip_and_rise() {
    let chapters = chapters();
    let heard = read_all(&strs(&chapters), 0);
    let total = heard.last().unwrap().end_ms;
    let map = match_book(&strs(&chapters), &heard);
    let units = units(&chapters);
    let clips = clips(&map, &units, total, None);

    for (unit, clip) in units.iter().zip(&clips) {
        assert_eq!(clip.words.len(), unit.words.len());
        let spans: Vec<_> = clip.words.iter().map(|w| w.clone().unwrap()).collect();
        assert!(spans.windows(2).all(|w| w[0].start <= w[1].start));
        let length = clip.audio.end - clip.audio.start;
        assert!(spans.iter().all(|s| s.end <= length));
        let text = clip.text.clone().unwrap();
        assert_eq!(spans[0].start, text.start - clip.audio.start);
    }
}
