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
            Warning::LeftOut {
                what: aoide_document::LeftOut::Table,
                count: 1
            },
        ]
    );
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
