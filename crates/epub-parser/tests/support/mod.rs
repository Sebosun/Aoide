#![allow(dead_code)] // Shared by several test crates; each uses a subset.
use std::io::{Cursor, Write};
use zip::{write::SimpleFileOptions, ZipWriter};

pub fn archive(files: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (path, body) in files {
        writer
            .start_file(*path, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(body.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

pub fn epub(manifest: &str, spine: &str, files: &[(&str, &str)]) -> Vec<u8> {
    epub_with_guide(manifest, spine, "", files)
}

pub fn epub_with_guide(
    manifest: &str,
    spine: &str,
    guide: &str,
    files: &[(&str, &str)],
) -> Vec<u8> {
    let package = format!(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
        <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
          <dc:title>A Small Journey</dc:title><dc:creator>Test Author</dc:creator><dc:language>en</dc:language>
        </metadata><manifest>{manifest}</manifest><spine>{spine}</spine><guide>{guide}</guide></package>"#
    );
    let mut entries = vec![
        ("mimetype", "application/epub+zip"),
        (
            "META-INF/container.xml",
            r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
        ),
        ("OPS/book.opf", package.as_str()),
    ];
    entries.extend_from_slice(files);
    archive(&entries)
}

pub fn single(body: &str) -> Vec<u8> {
    epub(
        r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/>"#,
        r#"<itemref idref="chapter"/>"#,
        &[("OPS/chapter.xhtml", body)],
    )
}
