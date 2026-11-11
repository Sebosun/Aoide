use crate::{
    archive::Archive, content::Content, location::resolve, package::xml, ExclusionReason,
    ParseError, Warning,
};
use scraper::{ElementRef, Html, Selector};

pub(crate) struct Entry {
    pub path: String,
    pub fragment: Option<String>,
    pub title: String,
    pub depth: usize,
    pub kind: Option<ExclusionReason>,
    pub is_landmark: bool,
}

pub(crate) struct Navigation {
    pub path: String,
    pub is_ncx: bool,
}

impl Entry {
    pub fn offset(&self, content: &Content) -> Option<usize> {
        self.fragment
            .as_ref()
            .map_or(Some(0), |fragment| content.anchors.get(fragment).copied())
    }

    pub fn missing_warning(&self) -> Warning {
        Warning::NavigationTargetMissing {
            path: self.path.clone(),
            fragment: self.fragment.clone(),
        }
    }
}

pub(crate) fn read(
    archive: &mut Archive<'_>,
    navigation: Option<&Navigation>,
) -> Result<Vec<Entry>, ParseError> {
    let Some(navigation) = navigation else {
        return Ok(Vec::new());
    };
    let path = navigation.path.as_str();
    let source = archive.read(path)?;
    if !navigation.is_ncx {
        return read_html(&source, path);
    }
    let doc = xml(&source)?;
    let mut entries = Vec::new();
    for point in doc.descendants().filter(|n| n.has_tag_name("navPoint")) {
        let title = point
            .children()
            .find(|n| n.has_tag_name("navLabel"))
            .and_then(|n| n.descendants().find(|n| n.has_tag_name("text")))
            .and_then(|n| n.text())
            .unwrap_or("Untitled chapter")
            .to_owned();
        if let Some(href) = point
            .children()
            .find(|n| n.has_tag_name("content"))
            .and_then(|n| n.attribute("src"))
        {
            let target = resolve(path, href)?;
            let depth = point
                .ancestors()
                .skip(1)
                .filter(|n| n.has_tag_name("navPoint"))
                .count();
            entries.push(Entry {
                path: target.path,
                fragment: target.fragment,
                title,
                depth,
                kind: None,
                is_landmark: false,
            });
        }
    }
    Ok(entries)
}

fn read_html(source: &str, path: &str) -> Result<Vec<Entry>, ParseError> {
    let html = Html::parse_document(&crate::xhtml::expand_self_closing(source));
    let nav_selector = Selector::parse("nav").unwrap();
    let links = Selector::parse("a[href]").unwrap();
    let mut entries = Vec::new();
    for nav in html.select(&nav_selector).filter(|n| {
        n.value().attr("epub:type").is_some_and(|v| {
            v.split_whitespace()
                .any(|s| matches!(s, "toc" | "landmarks"))
        }) || n.value().attr("role") == Some("doc-toc")
    }) {
        for link in nav.select(&links) {
            let is_landmark = nav
                .value()
                .attr("epub:type")
                .is_some_and(|v| v.split_whitespace().any(|s| s == "landmarks"));
            let tokens = link.value().attr("epub:type").unwrap_or("");
            let kind = tokens
                .split_whitespace()
                .find_map(ExclusionReason::from_token);
            if is_landmark
                && kind.is_none()
                && !tokens.split_whitespace().any(|s| s == "bodymatter")
            {
                continue;
            }
            let href = link.value().attr("href").unwrap();
            let target = resolve(path, href)?;
            let depth = link
                .ancestors()
                .filter_map(ElementRef::wrap)
                .take_while(|e| e.id() != nav.id())
                .filter(|e| e.value().name() == "li")
                .count()
                .saturating_sub(1);
            let title = link
                .text()
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            entries.push(Entry {
                path: target.path,
                fragment: target.fragment,
                title,
                depth,
                kind,
                is_landmark,
            });
        }
    }
    entries.sort_by_key(|e| e.is_landmark);
    Ok(entries)
}
