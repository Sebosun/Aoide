use aoide_text::{
    block_offsets, estimate_word_timings, segment_at, segment_chapter, Block, BlockKind,
    SegmentOptions,
};

fn p(text: &str) -> Block {
    Block {
        kind: BlockKind::Paragraph,
        text: text.into(),
    }
}

fn h(text: &str) -> Block {
    Block {
        kind: BlockKind::Heading { level: 1 },
        text: text.into(),
    }
}

fn chapter_text(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn displays(blocks: &[Block], options: &SegmentOptions) -> Vec<String> {
    segment_chapter(0, blocks, options)
        .into_iter()
        .map(|s| s.display)
        .collect()
}

#[test]
fn block_offsets_count_chars_and_blank_line_separators() {
    let blocks = [p("Żółw."), p("Ab"), p("c")];
    assert_eq!(block_offsets(&blocks), vec![0, 7, 11]);
}

#[test]
fn long_sentences_become_their_own_segments_with_exact_ranges() {
    let blocks = [
        h("The Arrival"),
        p("The boat reached the grey shore before the storm arrived. Everyone aboard the small boat was very tired and wet."),
    ];
    let text = chapter_text(&blocks);
    let segments = segment_chapter(3, &blocks, &SegmentOptions::default());
    assert_eq!(segments.len(), 3);
    assert_eq!(segments[0].id, "3:0:0");
    assert_eq!(segments[1].id, "3:1:0");
    assert_eq!(segments[2].id, "3:1:1");
    assert_eq!(
        segments[1].display,
        "The boat reached the grey shore before the storm arrived."
    );
    assert_eq!(
        segments[2].display,
        "Everyone aboard the small boat was very tired and wet."
    );
    for segment in &segments {
        let slice: String = text
            .chars()
            .skip(segment.range.start)
            .take(segment.range.len())
            .collect();
        assert_eq!(slice, segment.display);
    }
    assert_eq!(segments[1].block, 1);
    assert_eq!(segments[1].sentences, vec![segments[1].range.clone()]);
}

#[test]
fn headings_get_a_pause_and_terminal_punctuation_in_speech_only() {
    let segments = segment_chapter(
        0,
        &[h("Chapter One"), p("It began.")],
        &SegmentOptions::default(),
    );
    assert_eq!(segments[0].display, "Chapter One");
    assert_eq!(segments[0].speech, "Chapter One.");
    assert_eq!(segments[0].pause_after_ms, 700);
    assert_eq!(segments[1].pause_after_ms, 0);
    assert_eq!(segments[1].speech, "It began.");
}

#[test]
fn short_sentences_are_grouped_within_a_block_but_never_across_blocks() {
    let blocks = [p("Yes. No. Maybe so."), p("Fine.")];
    let segments = segment_chapter(0, &blocks, &SegmentOptions::default());
    assert_eq!(
        segments
            .iter()
            .map(|s| s.display.as_str())
            .collect::<Vec<_>>(),
        vec!["Yes. No. Maybe so.", "Fine."]
    );
    assert_eq!(segments[0].sentences.len(), 3);
    assert_eq!(segments[0].sentences[1], 5..8);
}

#[test]
fn abbreviations_and_initials_do_not_end_sentences() {
    let options = SegmentOptions {
        min_chars: 0,
        ..Default::default()
    };
    assert_eq!(
        displays(
            &[p(
                "Mr. Hale met Dr. Price near St. Albans. J. R. Moss wrote it, e.g. in 1938."
            )],
            &options
        ),
        vec![
            "Mr. Hale met Dr. Price near St. Albans.",
            "J. R. Moss wrote it, e.g. in 1938."
        ]
    );
}

#[test]
fn sentences_over_the_limit_split_at_clauses_then_words() {
    let options = SegmentOptions {
        max_chars: 30,
        min_chars: 0,
        ..Default::default()
    };
    let parts = displays(
        &[p(
            "One two three four five, six seven eight nine ten eleven twelve thirteen.",
        )],
        &options,
    );
    assert_eq!(parts[0], "One two three four five,");
    assert!(parts.iter().all(|p| p.chars().count() <= 30), "{parts:?}");
    assert_eq!(
        parts.join(" "),
        "One two three four five, six seven eight nine ten eleven twelve thirteen."
    );
}

#[test]
fn bracketed_numeric_note_markers_are_not_spoken() {
    let segments = segment_chapter(
        0,
        &[p("The sea was calm.[12] Then[3] it rose.")],
        &SegmentOptions {
            min_chars: 0,
            ..Default::default()
        },
    );
    assert_eq!(segments[0].display, "The sea was calm.[12]");
    assert_eq!(segments[0].speech, "The sea was calm.");
    assert_eq!(segments[1].speech, "Then it rose.");
}

#[test]
fn segments_without_letters_or_digits_have_empty_speech() {
    let segments = segment_chapter(0, &[p("* * *"), p("[4]")], &SegmentOptions::default());
    assert_eq!(segments.len(), 2);
    assert!(segments.iter().all(|s| s.speech.is_empty()));
}

#[test]
fn words_are_whitespace_separated_chapter_ranges() {
    let blocks = [p("Hi"), p("Sea  was calm.")];
    let segments = segment_chapter(0, &blocks, &SegmentOptions::default());
    assert_eq!(segments[1].words, vec![4..7, 9..12, 13..18]);
}

#[test]
fn segment_at_finds_containing_or_next_segment() {
    let blocks = [
        p("Alpha beta gamma delta epsilon zeta eta theta."),
        p("Second block here, quite long enough to stand."),
    ];
    let segments = segment_chapter(0, &blocks, &SegmentOptions::default());
    assert_eq!(segment_at(&segments, 0), Some(0));
    assert_eq!(segment_at(&segments, 10), Some(0));
    let gap = segments[0].range.end; // first separator char
    assert_eq!(segment_at(&segments, gap), Some(1));
    assert_eq!(segment_at(&segments, segments[1].range.end + 5), None);
}

#[test]
fn word_estimates_cover_the_duration_in_order_weighted_by_length() {
    let segments = segment_chapter(
        0,
        &[p("I walked, slowly, homeward.")],
        &SegmentOptions::default(),
    );
    let estimates = estimate_word_timings(&segments[0], 3000);
    assert_eq!(estimates.len(), 4);
    assert_eq!(estimates[0].start_ms, 0);
    assert_eq!(estimates.last().unwrap().end_ms, 3000);
    for pair in estimates.windows(2) {
        assert_eq!(pair[0].end_ms, pair[1].start_ms);
    }
    let len = |i: usize| estimates[i].end_ms - estimates[i].start_ms;
    assert!(len(0) < len(1), "short word gets less time: {estimates:?}");
    assert!(len(3) > len(1));
    assert!(estimate_word_timings(&segments[0], 0)
        .iter()
        .all(|e| e.end_ms == 0));
}

#[test]
fn output_is_deterministic() {
    let blocks = [h("A"), p("Some text. More text here! And a question? Yes.")];
    let options = SegmentOptions::default();
    assert_eq!(
        segment_chapter(1, &blocks, &options),
        segment_chapter(1, &blocks, &options)
    );
}

#[test]
fn numbering_abbreviations_only_continue_before_numbers() {
    let options = SegmentOptions {
        min_chars: 0,
        ..Default::default()
    };
    assert_eq!(
        displays(&[p("See No. 5 in the list. No. That is wrong.")], &options),
        vec!["See No. 5 in the list.", "No.", "That is wrong."]
    );
}
