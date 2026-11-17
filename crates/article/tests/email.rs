//! A saved email (`.eml`) read as a page: its HTML part, or its plain text in paragraphs.
use aoide_article::{from_email, EmailPage};

fn crlf(text: &str) -> Vec<u8> {
    text.replace('\n', "\r\n").into_bytes()
}

const ALTERNATIVE: &str = r#"From: "Harbour Weekly" <letters@harbour.example>
To: reader@example.org
Subject: =?UTF-8?B?VGlkZXMg4oCTIGEgd2Vla2x5IG5vdGU=?=
Date: Tue, 10 Nov 2026 07:30:00 +0000
Message-ID: <issue-41@harbour.example>
MIME-Version: 1.0
Content-Type: multipart/alternative; boundary="b1"

--b1
Content-Type: text/plain; charset=utf-8

The plain version of the note.

--b1
Content-Type: text/html; charset=utf-8
Content-Transfer-Encoding: quoted-printable

<html><body><p><a href=3D"https://harbour.example/issues/41?utm_source=3Dmail">View this email in your browser</a></p>
<h1>Tides</h1><p>The spring tides come early this year, and the harbour master has moved the =
moorings.</p></body></html>
--b1--
"#;

#[test]
fn the_html_part_is_the_page_with_the_headers_decoded() {
    let page = from_email(&crlf(ALTERNATIVE)).unwrap();
    assert_eq!(page.subject.as_deref(), Some("Tides – a weekly note"));
    assert_eq!(page.from.as_deref(), Some("Harbour Weekly"));
    assert_eq!(page.message_id.as_deref(), Some("issue-41@harbour.example"));
    let date = page.date.unwrap();
    assert_eq!(date.at_ms, Some(1_794_295_800_000));
    assert!(page.html.contains("<h1>Tides</h1>"));
    assert!(page.html.contains("has moved the moorings."));
    assert!(!page.html.contains("plain version"));
    assert_eq!(
        page.view_online.map(String::from).as_deref(),
        Some("https://harbour.example/issues/41?utm_source=mail")
    );
    assert_eq!(page.attachments, 0);
}

#[test]
fn a_plain_email_is_wrapped_in_paragraphs() {
    let mail = "From: keeper@lighthouse.example\nSubject: Lamp log\n\n\
                The lamp was lit at dusk.\nIt burned until <dawn> & after.\n\nSecond paragraph.\n";
    let page = from_email(&crlf(mail)).unwrap();
    // No display name: the address names the sender.
    assert_eq!(page.from.as_deref(), Some("keeper@lighthouse.example"));
    assert_eq!(page.message_id, None);
    assert_eq!(page.view_online, None);
    assert!(
        page.html.contains(
            "<p>The lamp was lit at dusk.\nIt burned until &lt;dawn&gt; &amp; after.</p>"
        ),
        "{}",
        page.html
    );
    assert!(page.html.contains("<p>Second paragraph.</p>"));
}

#[test]
fn attachments_are_left_out_and_counted() {
    let mail = r#"From: Quay Notes <notes@quay.example>
Subject: Notes with a chart
Message-ID: <n7@quay.example>
Content-Type: multipart/mixed; boundary="outer"

--outer
Content-Type: text/html; charset=utf-8

<p>The chart is attached.</p>
--outer
Content-Type: application/pdf; name="chart.pdf"
Content-Disposition: attachment; filename="chart.pdf"
Content-Transfer-Encoding: base64

JVBERi0xLjQK
--outer
Content-Type: image/png; name="buoy.png"
Content-Disposition: attachment; filename="buoy.png"
Content-Transfer-Encoding: base64

iVBORw0KGgo=
--outer--
"#;
    let page: EmailPage = from_email(&crlf(mail)).unwrap();
    assert_eq!(page.attachments, 2);
    assert!(page.html.contains("<p>The chart is attached.</p>"));
    assert!(!page.html.contains("JVBER"));
}

#[test]
fn a_view_online_link_is_only_a_web_address() {
    let mail = "From: a@b.example\nSubject: x\nContent-Type: text/html\n\n\
                <a href=\"mailto:a@b.example\">View online</a><p>Body text.</p>";
    assert_eq!(from_email(&crlf(mail)).unwrap().view_online, None);
    let mail = "From: a@b.example\nSubject: x\nContent-Type: text/html\n\n\
                <p>Read the <a href=\"https://b.example/web/9\">web version</a>.</p>";
    assert_eq!(
        from_email(&crlf(mail))
            .unwrap()
            .view_online
            .map(String::from)
            .as_deref(),
        Some("https://b.example/web/9")
    );
}

#[test]
fn something_that_is_not_an_email_is_none() {
    assert!(from_email(b"").is_none());
    assert!(from_email(b"%PDF-1.4 not a mail at all").is_none());
    assert!(from_email(b"<html><body><p>A page.</p></body></html>").is_none());
}

#[test]
fn the_page_made_from_an_email_reads_with_its_subject_sender_and_date() {
    use aoide_article::{extract, ExtractOptions};
    let page = from_email(&crlf(ALTERNATIVE)).unwrap();
    let url = url::Url::parse("email:issue-41@harbour.example").unwrap();
    let email = ExtractOptions {
        email: true,
        ..ExtractOptions::default()
    };
    let read = extract(&page.page(), &url, &email);
    assert_eq!(read.meta.title.as_deref(), Some("Tides – a weekly note"));
    assert_eq!(read.meta.byline.as_deref(), Some("Harbour Weekly"));
    assert_eq!(read.meta.site_name.as_deref(), Some("Harbour Weekly"));
    assert_eq!(
        read.meta.published.and_then(|p| p.at_ms),
        Some(1_794_295_800_000)
    );
    let text = &read.document.chapters[0].text;
    assert!(
        text.contains("The spring tides come early this year"),
        "{text}"
    );
}

#[test]
fn a_short_email_is_whole_and_an_empty_one_is_not() {
    use aoide_article::{extract, Completeness, ExtractOptions};
    let email = ExtractOptions {
        email: true,
        ..ExtractOptions::default()
    };
    let url = url::Url::parse("email:short@quay.example").unwrap();
    let short = from_email(&crlf(
        "From: a@quay.example\nSubject: Ferry\n\nNo ferry today.\n",
    ))
    .unwrap();
    assert_eq!(
        extract(&short.page(), &url, &email).completeness,
        Completeness::Complete
    );
    let empty = from_email(&crlf("From: a@quay.example\nSubject: Ferry\n\n\n")).unwrap();
    assert_eq!(
        extract(&empty.page(), &url, &email).completeness,
        Completeness::Empty
    );
}
