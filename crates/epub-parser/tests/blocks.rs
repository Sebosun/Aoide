mod support;

use aoide_epub::{parse_epub, Block, BlockKind, ParseOptions};

fn blocks(body: &str) -> Vec<Block> {
    let book = parse_epub(&support::single(body), &ParseOptions::default()).unwrap();
    book.chapters.into_iter().next().unwrap().blocks
}

fn block(kind: BlockKind, text: &str) -> Block {
    Block {
        kind,
        text: text.into(),
    }
}

#[test]
fn chapter_blocks_carry_kinds_and_join_to_the_chapter_text() {
    let bytes = support::single(
        "<h1>The  Arrival</h1><p>A boat reached<br/>the shore.</p><ul><li>One oar</li><li><p>Two sails</p></li></ul><pre>let x = 1;</pre><h2>Later</h2><div>Loose text</div>",
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    let chapter = &book.chapters[0];
    assert_eq!(
        chapter.blocks,
        vec![
            block(BlockKind::Heading { level: 1 }, "The Arrival"),
            block(BlockKind::Paragraph, "A boat reached"),
            block(BlockKind::Paragraph, "the shore."),
            block(BlockKind::ListItem, "One oar"),
            block(BlockKind::Paragraph, "Two sails"),
            block(BlockKind::Preformatted, "let x = 1;"),
            block(BlockKind::Heading { level: 2 }, "Later"),
            block(BlockKind::Paragraph, "Loose text"),
        ]
    );
    let joined: Vec<&str> = chapter.blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(chapter.text, joined.join("\n\n"));
}

#[test]
fn text_after_a_nested_block_reverts_to_the_enclosing_kind() {
    assert_eq!(
        blocks("<li>Item start<p>inner</p>item tail</li>"),
        vec![
            block(BlockKind::ListItem, "Item start"),
            block(BlockKind::Paragraph, "inner"),
            block(BlockKind::ListItem, "item tail"),
        ]
    );
}

#[test]
fn blocks_from_merged_spine_documents_are_appended_in_order() {
    let bytes = support::epub(
        r#"<item id="a" href="a.xhtml" media-type="application/xhtml+xml"/><item id="b" href="b.xhtml" media-type="application/xhtml+xml"/><item id="toc" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
        r#"<itemref idref="a"/><itemref idref="b"/>"#,
        &[
            (
                "OPS/toc.ncx",
                r#"<ncx><navMap><navPoint><navLabel><text>One</text></navLabel><content src="a.xhtml"/></navPoint></navMap></ncx>"#,
            ),
            ("OPS/a.xhtml", "<h1>One</h1><p>First part.</p>"),
            ("OPS/b.xhtml", "<p>Second part.</p>"),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    let chapter = &book.chapters[0];
    let texts: Vec<&str> = chapter.blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(texts, vec!["One", "First part.", "Second part."]);
    assert_eq!(chapter.text, texts.join("\n\n"));
}
