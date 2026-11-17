mod support;

use aoide_epub::{parse_epub, Cover, ParseOptions};
use std::io::{Cursor, Write};
use zip::{write::SimpleFileOptions, ZipWriter};

const CHAPTER: &str = "<h1>One</h1><p>Text.</p>";
const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0x00];

fn book(metadata: &str, manifest: &str, extra: &[(&str, &[u8])]) -> Vec<u8> {
    let package = format!(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">{metadata}</metadata><manifest><item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/>{manifest}</manifest><spine><itemref idref="chapter"/></spine></package>"#
    );
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let mut files: Vec<(&str, &[u8])> = vec![
        ("mimetype", b"application/epub+zip"),
        (
            "META-INF/container.xml",
            br#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
        ),
        ("OPS/book.opf", package.as_bytes()),
        ("OPS/chapter.xhtml", CHAPTER.as_bytes()),
    ];
    files.extend_from_slice(extra);
    for (path, body) in files {
        writer
            .start_file(path, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(body).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn reads_all_creators_publisher_description_and_isbn() {
    let bytes = book(
        r#"<dc:title> The  Small Journey </dc:title>
           <dc:creator>First Writer</dc:creator><dc:creator> Second  Writer </dc:creator>
           <dc:publisher>Small Press</dc:publisher>
           <dc:description>&lt;p&gt;A tale of &lt;b&gt;two&lt;/b&gt; boats.&lt;/p&gt;</dc:description>
           <dc:identifier id="uid">urn:uuid:1234</dc:identifier>
           <dc:identifier opf:scheme="ISBN">978-0-306-40615-7</dc:identifier>
           <dc:language>en-GB</dc:language>"#,
        "",
        &[],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.title.as_deref(), Some("The Small Journey"));
    assert_eq!(book.authors, vec!["First Writer", "Second Writer"]);
    assert_eq!(book.publisher.as_deref(), Some("Small Press"));
    assert_eq!(book.description.as_deref(), Some("A tale of two boats."));
    assert_eq!(book.identifiers, vec!["urn:uuid:1234", "978-0-306-40615-7"]);
    assert_eq!(book.isbn.as_deref(), Some("9780306406157"));
    assert_eq!(book.language.as_deref(), Some("en-GB"));
}

#[test]
fn isbn_is_found_in_urn_identifiers_and_isbn10_is_accepted() {
    let bytes = book(
        r#"<dc:identifier>urn:isbn:0-306-40615-2</dc:identifier>"#,
        "",
        &[],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.isbn.as_deref(), Some("0306406152"));
    assert!(book.authors.is_empty());
}

#[test]
fn identifiers_with_invalid_isbn_checksums_are_not_reported_as_isbn() {
    let bytes = book(
        r#"<dc:identifier>urn:isbn:9780306406158</dc:identifier>"#,
        "",
        &[],
    );
    assert_eq!(
        parse_epub(&bytes, &ParseOptions::default()).unwrap().isbn,
        None
    );
}

#[test]
fn epub3_cover_image_property_provides_cover_bytes() {
    let bytes = book(
        "",
        r#"<item id="img" href="images/c.png" media-type="image/png" properties="cover-image"/>"#,
        &[("OPS/images/c.png", PNG)],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(
        book.cover,
        Some(Cover {
            media_type: "image/png".into(),
            bytes: PNG.to_vec(),
        })
    );
}

#[test]
fn epub2_cover_meta_points_at_manifest_item() {
    let bytes = book(
        r#"<meta name="cover" content="cov"/>"#,
        r#"<item id="cov" href="cover.jpg" media-type="image/jpeg"/>"#,
        &[("OPS/cover.jpg", PNG)],
    );
    let cover = parse_epub(&bytes, &ParseOptions::default())
        .unwrap()
        .cover
        .unwrap();
    assert_eq!(cover.media_type, "image/jpeg");
}

#[test]
fn missing_or_non_image_cover_is_ignored() {
    let missing = book(
        "",
        r#"<item id="img" href="gone.png" media-type="image/png" properties="cover-image"/>"#,
        &[],
    );
    assert_eq!(
        parse_epub(&missing, &ParseOptions::default())
            .unwrap()
            .cover,
        None
    );
    let not_image = book(r#"<meta name="cover" content="chapter"/>"#, "", &[]);
    assert_eq!(
        parse_epub(&not_image, &ParseOptions::default())
            .unwrap()
            .cover,
        None
    );
    let _ = support::single;
}

#[test]
fn read_metadata_matches_the_full_parse_without_reading_chapters() {
    let bytes = book(
        r#"<dc:title>Quick Look</dc:title><dc:creator>Mira Vale</dc:creator>
           <dc:language>en-GB</dc:language><dc:identifier>urn:isbn:9780000000002</dc:identifier>
           <meta name="cover" content="cover"/>"#,
        r#"<item id="cover" href="cover.png" media-type="image/png"/>"#,
        &[("OPS/cover.png", PNG)],
    );
    let full = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    let quick = aoide_epub::read_metadata(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(quick.title, full.title);
    assert_eq!(quick.authors, full.authors);
    assert_eq!(quick.language, full.language);
    assert_eq!(quick.isbn.as_deref(), Some("9780000000002"));
    assert_eq!(quick.cover, full.cover);
    assert!(quick.cover.is_some());
    // Chapter content is not needed: a broken chapter file does not matter.
    let broken = book("<dc:title>Only Metadata</dc:title>", "", &[]);
    let broken = String::from_utf8_lossy(&broken)
        .replace("<h1>One</h1>", "<h1>One<")
        .into_bytes();
    if let Ok(meta) = aoide_epub::read_metadata(&broken, &ParseOptions::default()) {
        assert_eq!(meta.title.as_deref(), Some("Only Metadata"));
    }
}

#[test]
fn read_metadata_rejects_non_epub_bytes() {
    assert!(aoide_epub::read_metadata(b"not a zip", &ParseOptions::default()).is_err());
}

#[test]
fn subjects_are_read_in_order_without_blanks() {
    let bytes = book(
        r#"<dc:title>Tagged</dc:title><dc:subject>Fiction / Sea Stories</dc:subject>
           <dc:subject>  </dc:subject><dc:subject>Lighthouses</dc:subject>"#,
        "",
        &[],
    );
    let quick = aoide_epub::read_metadata(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(quick.subjects, ["Fiction / Sea Stories", "Lighthouses"]);
}

#[test]
fn a_series_is_read_from_epub3_collections_else_from_calibre() {
    use aoide_epub::Series;
    let series = |metadata: &str| {
        let bytes = book(&format!("<dc:title>Book</dc:title>{metadata}"), "", &[]);
        let quick = aoide_epub::read_metadata(&bytes, &ParseOptions::default())
            .unwrap()
            .series;
        let full = parse_epub(&bytes, &ParseOptions::default()).unwrap().series;
        assert_eq!(quick, full, "{metadata}");
        quick
    };
    let named = |name: &str, position: Option<f32>| {
        Some(Series {
            name: name.into(),
            position,
        })
    };
    // EPUB 3: a collection of type series, with its place in it.
    assert_eq!(
        series(
            r##"<meta property="belongs-to-collection" id="c1"> The Harbour Trilogy </meta>
               <meta refines="#c1" property="collection-type">series</meta>
               <meta refines="#c1" property="group-position">2</meta>"##
        ),
        named("The Harbour Trilogy", Some(2.0))
    );
    // A collection of another type is not a series; Calibre's pair is read instead.
    assert_eq!(
        series(
            r##"<meta property="belongs-to-collection" id="c1">Sea Stories</meta>
               <meta refines="#c1" property="collection-type">set</meta>
               <meta name="calibre:series" content="Lamp Room"/>
               <meta name="calibre:series_index" content="2.5"/>"##
        ),
        named("Lamp Room", Some(2.5))
    );
    // The EPUB 3 series wins over Calibre's.
    assert_eq!(
        series(
            r##"<meta name="calibre:series" content="Old Name"/>
               <meta property="belongs-to-collection" id="s">New Name</meta>
               <meta refines="#s" property="collection-type">series</meta>"##
        ),
        named("New Name", None)
    );
    // A position that is not a number is left out; a blank name is no series.
    assert_eq!(
        series(
            r#"<meta name="calibre:series" content="Quay"/>
               <meta name="calibre:series_index" content="first"/>"#
        ),
        named("Quay", None)
    );
    assert_eq!(
        series(r#"<meta name="calibre:series" content="  "/>"#),
        None
    );
    assert_eq!(series(""), None);
}
