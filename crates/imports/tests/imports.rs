//! Each read-later app's export, on small synthetic files with the app's own header.
use aoide_imports::{detect, read, Source};
use std::io::Write;

fn urls(links: &[aoide_imports::SavedLink]) -> Vec<&str> {
    links.iter().map(|l| l.url.as_str()).collect()
}

#[test]
fn pocket_csv_with_tags_status_and_time() {
    let csv = "title,url,time_added,tags,status\n\
               Tides of the Quay,https://example.org/tides,1700000000,sea|harbours,unread\n\
               \"A title, with a comma\",https://example.org/lamp?utm_source=x,1600000000,,archive\n\
               Not a page,mailto:someone@example.org,1600000000,,unread\n";
    assert_eq!(
        detect("part_000000.csv", csv.as_bytes()),
        Some(Source::Pocket)
    );
    let read = read(Source::Pocket, csv.as_bytes()).unwrap();
    assert_eq!(
        urls(&read.links),
        [
            "https://example.org/tides",
            "https://example.org/lamp?utm_source=x"
        ]
    );
    let first = &read.links[0];
    assert_eq!(first.title.as_deref(), Some("Tides of the Quay"));
    assert_eq!(first.saved_at, Some(1_700_000_000_000));
    assert_eq!(first.labels, ["sea", "harbours"]);
    assert!(!first.archived);
    assert_eq!(
        read.links[1].title.as_deref(),
        Some("A title, with a comma")
    );
    assert!(read.links[1].archived);
    assert_eq!(read.not_web, 1);
}

#[test]
fn pocket_html_from_the_older_export() {
    let html = r#"<!DOCTYPE html><html><head><title>Pocket Export</title></head><body>
        <h1>Unread</h1><ul>
        <li><a href="https://example.org/a" time_added="1700000000" tags="sea,quay">Quay</a></li>
        </ul><h1>Read Archive</h1><ul>
        <li><a href="https://example.org/b" time_added="1600000000" tags="">Lamp</a></li>
        </ul></body></html>"#;
    assert_eq!(
        detect("ril_export.html", html.as_bytes()),
        Some(Source::Pocket)
    );
    let read = read(Source::Pocket, html.as_bytes()).unwrap();
    assert_eq!(
        urls(&read.links),
        ["https://example.org/a", "https://example.org/b"]
    );
    assert_eq!(read.links[0].labels, ["sea", "quay"]);
    assert!(!read.links[0].archived && read.links[1].archived);
    assert_eq!(read.links[1].saved_at, Some(1_600_000_000_000));
}

#[test]
fn instapaper_csv_folders_and_archive() {
    let csv = "URL,Title,Selection,Folder,Timestamp\n\
               https://example.org/a,Quay,,Unread,1700000000\n\
               https://example.org/b,Lamp,a quote,Archive,1600000000\n\
               https://example.org/c,Gulls,,Harbours,1500000000\n";
    assert_eq!(
        detect("instapaper-export.csv", csv.as_bytes()),
        Some(Source::Instapaper)
    );
    let read = read(Source::Instapaper, csv.as_bytes()).unwrap();
    assert_eq!(read.links.len(), 3);
    assert!(read.links[1].archived);
    assert!(read.links[0].labels.is_empty() && read.links[1].labels.is_empty());
    assert_eq!(read.links[2].labels, ["Harbours"]);
    assert_eq!(read.links[2].saved_at, Some(1_500_000_000_000));
}

#[test]
fn readwise_reader_csv() {
    let csv = "Title,URL,ID,Document tags,Saved date,Reading progress,Location,Seen\n\
               Quay,https://example.org/a,01h,\"['sea', 'quay']\",2023-05-01 12:34:56+00:00,0.5,later,True\n\
               Lamp,https://example.org/b,01j,,2023-05-02 08:00:00+00:00,1,archive,True\n";
    assert_eq!(
        detect("Readwise_export.csv", csv.as_bytes()),
        Some(Source::Readwise)
    );
    let read = read(Source::Readwise, csv.as_bytes()).unwrap();
    assert_eq!(read.links[0].labels, ["sea", "quay"]);
    assert_eq!(read.links[0].saved_at, Some(1_682_944_496_000));
    assert!(!read.links[0].archived && read.links[1].archived);
}

#[test]
fn wallabag_json_with_the_page_text() {
    let json = r#"[{"is_archived":1,"is_starred":0,"tags":["sea"],"id":7,"title":"Quay",
        "url":"https://example.org/a","content":"<p>The quay lights.</p>",
        "created_at":"2023-01-01T12:00:00+0100","domain_name":"example.org"},
        {"is_archived":0,"tags":[],"title":"Lamp","url":"https://example.org/b",
        "content":"","created_at":"2023-01-02T12:00:00+0100"}]"#;
    assert_eq!(
        detect("All articles.json", json.as_bytes()),
        Some(Source::Wallabag)
    );
    let read = read(Source::Wallabag, json.as_bytes()).unwrap();
    let first = &read.links[0];
    assert_eq!(
        first.content_html.as_deref(),
        Some("<p>The quay lights.</p>")
    );
    assert!(first.archived);
    assert_eq!(first.labels, ["sea"]);
    assert_eq!(first.saved_at, Some(1_672_570_800_000));
    // Empty content is no content.
    assert_eq!(read.links[1].content_html, None);
}

#[test]
fn omnivore_zip_with_metadata_and_saved_pages() {
    let metadata = r#"[{"id":"1","slug":"quay-a1","title":"Quay","url":"https://example.org/a",
        "state":"Archived","labels":["sea"],"savedAt":"2023-03-01T10:00:00.000Z"},
        {"id":"2","slug":"lamp-b2","title":"Lamp","url":"https://example.org/b",
        "state":"Succeeded","labels":[],"savedAt":"2023-03-02T10:00:00.000Z"}]"#;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("metadata_0_to_2.json", options).unwrap();
    zip.write_all(metadata.as_bytes()).unwrap();
    zip.start_file("content/quay-a1.html", options).unwrap();
    zip.write_all(b"<p>Saved quay.</p>").unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    assert_eq!(
        detect("omnivore-export.zip", &bytes),
        Some(Source::Omnivore)
    );
    let read = read(Source::Omnivore, &bytes).unwrap();
    assert_eq!(
        read.links[0].content_html.as_deref(),
        Some("<p>Saved quay.</p>")
    );
    assert!(read.links[0].archived);
    assert_eq!(read.links[0].saved_at, Some(1_677_664_800_000));
    assert_eq!(read.links[1].content_html, None);
    assert!(!read.links[1].archived);
}

#[test]
fn browser_bookmarks_and_a_plain_list() {
    let html = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
        <TITLE>Bookmarks</TITLE><H1>Bookmarks</H1><DL><p>
        <DT><H3>Reading</H3><DL><p>
        <DT><A HREF="https://example.org/a" ADD_DATE="1700000000" TAGS="sea,quay">Quay</A>
        <DT><A HREF="javascript:void(0)">A bookmarklet</A>
        </DL><p></DL>"#;
    assert_eq!(
        detect("bookmarks.html", html.as_bytes()),
        Some(Source::Bookmarks)
    );
    let marks = read(Source::Bookmarks, html.as_bytes()).unwrap();
    assert_eq!(urls(&marks.links), ["https://example.org/a"]);
    assert_eq!(marks.links[0].labels, ["sea", "quay"]);
    assert_eq!(marks.not_web, 1);

    let list =
        "https://example.org/a\n\n  https://example.org/b  \nftp://example.org/c\n# a note\n";
    assert_eq!(
        detect("links.txt", list.as_bytes()),
        Some(Source::PlainList)
    );
    let read = read(Source::PlainList, list.as_bytes()).unwrap();
    assert_eq!(
        urls(&read.links),
        ["https://example.org/a", "https://example.org/b"]
    );
    assert_eq!(read.not_web, 1);
}

#[test]
fn something_else_is_not_an_export() {
    assert_eq!(detect("photo.png", b"\x89PNG\r\n"), None);
    assert!(read(Source::Wallabag, b"not json").is_err());
    assert!(read(Source::Pocket, b"name,address\nx,y\n").is_err());
}
