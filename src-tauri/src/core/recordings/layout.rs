//! A synced recording laid out for playback over the book's entries.

use super::*;

impl Recordings {
    /// The book's recording laid onto its entries, and how many of them are synced: once
    /// synced, by the stored sync map; while it syncs, by the transcript so far matched here,
    /// with the entries the sync has not safely reached left for playback to wait on. Laid out
    /// again only once the sync has moved on.
    pub(in crate::core) fn playable(
        &self,
        host: &dyn SyncHost,
        book: &BookModel,
    ) -> Result<(Arc<RecordingPlan>, usize), String> {
        let key = {
            let state = lock(&self.state);
            let live = state
                .live
                .get(&book.id)
                .ok_or("the book has no recording")?;
            Playable {
                book: book.id.clone(),
                entries: book.total_entries(),
                changes: live.changes,
            }
        };
        if let Some((kept, plan, synced)) = lock(&self.playable).as_ref() {
            if *kept == key {
                return Ok((Arc::clone(plan), *synced));
            }
        }
        let (plan, synced) = self.lay_out(host, book)?;
        let plan = Arc::new(plan);
        *lock(&self.playable) = Some((key, Arc::clone(&plan), synced));
        Ok((plan, synced))
    }

    pub(super) fn lay_out(
        &self,
        host: &dyn SyncHost,
        book: &BookModel,
    ) -> Result<(RecordingPlan, usize), String> {
        let book_id = BookId(book.id.clone());
        let (row, stored) = self.stored_recording(&book.id)?;
        let blocks = lock(&self.library)
            .transcript_blocks(&book_id)
            .map_err(|e| e.to_string())?;
        let (map, edge) = if self.recording_stage(&book.id) == Some(SyncStage::Ready) {
            let map = lock(&self.library)
                .sync_map(&book_id)
                .map_err(|e| e.to_string())?
                .ok_or("the recording is not matched yet")?;
            if map.text_hash != text_hash(book) {
                self.sync_event(host, &book.id, SyncEvent::TextChanged, None);
                self.wake_sync();
                return Err("the book's text changed: its recording is being matched again".into());
            }
            let chapters = decode_chapters(&map.map).ok_or("the stored sync map is damaged")?;
            let report: Report = serde_json::from_str(&map.report).map_err(|e| e.to_string())?;
            (SyncMap { chapters, report }, None)
        } else {
            let texts: Vec<&str> = book.chapters.iter().map(|c| c.text.as_str()).collect();
            let edge = blocks.last().map_or(0, |b| b.end_ms);
            (match_book(&texts, &self.heard_words(&book.id)?), Some(edge))
        };
        let mut levels = Vec::new();
        for block in blocks {
            levels.extend(block.levels);
        }
        let loudness = Loudness {
            frame_ms: FRAME_MS,
            levels,
        };
        let units: Vec<Unit> = book
            .chapters
            .iter()
            .enumerate()
            .flat_map(|(chapter, model)| {
                model.segments.iter().map(move |segment| Unit {
                    chapter,
                    range: segment.range.clone(),
                    words: segment.words.clone(),
                })
            })
            .collect();
        let clips = clips(&map, &units, edge.unwrap_or(row.total_ms), Some(&loudness));
        // While it syncs: the units after the last one that ends safely before the sync's edge
        // wait for it (the matcher may still move what lies near the edge).
        let synced = edge.map_or(clips.len(), |edge| {
            let safe = edge.saturating_sub(SYNC_MARGIN_MS);
            clips
                .iter()
                .rposition(|clip| !clip.audio.is_empty() && clip.audio.end <= safe)
                .map_or(0, |last| last + 1)
        });
        let recording = RecordingPlan {
            files: stored.files.iter().map(StoredFile::audio).collect(),
            clips,
        };
        Ok((recording, synced))
    }
}
