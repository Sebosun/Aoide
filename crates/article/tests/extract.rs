use aoide_article::{extract, ExtractOptions};
use aoide_document::{BlockKind, Warning};
use url::Url;

fn page(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/pages/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn url() -> Url {
    Url::parse("https://garden.example/bees-on-a-balcony").unwrap()
}

#[test]
fn the_article_body_becomes_one_chapter_of_blocks_without_the_page_around_it() {
    let extracted = extract(&page("plain.html"), &url(), &ExtractOptions::default());
    let document = &extracted.document;
    assert_eq!(document.chapters.len(), 1);
    let chapter = &document.chapters[0];
    let blocks: Vec<(BlockKind, &str)> = chapter
        .blocks
        .iter()
        .map(|b| (b.kind, b.text.as_str()))
        .collect();
    let heading = |level| BlockKind::Heading { level };
    assert_eq!(
        blocks,
        [
            (BlockKind::Paragraph, "A balcony four floors up is not where most people picture a hive, yet the bees on Corvin Street have done well there for three summers. The trick, their keeper says, is to think about wind before anything else."),
            (heading(2), "Choosing the spot"),
            (BlockKind::Paragraph, "Bees want morning sun and shelter from the gusts that sweep between buildings. A corner with a solid wall on two sides is ideal, and a screen of tall grasses helps the rest."),
            (BlockKind::Paragraph, "Keep the entrance facing away from the door you use, so that you and the foragers are not always in each other's way."),
            (heading(2), "What you will need"),
            (BlockKind::ListItem, "A small hive with a sloped roof"),
            (BlockKind::ListItem, "A shallow water dish with stones in it"),
            (BlockKind::ListItem, "Plants that flower from spring to autumn"),
            (heading(2), "Keeping records"),
            (BlockKind::Paragraph, "Write down what you see at every inspection. A plain text file works as well as any app, as long as you keep it in one place."),
            (BlockKind::Preformatted, "date: 12 May\nbrood: good\ntemper: calm"),
            (BlockKind::Paragraph, "Bees forgive a slow keeper, but never a clumsy one."),
            (BlockKind::Paragraph, "By the end of the first season the hive gave nine jars of honey, which went to the neighbours who had put up with the buzzing."),
        ]
    );
    let texts: Vec<&str> = chapter.blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(chapter.text, texts.join("\n\n"));
    for left in ["Popular this week", "Subscribe", "Copyright", "About us"] {
        assert!(!chapter.text.contains(left), "{left} was kept");
    }
    assert_eq!(
        document.warnings,
        [
            Warning::LeftOut {
                what: aoide_document::LeftOut::Image,
                count: 1
            },
            Warning::LeftOut {
                what: aoide_document::LeftOut::Figure,
                count: 1
            },
        ]
    );
}

#[test]
fn a_data_table_is_kept_beside_the_text_and_a_layout_table_left_out() {
    use aoide_document::{Insert, InsertKind};
    let extracted = extract(&page("plain.html"), &url(), &ExtractOptions::default());
    let chapter = &extracted.document.chapters[0];
    let at = chapter
        .blocks
        .iter()
        .position(|b| b.text == "Keeping records")
        .unwrap();
    assert_eq!(
        chapter.inserts,
        [Insert {
            before_block: at,
            kind: InsertKind::Table {
                rows: vec![
                    vec!["Month".into(), "Task".into()],
                    vec!["April".into(), "Check the queen".into()]
                ],
                header_rows: 1,
                caption: None,
            },
        }]
    );
    assert!(!chapter.text.contains("Check the queen"), "not in the text");

    let nested = "<html><body><article><h1>Grids</h1><p>A first paragraph that is long enough to be the article body here.</p>\
        <table><caption>Hives  by year</caption><thead><tr><th>Year</th><th>Hives</th></tr></thead><tbody><tr><td>2025</td><td>3</td></tr></tbody></table>\
        <table><tr><td><table><tr><td>Inner</td></tr></table></td></tr></table>\
        <p>A last paragraph that is long enough to be part of the article body too.</p></article></body></html>";
    let extracted = extract(nested, &url(), &ExtractOptions::default());
    let chapter = &extracted.document.chapters[0];
    let tables: Vec<&InsertKind> = chapter.inserts.iter().map(|i| &i.kind).collect();
    assert_eq!(
        tables,
        [&InsertKind::Table {
            rows: vec![
                vec!["Year".into(), "Hives".into()],
                vec!["2025".into(), "3".into()]
            ],
            header_rows: 1,
            caption: Some("Hives by year".into()),
        }]
    );
    // The layout table (one inside another) is not drawn as one.
}

#[test]
fn extraction_is_the_same_every_time() {
    let html = page("plain.html");
    let first = extract(&html, &url(), &ExtractOptions::default());
    let second = extract(&html, &url(), &ExtractOptions::default());
    assert_eq!(first, second);
    assert_eq!(format!("{first:?}"), format!("{second:?}"));
}

#[test]
fn deep_nesting_and_huge_pages_stay_within_bounds() {
    // Past `max_depth` the page is read as plain text, chrome left out, in linear time.
    let depth = 50_000;
    let deep = format!(
        "<html><head><title>Boxes &amp; more</title><script>if (a<b) {{}}</script></head><body>\
         <nav><a href=\"/\">Home</a></nav>{}<p>At the bottom of many boxes there is one sentence.</p>{}\
         <footer>All rights reserved.</footer></body></html>",
        "<div>".repeat(depth),
        "</div>".repeat(depth)
    );
    let started = std::time::Instant::now();
    let extracted = extract(&deep, &url(), &ExtractOptions::default());
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    let chapter = &extracted.document.chapters[0];
    assert_eq!(
        chapter.text,
        "At the bottom of many boxes there is one sentence."
    );
    assert_eq!(extracted.meta.title.as_deref(), Some("Boxes & more"));

    let sentence = "Every line of this long page says the same plain thing again. ";
    let paragraphs = format!("<p>{}</p>", sentence.repeat(40)).repeat(1_000);
    let huge = format!("<html><body><article>{paragraphs}</article></body></html>");
    let options = ExtractOptions {
        max_blocks: 100,
        max_text_bytes: 64 * 1024,
        ..ExtractOptions::default()
    };
    let extracted = extract(&huge, &url(), &options);
    let chapter = &extracted.document.chapters[0];
    assert!(!chapter.blocks.is_empty());
    assert!(chapter.blocks.len() <= 100);
    assert!(chapter.text.len() <= 64 * 1024 + sentence.len() * 40);
}

/// The blocks of a page whose article is `body`, as (kind, text).
fn blocks_of(body: &str) -> Vec<(BlockKind, String)> {
    let html = format!(
        "<html><head><title>Notes</title></head><body><article>{body}</article></body></html>"
    );
    extract(&html, &url(), &ExtractOptions::default())
        .document
        .chapters[0]
        .blocks
        .iter()
        .map(|b| (b.kind, b.text.clone()))
        .collect()
}

const FILLER: &str =
    "<p>Every spring the allotment wakes slowly, and the first job is always the same: \
    clear the paths, turn the beds and see what the winter left behind for the season ahead.</p>";

#[test]
fn a_rule_marks_the_block_after_it_as_after_a_scene_break_without_adding_one() {
    let html = format!(
        "<html><head><title>Notes</title></head><body><article>{FILLER}<hr>\
         <p>The second half of the season is quieter.</p>{FILLER}</article></body></html>"
    );
    let extracted = extract(&html, &url(), &ExtractOptions::default());
    let blocks = &extracted.document.chapters[0].blocks;
    assert_eq!(blocks.len(), 3);
    let breaks: Vec<bool> = blocks.iter().map(|b| b.break_before).collect();
    assert_eq!(breaks, [false, true, false]);

    // A page too deep for a tree is read as plain text, and keeps its rules too.
    let plain = extract(
        &html,
        &url(),
        &ExtractOptions {
            max_depth: 1,
            ..ExtractOptions::default()
        },
    );
    let breaks: Vec<bool> = plain.document.chapters[0]
        .blocks
        .iter()
        .map(|b| b.break_before)
        .collect();
    assert_eq!(breaks, [false, true, false]);
}

#[test]
fn a_list_item_written_as_paragraphs_is_one_item_and_what_follows_it() {
    let blocks = blocks_of(&format!(
        "{FILLER}<ol><li><p>Turn the compost before the frost.</p><p>Give it a week to settle.</p></li>\
         <li><p>Sow the broad beans.</p></li></ol>{FILLER}"
    ));
    assert_eq!(
        blocks[1..4],
        [
            (
                BlockKind::ListItem,
                "Turn the compost before the frost.".into()
            ),
            (BlockKind::Paragraph, "Give it a week to settle.".into()),
            (BlockKind::ListItem, "Sow the broad beans.".into()),
        ]
    );
}

#[test]
fn a_formula_is_read_in_its_sentence_without_its_source_annotation() {
    let blocks = blocks_of(&format!(
        "{FILLER}<p>When <math><semantics><mrow><mi>a</mi><mo>≠</mo><mn>0</mn></mrow>\
         <annotation encoding=\"application/x-tex\">a \\neq 0</annotation></semantics></math>, \
         there are two solutions.</p>{FILLER}"
    ));
    assert_eq!(
        blocks[1],
        (
            BlockKind::Paragraph,
            "When a ≠ 0, there are two solutions.".into()
        )
    );
}

#[test]
fn chinese_and_japanese_are_counted_by_their_characters() {
    // Original Chinese text: 55 characters, and three Latin words.
    let chinese = "<p>春天来了，花园里的蜜蜂开始忙碌起来。它们从早到晚在花丛中飞来飞去。</p>\
        <p>养蜂人说，今年的蜂蜜会比去年更甜，因为雨水来得正是时候。 Apis mellifera ligustica</p>";
    let html = format!(
        "<html><head><title>蜜蜂</title></head><body><article>{chinese}</article></body></html>"
    );
    let extracted = extract(&html, &url(), &ExtractOptions::default());
    let han = chinese
        .chars()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
        .count();
    assert_eq!(extracted.meta.word_count, han + 3);
    assert_eq!(
        extracted.completeness,
        aoide_article::Completeness::Complete
    );
}
