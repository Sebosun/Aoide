mod support;

use aoide_epub::{parse_epub, ParseError, ParseOptions};

#[test]
fn rejects_unsafe_zip_members_even_when_they_are_not_in_the_spine() {
    for path in [
        "../outside.txt",
        "/absolute.txt",
        "OPS/../../outside.txt",
        "OPS\\outside.txt",
    ] {
        let bytes = support::epub(
            r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/>"#,
            r#"<itemref idref="chapter"/>"#,
            &[
                ("OPS/chapter.xhtml", "<h1>Story</h1><p>Safe text.</p>"),
                (path, "not a book resource"),
            ],
        );
        assert!(
            matches!(
                parse_epub(&bytes, &ParseOptions::default()),
                Err(ParseError::UnsafePath(_))
            ),
            "accepted {path}"
        );
    }
}

#[test]
fn malformed_archives_containers_and_missing_resources_return_errors() {
    let cases = [
        b"not a ZIP".to_vec(),
        support::archive(&[("some.xml", "<not-an-epub/>")]),
        support::archive(&[("META-INF/container.xml", "<container/>")]),
        support::archive(&[("META-INF/container.xml", "<broken")]),
        support::epub(
            r#"<item id="chapter" href="missing.xhtml" media-type="application/xhtml+xml"/>"#,
            r#"<itemref idref="chapter"/>"#,
            &[],
        ),
        support::epub("", r#"<itemref idref="absent"/>"#, &[]),
        support::epub("", "", &[]),
    ];
    for bytes in cases {
        assert!(matches!(
            parse_epub(&bytes, &ParseOptions::default()),
            Err(ParseError::Invalid(_))
        ));
    }
}

#[test]
fn rejects_custom_xml_entities_instead_of_expanding_them() {
    let bytes = support::archive(&[(
        "META-INF/container.xml",
        r#"<!DOCTYPE container [<!ENTITY x "replacement">]><container>&x;</container>"#,
    )]);
    assert!(
        matches!(parse_epub(&bytes, &ParseOptions::default()), Err(ParseError::Invalid(message)) if message.contains("entities"))
    );
}

#[test]
fn rejects_external_and_escaping_package_references() {
    for href in [
        "../../outside.xhtml",
        "%2e%2e/%2e%2e/outside.xhtml",
        "https://example.com/book.xhtml",
        "/etc/passwd",
        "C:/book.xhtml",
    ] {
        let manifest =
            format!(r#"<item id="chapter" href="{href}" media-type="application/xhtml+xml"/>"#);
        let bytes = support::epub(&manifest, r#"<itemref idref="chapter"/>"#, &[]);
        assert!(
            matches!(
                parse_epub(&bytes, &ParseOptions::default()),
                Err(ParseError::UnsafePath(_))
            ),
            "accepted {href}"
        );
    }
}

#[test]
fn font_obfuscation_does_not_prevent_reading_unencrypted_chapter_text() {
    let bytes = support::epub(
        r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/>"#,
        r#"<itemref idref="chapter"/>"#,
        &[
            ("OPS/chapter.xhtml", "<p>A perfectly readable story.</p>"),
            (
                "META-INF/encryption.xml",
                r#"<encryption><EncryptedData><EncryptionMethod Algorithm="http://www.idpf.org/2008/embedding"/><CipherData><CipherReference URI="OPS/fonts/book.otf"/></CipherData></EncryptedData></encryption>"#,
            ),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters[0].text, "A perfectly readable story.");
}

#[test]
fn rejects_encrypted_reading_content_instead_of_treating_it_as_plain_text() {
    let bytes = support::epub(
        r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/>"#,
        r#"<itemref idref="chapter"/>"#,
        &[
            (
                "OPS/chapter.xhtml",
                "<p>This would be encrypted in a protected book.</p>",
            ),
            (
                "META-INF/encryption.xml",
                r#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><EncryptedData xmlns="http://www.w3.org/2001/04/xmlenc#"><EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes128-cbc"/><CipherData><CipherReference URI="OPS/chapter.xhtml"/></CipherData></EncryptedData></encryption>"#,
            ),
        ],
    );
    assert!(
        matches!(parse_epub(&bytes, &ParseOptions::default()), Err(ParseError::UnsupportedEncryption(path)) if path == "OPS/chapter.xhtml")
    );
}

#[test]
fn enforces_archive_entry_and_decompression_budgets() {
    let bytes = support::single(&format!("<p>{}</p>", "A".repeat(2048)));
    for (options, limit) in [
        (
            ParseOptions {
                max_archive_bytes: 10,
                ..Default::default()
            },
            "archive bytes",
        ),
        (
            ParseOptions {
                max_entries: 2,
                ..Default::default()
            },
            "entry count",
        ),
        (
            ParseOptions {
                max_entry_bytes: 1024,
                ..Default::default()
            },
            "entry bytes",
        ),
        (
            ParseOptions {
                max_uncompressed_bytes: 2500,
                ..Default::default()
            },
            "uncompressed bytes",
        ),
    ] {
        assert!(
            matches!(parse_epub(&bytes, &options), Err(ParseError::LimitExceeded(actual)) if actual == limit),
            "did not enforce {limit}"
        );
    }
}
