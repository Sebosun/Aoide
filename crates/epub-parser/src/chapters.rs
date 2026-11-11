use crate::{
    archive::Archive,
    boundaries::Boundaries,
    content::{self, Content},
    navigation::Entry,
    package::Package,
    Block, Chapter, ExclusionReason, Note, NoteRef, ParseError, SourceLocation, Warning,
};

pub(crate) struct ReadingSection {
    pub chapter: Chapter,
    pub reason: Option<ExclusionReason>,
    targets: Vec<(String, String)>,
}

pub(crate) struct Extraction {
    pub sections: Vec<ReadingSection>,
    pub warnings: Vec<Warning>,
    pub notes: Vec<Note>,
}

pub(crate) fn extract(
    archive: &mut Archive<'_>,
    package: &Package,
    navigation: &[Entry],
) -> Result<Extraction, ParseError> {
    let mut extraction = Extraction {
        sections: Vec::new(),
        warnings: Vec::new(),
        notes: Vec::new(),
    };
    if navigation.is_empty() {
        extraction.warnings.push(Warning::MissingNavigation);
    }
    for entry in navigation
        .iter()
        .chain(&package.guide)
        .filter(|e| !package.spine.contains(&e.path))
    {
        extraction.warnings.push(entry.missing_warning());
    }
    // Notes first, so a link anywhere in the book can be recognised as pointing at one.
    let mut note_ids = std::collections::HashSet::new();
    for path in &package.spine {
        let ids = content::note_ids(&archive.read(path)?);
        note_ids.extend(ids.into_iter().map(|id| format!("{path}#{id}")));
    }
    for path in &package.spine {
        let links_to_note = |href: &str| {
            crate::location::resolve(path, href).is_ok_and(|target| {
                target.fragment.is_some_and(|fragment| {
                    note_ids.contains(&format!("{}#{fragment}", target.path))
                })
            })
        };
        let content = content::read(&archive.read(path)?, &links_to_note);
        extraction
            .notes
            .extend(content.notes.iter().map(|note| Note {
                id: format!("{path}#{}", note.id),
                text: note.text.clone(),
            }));
        let mut boundaries = Boundaries::read(
            path,
            &content,
            navigation,
            &package.guide,
            &mut extraction.warnings,
        );
        if boundaries.starts.is_empty() {
            let (blocks, refs) = content.blocks_and_refs(0, content.tokens.len());
            let reason = content.reason(0, content.tokens.len());
            if !navigation.is_empty()
                && extraction
                    .sections
                    .last()
                    .is_some_and(|s| s.reason == reason)
            {
                append(extraction.sections.last_mut().unwrap(), path, blocks, refs);
                continue;
            }
            boundaries.starts.push((0, content.title.clone()));
        }
        preserve_prefix(
            &mut extraction.sections,
            path,
            &content,
            &mut boundaries,
            !navigation.is_empty(),
        );
        for (index, (offset, title)) in boundaries.starts.iter().enumerate() {
            let end = boundaries
                .starts
                .get(index + 1)
                .map_or(content.tokens.len(), |(offset, _)| *offset);
            let fragment = navigation
                .iter()
                .find(|e| e.path == *path && e.depth == 0 && e.offset(&content) == Some(*offset))
                .and_then(|e| e.fragment.clone());
            let (blocks, refs) = content.blocks_and_refs(*offset, end);
            let (note_refs, targets) = references(path, refs);
            extraction.sections.push(ReadingSection {
                chapter: Chapter {
                    title: title.clone(),
                    text: content::join_blocks(&blocks),
                    blocks,
                    note_refs,
                    sources: vec![SourceLocation {
                        path: path.clone(),
                        fragment,
                    }],
                },
                reason: boundaries.reason(&content, *offset, end),
                targets,
            });
        }
    }
    let mut referenced = vec![false; extraction.notes.len()];
    for section in &mut extraction.sections {
        for (reference, (path, href)) in section.chapter.note_refs.iter_mut().zip(&section.targets)
        {
            let target = crate::location::resolve(path, href)?;
            let id = format!("{}#{}", target.path, target.fragment.unwrap_or_default());
            reference.note = extraction.notes.iter().position(|note| note.id == id);
            if let Some(index) = reference.note {
                referenced[index] = true;
            } else {
                extraction.warnings.push(Warning::UnresolvedNoteRef {
                    path: path.clone(),
                    target: id,
                });
            }
        }
    }
    for (note, referenced) in extraction.notes.iter().zip(referenced) {
        if !referenced {
            extraction.warnings.push(Warning::UnreferencedNote {
                id: note.id.clone(),
            });
        }
    }
    Ok(extraction)
}

fn preserve_prefix(
    sections: &mut [ReadingSection],
    path: &str,
    content: &Content,
    boundaries: &mut Boundaries,
    has_navigation: bool,
) {
    let Some((offset, _)) = boundaries.starts.first() else {
        return;
    };
    let (blocks, refs) = content.blocks_and_refs(0, *offset);
    if blocks.is_empty() {
        return;
    }
    if has_navigation && content.reason(0, *offset).is_none() {
        if let Some(previous) = sections.last_mut().filter(|s| s.reason.is_none()) {
            append(previous, path, blocks, refs);
            return;
        }
    }
    boundaries.starts.insert(0, (0, "Untitled section".into()));
}

fn references(path: &str, refs: Vec<(NoteRef, String)>) -> (Vec<NoteRef>, Vec<(String, String)>) {
    refs.into_iter()
        .map(|(reference, href)| (reference, (path.to_owned(), href)))
        .unzip()
}

fn append(
    section: &mut ReadingSection,
    path: &str,
    blocks: Vec<Block>,
    refs: Vec<(NoteRef, String)>,
) {
    if blocks.is_empty() {
        return;
    }
    let (mut refs, targets) = references(path, refs);
    for reference in &mut refs {
        reference.block += section.chapter.blocks.len();
    }
    section.chapter.note_refs.extend(refs);
    section.targets.extend(targets);
    section.chapter.blocks.extend(blocks);
    section.chapter.text = content::join_blocks(&section.chapter.blocks);
    section.chapter.sources.push(SourceLocation {
        path: path.into(),
        fragment: None,
    });
}
