mod support;

use aoide_sync::{match_book, tokenize, HeardWord, SyncMap, Timing};
use support::{chapters, read, read_all, strs, word};

fn heard_count(map: &SyncMap, chapter: usize) -> usize {
    map.chapters[chapter]
        .words
        .iter()
        .filter(|w| matches!(w.timing, Timing::Heard { .. }))
        .count()
}

fn starts(map: &SyncMap, chapter: usize) -> Vec<u64> {
    map.chapters[chapter]
        .words
        .iter()
        .filter_map(|w| w.timing.span().map(|(start, _)| start))
        .collect()
}

fn assert_never_go_back(map: &SyncMap) {
    for chapter in 0..map.chapters.len() {
        let starts = starts(map, chapter);
        assert!(
            starts.windows(2).all(|w| w[0] <= w[1]),
            "chapter {chapter} goes back in time: {starts:?}"
        );
    }
}

/// Character ranges of the whitespace-separated words of `text`.
fn spoken_words(text: &str) -> Vec<std::ops::Range<usize>> {
    let chars: Vec<char> = text.chars().collect();
    let mut words = Vec::new();
    let mut start = None;
    for (i, c) in chars.iter().enumerate() {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                words.push(s..i);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        words.push(s..chars.len());
    }
    words
}

#[test]
fn an_exact_transcript_times_every_word_with_what_was_heard() {
    let chapters = chapters();
    let heard = read_all(&strs(&chapters), 0);
    let map = match_book(&strs(&chapters), &heard);

    let mut offset = 0;
    for (c, text) in chapters.iter().enumerate() {
        let words = spoken_words(text);
        let tokens = tokenize(text);
        assert_eq!(map.chapters[c].words.len(), tokens.len());
        for (token, timed) in tokens.iter().zip(&map.chapters[c].words) {
            assert_eq!(timed.range, token.range);
            let w = words
                .iter()
                .position(|r| r.contains(&token.range.start))
                .unwrap();
            let said = &heard[offset + w];
            match timed.timing {
                Timing::Heard { start_ms, end_ms } => assert!(
                    said.start_ms <= start_ms && end_ms <= said.end_ms,
                    "{:?} timed {start_ms}..{end_ms}, said {}..{}",
                    token.norm,
                    said.start_ms,
                    said.end_ms
                ),
                other => panic!("{:?} is {other:?}", token.norm),
            }
        }
        offset += words.len();
    }
    let report = &map.report;
    assert_eq!(report.matched_words, report.words);
    assert_eq!(report.matched_heard_words, report.heard_words);
    assert!(report.unmatched_audio.is_empty());
}

#[test]
fn dropped_words_are_estimated_between_their_neighbours() {
    let chapters = chapters();
    let mut heard = read_all(&strs(&chapters), 0);
    let dropped: Vec<HeardWord> = heard.iter().skip(10).step_by(20).cloned().collect();
    heard.retain(|w| !dropped.contains(w));
    let map = match_book(&strs(&chapters), &heard);

    let report = &map.report;
    assert_eq!(
        report.timed_words, report.words,
        "every word still has a time"
    );
    assert!(report.matched_words < report.words);
    let estimated = map
        .chapters
        .iter()
        .flat_map(|c| &c.words)
        .filter(|w| matches!(w.timing, Timing::Estimated { .. }))
        .count();
    assert!(estimated >= dropped.len(), "{estimated} estimated");
    assert_never_go_back(&map);
}

#[test]
fn misheard_words_take_the_time_of_what_was_heard_in_their_place() {
    let chapters = chapters();
    let mut heard = read_all(&strs(&chapters), 0);
    // Interior words only: at the very end of the recording, unmatched heard words are free
    // (closing credits), so a misheard last word is placed next to its neighbour instead.
    let interior = heard.len() - 5;
    let mut garbled = Vec::new();
    for (i, w) in heard
        .iter_mut()
        .enumerate()
        .take(interior)
        .skip(7)
        .step_by(15)
    {
        if w.text
            .chars()
            .all(|c| c.is_alphabetic() || ",.".contains(c))
        {
            w.text = "zzq".into();
            garbled.push((i, w.start_ms, w.end_ms));
        }
    }
    assert!(garbled.len() > 10);
    let map = match_book(&strs(&chapters), &heard);

    let in_place = map
        .chapters
        .iter()
        .flat_map(|c| &c.words)
        .filter(|w| match w.timing {
            Timing::Estimated { start_ms, end_ms } => garbled
                .iter()
                .any(|(_, start, end)| *start == start_ms && *end == end_ms),
            _ => false,
        })
        .count();
    assert_eq!(in_place, garbled.len());
    assert_eq!(map.report.timed_words, map.report.words);
}

#[test]
fn credits_before_the_text_are_unmatched_audio() {
    let chapters = chapters();
    let credits = read(
        "This is Gull Point, written by an author and read for you by a narrator. Recorded in a small studio by the sea.",
        0,
    );
    let from = credits.last().unwrap().end_ms + 2_000;
    let mut heard = credits;
    heard.extend(read_all(&strs(&chapters), from));
    let map = match_book(&strs(&chapters), &heard);

    assert_eq!(map.report.matched_words, map.report.words);
    let gap = &map.report.unmatched_audio;
    assert_eq!(gap.len(), 1, "{gap:?}");
    assert_eq!(gap[0].start_ms, 0);
    assert!(gap[0].end_ms <= from);
    assert!(
        gap[0].excerpt.starts_with("This is Gull Point"),
        "{}",
        gap[0].excerpt
    );
}

#[test]
fn a_chapter_the_narrator_skipped_is_unread() {
    let chapters = chapters();
    let text = strs(&chapters);
    let heard = read_all(&[text[0], text[1], text[3]], 0);
    let map = match_book(&text, &heard);

    assert!(map.chapters[2]
        .words
        .iter()
        .all(|w| w.timing == Timing::Unread));
    assert_eq!(map.report.chapters[2].timed_words, 0);
    for c in [0, 1, 3] {
        assert_eq!(
            heard_count(&map, c),
            map.chapters[c].words.len(),
            "chapter {c}"
        );
    }
    assert!(map.report.unmatched_audio.is_empty());
}

#[test]
fn chapters_recorded_in_another_order_are_still_found() {
    let chapters = chapters();
    let text = strs(&chapters);
    let heard = read_all(&[text[0], text[2], text[1], text[3]], 0);
    let map = match_book(&text, &heard);

    for c in 0..4 {
        assert_eq!(
            heard_count(&map, c),
            map.chapters[c].words.len(),
            "chapter {c}"
        );
    }
    assert!(starts(&map, 2)[0] < starts(&map, 1)[0]);
    assert!(starts(&map, 1)[0] < starts(&map, 3)[0]);
    assert_never_go_back(&map);
}

#[test]
fn a_passage_read_twice_is_matched_once_and_times_never_go_back() {
    let chapters = chapters();
    let retake = "Odile read it twice at the kitchen table, then folded it into quarters and tucked it behind the clock.";
    let second = chapters[1].replacen(retake, &format!("{retake} {retake}"), 1);
    let heard = read_all(&[&chapters[0], &second, &chapters[2], &chapters[3]], 0);
    let map = match_book(&strs(&chapters), &heard);

    assert_eq!(map.report.timed_words, map.report.words);
    assert!(map.report.matched_words * 100 / map.report.words >= 95);
    assert_never_go_back(&map);
}

#[test]
fn a_number_read_as_words_gets_a_time_between_its_neighbours() {
    let chapters = chapters();
    let spoken = chapters[0].replacen("1865,", "eighteen sixty-five,", 1);
    let text = strs(&chapters);
    let heard = read_all(&[&spoken, text[1], text[2], text[3]], 0);
    let map = match_book(&text, &heard);

    let tokens = tokenize(&chapters[0]);
    let at = tokens.iter().position(|t| t.norm == "1865").unwrap();
    let span = |i: usize| map.chapters[0].words[i].timing.span().unwrap();
    let (start, end) = span(at);
    assert!(span(at - 1).1 <= start && end <= span(at + 1).0);
    let eighteen = heard.iter().find(|w| w.text == "eighteen").unwrap();
    assert_eq!(start, eighteen.start_ms);
}

#[test]
fn narration_that_is_not_in_the_text_is_reported_with_what_was_said() {
    let chapters = chapters();
    let text = strs(&chapters);
    let dedication = "For my sister Agathe, who always kept a light burning in the window for me.";
    let heard = read_all(&[text[0], dedication, text[1], text[2], text[3]], 0);
    let map = match_book(&text, &heard);

    assert_eq!(map.report.matched_words, map.report.words);
    let gaps = &map.report.unmatched_audio;
    assert_eq!(gaps.len(), 1, "{gaps:?}");
    assert!(
        gaps[0].excerpt.starts_with("For my sister Agathe"),
        "{}",
        gaps[0].excerpt
    );
    let first = heard.iter().find(|w| w.text == "For").unwrap();
    assert_eq!(gaps[0].start_ms, first.start_ms);
}

#[test]
fn nothing_heard_leaves_everything_unread() {
    let chapters = chapters();
    let map = match_book(&strs(&chapters), &[word("hello", 0)]);
    assert_eq!(map.report.timed_words, 0);
    assert_eq!(map.report.matched_heard_words, 0);
}

#[test]
fn a_recording_that_stops_mid_chapter_leaves_the_rest_unread() {
    let chapters = chapters();
    let text = strs(&chapters);
    // Stop right after a common word that also occurs near the chapter's end.
    let words: Vec<&str> = chapters[1].split_whitespace().collect();
    let cut = (0..words.len() / 2)
        .rev()
        .find(|&i| words[i] == "the")
        .unwrap()
        + 1;
    let half = words[..cut].join(" ");
    let heard = read_all(&[text[0], &half], 0);
    let map = match_book(&text, &heard);

    let last_heard = heard.last().unwrap().end_ms;
    let timed: Vec<_> = map.chapters[1]
        .words
        .iter()
        .map(|w| w.timing.span().is_some())
        .collect();
    let first_untimed = timed.iter().position(|t| !t).expect("the rest is unread");
    assert!(
        timed[first_untimed..].iter().all(|t| !t),
        "nothing timed after the recording ends"
    );
    assert!(
        first_untimed + 3 >= cut,
        "what was read is timed: {first_untimed} of {cut}"
    );
    assert!(map.chapters[1]
        .words
        .iter()
        .filter_map(|w| w.timing.span())
        .all(|(_, end)| end <= last_heard));
}

#[test]
fn a_passage_the_narrator_skipped_is_unread_not_squeezed_in() {
    let chapters = chapters();
    let skipped = " Her grandmother had polished the same brass, and her mother had cursed the same draught that whistled under the gallery door on every north-easterly night.";
    let spoken = chapters[0].replacen(skipped, "", 1);
    assert_ne!(spoken, chapters[0]);
    let text = strs(&chapters);
    let heard = read_all(&[&spoken, text[1], text[2], text[3]], 0);
    let map = match_book(&text, &heard);

    let unread = map.chapters[0]
        .words
        .iter()
        .filter(|w| w.timing == Timing::Unread)
        .count();
    assert_eq!(unread, aoide_sync::tokenize(skipped).len());
}

#[test]
fn the_last_words_heard_match_where_the_reading_stopped_not_a_later_repeat() {
    let chapter = "Mara carried the lantern down to the harbour wall every evening. She sang to the boats in the old mill race, where the water ran black and fast under the willows. Nobody in the town admitted to listening, though windows opened all along the lane whenever she began. The baker left a loaf on the step, the cooper mended her bucket without being asked, and the children learned every verse by heart before the summer was out. At last the whole town gathered in the old mill race to hear her sing.";
    let stop = chapter.find("in the old mill race,").unwrap() + "in the old mill race".len();
    // The word before the tail is misheard, so no five-word anchor covers the last words.
    let spoken = chapter[..stop].replacen("boats", "zzq", 1);
    let heard = read(&spoken, 0);
    let map = match_book(&[chapter], &heard);

    let tokens = tokenize(chapter);
    let last_timed = map.chapters[0]
        .words
        .iter()
        .rposition(|w| w.timing.span().is_some())
        .unwrap();
    let said = tokenize(&chapter[..stop]).len();
    assert_eq!(
        last_timed + 1,
        said,
        "timed up to {:?}",
        tokens[last_timed].norm
    );
}
