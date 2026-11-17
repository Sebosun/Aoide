//! Pictures, tables and code: what they add to a chapter, and that its text stays as it was.
mod support;

use aoide_epub::{parse_epub, BlockKind, ParseOptions};

/// A chapter with a picture, a figure with its caption, a table with a caption, and code.
pub const ILLUSTRATED: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
<h1>The Lighthouse</h1>
<p>The keeper climbed the stair.</p>
<img src="images/stair.png" alt="A spiral stair"/>
<p>At the top the lamp waited.</p>
<figure><img src="../OPS/images/lamp.png" alt="The great lamp"/><figcaption>The lamp, lit at dusk.</figcaption></figure>
<p>He counted the years.</p>
<table><caption>Lamps lit each year</caption>
<tr><th>Year</th><th>Lamps</th></tr>
<tr><td>1901</td><td>12</td></tr>
<tr><td>1902</td><td>14 and a half</td></tr>
</table>
<pre>fn light() {
    lamp.on();
}</pre>
<p>Then he slept.</p>
</body></html>"#;

pub fn illustrated() -> Vec<u8> {
    support::single(ILLUSTRATED)
}

/// What the parser made of the chapter before pictures, tables and code were read: the guard
/// that the display text (and so every stored place) stays the same.
#[test]
fn the_display_text_is_what_it_was() {
    let book = parse_epub(&illustrated(), &ParseOptions::default()).unwrap();
    let chapter = &book.chapters[0];
    let texts: Vec<&str> = chapter.blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "The Lighthouse",
            "The keeper climbed the stair.",
            "At the top the lamp waited.",
            "The lamp, lit at dusk.",
            "He counted the years.",
            "Lamps lit each year",
            "Year Lamps",
            "1901 12",
            "1902 14 and a half",
            "fn light() { lamp.on(); }",
            "Then he slept.",
        ]
    );
    assert_eq!(chapter.blocks[9].kind, BlockKind::Preformatted);
    assert_eq!(
        chapter.text,
        texts.join("\n\n"),
        "the text is the blocks joined"
    );
}
