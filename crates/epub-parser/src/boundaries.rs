use crate::{content::Content, navigation::Entry, ExclusionReason, Warning};

pub(crate) struct Boundaries {
    pub starts: Vec<(usize, String)>,
    guide: Vec<(usize, Option<ExclusionReason>)>,
}

impl Boundaries {
    pub fn read(
        path: &str,
        content: &Content,
        navigation: &[Entry],
        guide: &[Entry],
        warnings: &mut Vec<Warning>,
    ) -> Self {
        let mut boundaries = Self {
            starts: Vec::new(),
            guide: Vec::new(),
        };
        for entry in navigation.iter().filter(|e| e.path == path) {
            if let Some(offset) = entry.offset(content) {
                if entry.depth == 0 {
                    boundaries.starts.push((offset, entry.title.clone()));
                }
            } else {
                warnings.push(entry.missing_warning());
            }
        }
        for entry in guide
            .iter()
            .chain(navigation.iter().filter(|e| e.is_landmark))
            .filter(|e| e.path == path)
        {
            if let Some(offset) = entry.offset(content) {
                boundaries.guide.push((offset, entry.kind));
                boundaries.insert(content, offset, entry.title.clone());
            } else if !warnings.contains(&entry.missing_warning()) {
                warnings.push(entry.missing_warning());
            }
        }
        boundaries.guide.sort_by_key(|(offset, _)| *offset);
        boundaries.add_semantic_sections(content);
        boundaries.infer_headings(path, content, navigation, warnings);
        boundaries.starts.sort_by_key(|(offset, _)| *offset);
        boundaries
            .starts
            .dedup_by(|a, b| equivalent(content, a.0, b.0));
        boundaries
    }

    pub fn reason(&self, content: &Content, start: usize, end: usize) -> Option<ExclusionReason> {
        if let Some(kind) = content.classification(start, end) {
            return kind;
        }
        self.guide
            .iter()
            .rev()
            .find(|(offset, _)| *offset <= start)
            .map(|(_, kind)| *kind)
            .unwrap_or_else(|| content.reason(start, end))
    }

    fn insert(&mut self, content: &Content, offset: usize, title: String) {
        if !self
            .starts
            .iter()
            .any(|(existing, _)| equivalent(content, *existing, offset))
        {
            self.starts.push((offset, title));
        }
    }

    fn add_semantic_sections(&mut self, content: &Content) {
        for section in &content.sections {
            let title = content
                .headings
                .iter()
                .find(|h| equivalent(content, section.start, h.offset))
                .map_or_else(
                    || {
                        section
                            .kind
                            .map_or_else(|| "Untitled section".into(), |kind| format!("{kind:?}"))
                    },
                    |h| h.title.clone(),
                );
            self.insert(content, section.start, title);
            if !content.text(section.end, content.tokens.len()).is_empty() {
                let title = content
                    .headings
                    .iter()
                    .find(|h| equivalent(content, section.end, h.offset))
                    .map_or_else(|| "Untitled section".into(), |h| h.title.clone());
                self.insert(content, section.end, title);
            }
        }
    }

    fn infer_headings(
        &mut self,
        path: &str,
        content: &Content,
        navigation: &[Entry],
        warnings: &mut Vec<Warning>,
    ) {
        let level = if navigation.is_empty() {
            content.headings.iter().map(|h| h.level).min()
        } else {
            content
                .headings
                .iter()
                .filter(|h| {
                    self.starts
                        .iter()
                        .any(|(offset, _)| equivalent(content, *offset, h.offset))
                })
                .map(|h| h.level)
                .min()
        };
        let known = navigation
            .iter()
            .filter(|e| e.path == path)
            .filter_map(|e| e.offset(content))
            .collect::<Vec<_>>();
        for heading in content.headings.iter().filter(|h| {
            Some(h.level) == level
                && !known
                    .iter()
                    .any(|offset| equivalent(content, *offset, h.offset))
        }) {
            self.starts.push((heading.offset, heading.title.clone()));
            warnings.push(Warning::InferredChapter {
                path: path.into(),
                title: heading.title.clone(),
            });
        }
    }
}

fn equivalent(content: &Content, left: usize, right: usize) -> bool {
    content.text(left.min(right), left.max(right)).is_empty()
}
