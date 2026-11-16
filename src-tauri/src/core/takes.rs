//! Saying one sentence again: its audio is thrown away and made again as a new take, from the
//! playhead put on it. The take is counted in the library (`segment_takes`) and goes into the
//! request, so the engine is asked for a different reading and the audio gets a new key.

use std::collections::HashSet;
use std::time::Instant;

use aoide_library::BookId;

use super::item::Needs;
use super::{lock, Core, PlanEvent};

/// Why a sentence cannot be said again.
const NOT_SPOKEN: &str = "this sentence is not said by the speech engine";

impl Core {
    /// Whether book `id`'s sentences can be said again: the speech engine reads it, not its
    /// recording (whose clips cannot be made again, and what it lacks goes with it).
    pub(super) fn says_again(&self, id: &str) -> bool {
        !self.plays_recording(id, &self.book_settings(id))
    }

    /// Throws away the audio of sentence `segment_id` of book `id` and makes it again, as a
    /// new take, from the playhead put on the sentence (playing or paused as before). A book
    /// that is not being listened to is listened to from that sentence.
    pub async fn say_again(&self, id: &str, segment_id: &str) -> Result<(), String> {
        let started = Instant::now();
        let result = self.retake(id, segment_id).await;
        let book = crate::logging::book(id);
        match &result {
            Ok(take) => tracing::info!(
                event = "speech.say_again",
                book,
                segment = segment_id,
                take,
                outcome = "ok",
                took_ms = started.elapsed().as_millis() as u64,
                "Saying a sentence again"
            ),
            Err(error) => tracing::warn!(
                event = "speech.say_again",
                book,
                segment = segment_id,
                outcome = "failed",
                %error,
                "Cannot say the sentence again"
            ),
        }
        result.map(drop)
    }

    /// [`Core::say_again`]; returns the new take.
    async fn retake(&self, id: &str, segment_id: &str) -> Result<u32, String> {
        self.require(id, Needs::Text)?;
        let book = self.load_book(id)?;
        let index = book.index_of(segment_id).ok_or("no such sentence")?;
        let silent = book.segment(index).map_or(true, |s| s.speech.is_empty());
        if silent || !self.says_again(id) {
            return Err(NOT_SPOKEN.into());
        }
        if !self.playback.is_book(id) {
            self.play(id, None, Some(segment_id.into())).await?;
        }
        let (request, old_key) = self
            .playback
            .with_narration(|playing, narration| {
                (playing == id)
                    .then(|| narration.speech_at(index))
                    .flatten()
                    .map(|(request, key)| (request.clone(), key.to_string()))
            })
            .flatten()
            .ok_or(NOT_SPOKEN)?;
        let speech = self
            .engine
            .speech(&self.book_settings(id), book.language.as_deref())?;
        let take = lock(&self.library)
            .count_take(&BookId(id.into()), segment_id, &request.text)
            .map_err(|e| e.to_string())?;
        let request = aoide_tts::SynthesisRequest { take, ..request };
        let key = aoide_tts::cache_key(&speech.descriptor, speech.model.as_deref(), &request);
        // As for a whole book made again: the worker forgets the key before its audio goes.
        self.generation.forget(vec![old_key.clone()]).await;
        self.storage.delete_segment(&old_key)?;
        // Another entry saying the same words loses its audio too.
        self.playback.audio_lost(&HashSet::from([old_key.clone()]));
        if !self.playback.retake(id, index, request, key) {
            return Err(NOT_SPOKEN.into());
        }
        self.plan(PlanEvent::SaidAgain {
            book: id.into(),
            key: old_key,
        });
        self.follow_through(self.playback.seek(index, None))?;
        Ok(take)
    }
}
