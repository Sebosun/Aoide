//! Finding what characters say, and segments that never mix narration and dialogue.
use aoide_text::{quoted_spans, segment_chapter, Block, BlockKind, Role, SegmentOptions};

/// The quoted parts of `text`, as text.
fn quoted(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    quoted_spans(&chars)
        .into_iter()
        .map(|r| chars[r].iter().collect())
        .collect()
}

#[test]
fn each_quote_style_is_found() {
    assert_eq!(
        quoted("“Come in,” she said. “The tea is hot.”"),
        ["“Come in,”", "“The tea is hot.”"]
    );
    assert_eq!(quoted(r#"He said "no" twice."#), [r#""no""#]);
    assert_eq!(quoted("« Entrez », dit-elle."), ["« Entrez »"]);
    assert_eq!(quoted("„Komm herein“, sagte sie."), ["„Komm herein“"]);
    assert_eq!(quoted("‘Wait here,’ he said."), ["‘Wait here,’"]);
}

#[test]
fn apostrophes_are_not_quotes() {
    assert!(quoted("It’s the harbour master’s lamp, isn’t it?").is_empty());
    assert_eq!(
        quoted("‘Don’t go,’ she said, ‘it’s late.’"),
        ["‘Don’t go,’", "‘it’s late.’"]
    );
    // Single quotes count only where no double quote is used.
    assert_eq!(
        quoted("“That’s the ‘new’ pier,” he said."),
        ["“That’s the ‘new’ pier,”"]
    );
}

#[test]
fn a_quote_left_open_runs_to_the_paragraphs_end() {
    // A long speech: its paragraph does not close it, the next opens again.
    assert_eq!(
        quoted("“The boats came in at dusk, every one."),
        ["“The boats came in at dusk, every one."]
    );
    assert!(quoted("No quotes here at all.").is_empty());
}

fn segments(text: &str, split: bool) -> Vec<aoide_text::Segment> {
    let blocks = [Block {
        kind: BlockKind::Paragraph,
        text: text.into(),
        break_before: false,
    }];
    segment_chapter(
        0,
        &blocks,
        &SegmentOptions {
            split_dialogue: split,
            ..SegmentOptions::default()
        },
    )
}

fn roles(text: &str) -> Vec<(String, Role)> {
    segments(text, true)
        .into_iter()
        .map(|s| (s.display, s.role))
        .collect()
}

#[test]
fn a_segment_never_mixes_narration_and_dialogue() {
    use Role::*;
    let text =
        "“I have walked this quay every evening for thirty years,” the harbour master said. \
                She lit the last lamp. “And I will walk it again tomorrow night.”";
    assert_eq!(
        roles(text),
        [
            (
                "“I have walked this quay every evening for thirty years,”".into(),
                Dialogue
            ),
            (
                "the harbour master said. She lit the last lamp.".into(),
                Narration
            ),
            (
                "“And I will walk it again tomorrow night.”".into(),
                Dialogue
            ),
        ]
    );
    // Off: as it always was, one role.
    assert!(segments(text, false).iter().all(|s| s.role == Narration));
}

#[test]
fn a_short_reply_stays_with_the_narration_around_it() {
    use Role::*;
    assert_eq!(
        roles("She looked up and said “yes” before turning back to the nets."),
        [(
            "She looked up and said “yes” before turning back to the nets.".into(),
            Narration
        )]
    );
    // Alone in its sentence, it is said by the second voice.
    assert_eq!(
        roles("“Yes.” The nets were dry by noon, and the boats were in."),
        [
            ("“Yes.”".into(), Dialogue),
            (
                "The nets were dry by noon, and the boats were in.".into(),
                Narration
            )
        ]
    );
}

#[test]
fn split_segments_still_cover_the_text_word_for_word() {
    let text = "“Come in,” she said, “the tea is getting cold and the fire is low.” He did.";
    let chars: Vec<char> = text.chars().collect();
    let split = segments(text, true);
    for segment in &split {
        let shown: String = chars[segment.range.clone()].iter().collect();
        assert_eq!(segment.display, shown);
        for word in &segment.words {
            assert!(segment.range.start <= word.start && word.end <= segment.range.end);
        }
    }
    let words = |segments: &[aoide_text::Segment]| -> Vec<String> {
        segments
            .iter()
            .flat_map(|s| {
                s.words
                    .iter()
                    .map(|w| chars[w.clone()].iter().collect::<String>())
            })
            .collect()
    };
    assert_eq!(words(&split), words(&segments(text, false)));
    assert!(split.len() > 1);
}
