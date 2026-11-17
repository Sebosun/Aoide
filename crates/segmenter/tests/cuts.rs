//! Places a segment must end: a table's cells, each drawn in its own box.
use aoide_text::{segment_chapter, segment_chapter_with_cuts, Block, BlockKind, SegmentOptions};

fn row(text: &str) -> Block {
    Block {
        kind: BlockKind::Paragraph,
        text: text.into(),
        break_before: false,
    }
}

fn displays(segments: &[aoide_text::Segment]) -> Vec<(&str, &str)> {
    segments
        .iter()
        .map(|s| (s.id.as_str(), s.display.as_str()))
        .collect()
}

#[test]
fn a_segment_ends_at_each_cut_and_short_pieces_are_not_joined_across_one() {
    let blocks = [row("Year Lamps"), row("1902 14 and a half. Then more.")];
    let options = SegmentOptions::default();
    // Uncut, short lines join as they always did.
    assert_eq!(
        displays(&segment_chapter(0, &blocks, &options)),
        [
            ("0:0:0", "Year Lamps"),
            ("0:1:0", "1902 14 and a half. Then more.")
        ]
    );
    let cut = segment_chapter_with_cuts(0, &blocks, &[(0, 5), (1, 5)], &options);
    assert_eq!(
        displays(&cut),
        [
            ("0:0:0", "Year"),
            ("0:0:1", "Lamps"),
            ("0:1:0", "1902"),
            ("0:1:1", "14 and a half. Then more.")
        ]
    );
    // Ranges are still the chapter's text: the second line starts after "Year Lamps\n\n".
    assert_eq!(cut[2].range, 12..16);
    assert_eq!(cut[3].words[0], 17..19);
}
