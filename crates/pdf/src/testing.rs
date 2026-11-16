//! Tiny PDFs written by hand, for tests (feature `testing`): pages of text lines at given places in Helvetica, an outline and
//! an Info dictionary. Original text only.

/// A line of text: where its baseline starts (points from the bottom left) and its size.
pub struct Line {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub text: String,
}

pub fn line(y: f32, text: &str) -> Line {
    Line {
        x: 72.0,
        y,
        size: 11.0,
        text: text.into(),
    }
}

pub fn sized(y: f32, size: f32, text: &str) -> Line {
    Line {
        x: 72.0,
        y,
        size,
        text: text.into(),
    }
}

/// Lines from the top of a page down, 14 points apart, starting at `top`.
pub fn lines_from(top: f32, texts: &[&str]) -> Vec<Line> {
    texts
        .iter()
        .enumerate()
        .map(|(i, t)| line(top - 14.0 * i as f32, t))
        .collect()
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

/// A PDF of `pages`, with top-level bookmarks `outline` (title, page index) and `info` entries.
pub fn pdf(pages: &[Vec<Line>], outline: &[(&str, usize)], info: &[(&str, &str)]) -> Vec<u8> {
    let mut objects: Vec<String> = Vec::new();
    // 1: catalog, 2: pages, 3: font; then page/content pairs, then outline, then info.
    let page_ids: Vec<usize> = (0..pages.len()).map(|i| 4 + 2 * i).collect();
    let after_pages = 4 + 2 * pages.len();
    let outline_root = (!outline.is_empty()).then_some(after_pages);
    let first_item = after_pages + 1;
    let info_id = after_pages
        + if outline.is_empty() {
            0
        } else {
            1 + outline.len()
        };
    let catalog = match outline_root {
        Some(root) => format!("<< /Type /Catalog /Pages 2 0 R /Outlines {root} 0 R >>"),
        None => "<< /Type /Catalog /Pages 2 0 R >>".into(),
    };
    objects.push(catalog);
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    objects.push(format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        pages.len()
    ));
    objects.push(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
    );
    for (i, page) in pages.iter().enumerate() {
        let mut content = String::new();
        for l in page {
            content.push_str(&format!(
                "BT /F1 {} Tf {} {} Td ({}) Tj ET\n",
                l.size,
                l.x,
                l.y,
                escape(&l.text)
            ));
        }
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            page_ids[i] + 1
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ));
    }
    if let Some(root) = outline_root {
        let last = first_item + outline.len() - 1;
        objects.push(format!(
            "<< /Type /Outlines /First {first_item} 0 R /Last {last} 0 R /Count {} >>",
            outline.len()
        ));
        for (i, (title, page)) in outline.iter().enumerate() {
            let id = first_item + i;
            let mut entry = format!(
                "<< /Title ({}) /Parent {root} 0 R /Dest [{} 0 R /Fit]",
                escape(title),
                page_ids[*page]
            );
            if i > 0 {
                entry.push_str(&format!(" /Prev {} 0 R", id - 1));
            }
            if i + 1 < outline.len() {
                entry.push_str(&format!(" /Next {} 0 R", id + 1));
            }
            entry.push_str(" >>");
            objects.push(entry);
        }
    }
    let info_entries: String = info
        .iter()
        .map(|(k, v)| format!("/{k} ({}) ", escape(v)))
        .collect();
    objects.push(format!("<< {info_entries}>>"));
    assert_eq!(objects.len(), info_id);

    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info {info_id} 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}
