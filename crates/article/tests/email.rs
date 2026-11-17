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

/// Three newsletters as different senders build them: the body, and the mailing parts around
/// it that are not the letter.
const NEWSLETTERS: [&str; 3] = [
    // Paragraphs, a header link and a footer paragraph.
    r#"<html><body>
<p style="font-size:11px"><a href="https://quay.example/w/1">View this email in your browser</a></p>
<h1>Quay Notes</h1>
<p>The new crane on the north quay lifted its first container on Monday, two weeks late.</p>
<p>Dockers say it is quieter than the old one, and the gulls have not yet learned to sit on it.</p>
<p class="footer">You are receiving this because you signed up at quay.example. <a href="https://quay.example/u?id=7">Unsubscribe</a> · 1 Harbour Row, Portsea</p>
<img src="https://track.quay.example/o.gif" width="1" height="1" alt="">
</body></html>"#,
    // Tables, as most mailing tools write them.
    r#"<html><body><table width="100%"><tr><td>
<table><tr><td align="right"><a href="https://lights.example/web">Read online</a></td></tr></table>
<table><tr><td>
<h2>Lamp Room</h2>
<p>This month the lamp on the west head was cleaned and its glass replaced after the storm.</p>
<p>Keepers logged eleven ships passing in fog, the most since the spring of the year before.</p>
</td></tr></table>
<table><tr><td style="font-size:10px;color:#999">
<p>Lighthouse Letters, Lamp Lane 4.</p>
<p><a href="https://lights.example/prefs">Update your preferences</a> or <a href="https://lights.example/unsub">unsubscribe from this list</a>.</p>
</td></tr></table>
<img src="https://px.lights.example/p.png" style="display:none" alt="">
</td></tr></table></body></html>"#,
    // Divs, and a footer of several lines.
    r#"<html><body><div class="wrapper">
<div class="preheader">Trouble reading? <a href="https://tide.example/i/3">View it online</a>.</div>
<div class="content">
<p>Spring tides arrive on the ninth, and the ferry will run to the tide table, not the clock.</p>
<p>The pontoon on the outer harbour stays closed while divers check its moorings this week.</p>
</div>
<div class="footer">
<p>Copyright 2026 Tide Table Weekly.</p>
<p>Want to change how you receive these emails? <a href="https://tide.example/manage">Manage your subscription</a>.</p>
</div>
<img src="https://open.tide.example/x.gif" width="1" height="1">
</div></body></html>"#,
];

#[test]
fn a_newsletters_mailing_parts_are_left_out_and_reported_the_same_each_time() {
    use aoide_article::{extract, ExtractOptions};
    let email = ExtractOptions {
        email: true,
        ..ExtractOptions::default()
    };
    let url = url::Url::parse("email:letter@quay.example").unwrap();
    let kept = [
        ["first container on Monday", "gulls have not yet learned"],
        ["west head was cleaned", "eleven ships passing in fog"],
        [
            "ferry will run to the tide table",
            "divers check its moorings",
        ],
    ];
    let gone = [
        ["View this email", "Unsubscribe", "1 Harbour Row"],
        ["Read online", "unsubscribe from this list", "Lamp Lane 4"],
        [
            "Trouble reading",
            "Manage your subscription",
            "Copyright 2026",
        ],
    ];
    for (n, html) in NEWSLETTERS.iter().enumerate() {
        let read = extract(html, &url, &email);
        let text = &read.document.chapters[0].text;
        for words in kept[n] {
            assert!(
                text.contains(words),
                "newsletter {n} keeps {words:?}: {text}"
            );
        }
        for words in gone[n] {
            assert!(
                !text.contains(words),
                "newsletter {n} leaves out {words:?}: {text}"
            );
        }
        let excluded: Vec<(&str, String)> = read
            .document
            .excluded
            .iter()
            .map(|e| (e.chapter.title.as_str(), format!("{:?}", e.reason)))
            .collect();
        assert_eq!(
            excluded,
            [
                ("View online link", "Mailing".to_string()),
                ("Mailing footer", "Mailing".to_string()),
                ("Tracking image", "Mailing".to_string()),
            ],
            "newsletter {n}"
        );
        assert!(read.document.excluded[1].chapter.text.contains(gone[n][1]));
        // Read again, the same.
        assert_eq!(extract(html, &url, &email).document, read.document);
    }
}

#[test]
fn a_web_page_keeps_its_footer_and_a_short_email_its_only_paragraph() {
    use aoide_article::{extract, ExtractOptions};
    let url = url::Url::parse("https://quay.example/notes").unwrap();
    let web = extract(NEWSLETTERS[0], &url, &ExtractOptions::default());
    assert!(web.document.excluded.is_empty());
    let email = ExtractOptions {
        email: true,
        ..ExtractOptions::default()
    };
    // The whole letter is one line beside its unsubscribe link: it stays.
    let short = r#"<p>No ferry today. <a href="https://quay.example/unsub">Unsubscribe</a></p>"#;
    let read = extract(short, &url, &email);
    assert!(read.document.chapters[0].text.contains("No ferry today"));
}
