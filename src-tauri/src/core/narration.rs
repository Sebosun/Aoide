//! What each entry of the book being listened to is narrated with: the speech engine, its clip
//! of the book's recording, or nothing. Every entry has one explicit [`Fate`], changed only by
//! [`next`]. The player's entries, the audio to give the player, the speech to make and what
//! the views show are all read from the fates, never worked out again by their readers.

use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

use aoide_audio::Entry as PlayerEntry;
use aoide_generation::{estimate_ms, PlanItem};
use aoide_tts::SynthesisRequest;

use super::preparation::Plan;
use super::recording::RecordingPlan;
use crate::book::BookModel;

/// How one entry is narrated.
#[derive(Debug, Clone, PartialEq)]
pub enum Fate {
    /// Nothing to say: only the pause after it plays.
    Silent,
    /// Said by the speech engine (a book read by one, or text its recording lacks); `made` once
    /// its audio is in the cache.
    Speech {
        request: SynthesisRequest,
        made: bool,
    },
    /// Its clip of the book's recording.
    Clip,
    /// The recording has no audio for it: the player passes it over.
    Unrecorded,
    /// In the part of the recording the sync has not safely reached yet.
    AwaitingSync,
}

/// What can happen to an entry.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Its speech is made.
    Made,
    /// Its speech was deleted and has to be made again.
    Lost,
    /// The user asked for it to be said again: it is said by this request (a new take), not
    /// made yet.
    Retaken(SynthesisRequest),
    /// The recording's sync went further; this is how the entry is narrated now.
    Resynced(Fate),
}

/// The whole state machine. Events that do not apply leave the fate as it is.
pub fn next(fate: Fate, event: Event) -> Fate {
    match (fate, event) {
        (Fate::Speech { request, .. }, Event::Made) => Fate::Speech {
            request,
            made: true,
        },
        (Fate::Speech { request, .. }, Event::Lost) => Fate::Speech {
            request,
            made: false,
        },
        (Fate::Speech { .. }, Event::Retaken(request)) => Fate::Speech {
            request,
            made: false,
        },
        (_, Event::Resynced(fate)) => fate,
        (fate, _) => fate,
    }
}

#[derive(Debug, Clone)]
struct Entry {
    fate: Fate,
    /// The speech's cache key, or a name for the entry.
    key: String,
    /// Its audio's length: exact once made (a clip always is), an estimate before.
    duration_ms: u32,
    pause_after_ms: u32,
}

impl Entry {
    fn has_audio(&self) -> bool {
        matches!(self.fate, Fate::Clip | Fate::Speech { made: true, .. })
    }
}

/// What following the sync changed for playback.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Followed {
    /// Entries that waited for the sync and turned out not to be in the recording.
    pub passed_over: Vec<usize>,
    /// Which entries the speech engine says changed: its plan has to be made again.
    pub speech_changed: bool,
}

/// How every entry of a book is narrated, and the recording that narrates it, if one does.
pub struct NarrationPlan {
    entries: Vec<Entry>,
    recording: Option<Arc<RecordingPlan>>,
}

impl NarrationPlan {
    /// A book read by the speech engine, from its generation plan.
    pub fn speech(book: &BookModel, plan: Plan) -> Self {
        let entries = plan
            .items
            .into_iter()
            .zip(plan.requests)
            .enumerate()
            .map(|(index, (item, request))| Entry {
                fate: match request {
                    Some(request) => Fate::Speech {
                        request,
                        made: item.cached,
                    },
                    None => Fate::Silent,
                },
                key: item.key,
                duration_ms: item.duration_ms,
                pause_after_ms: pause_after(book, index),
            })
            .collect();
        Self {
            entries,
            recording: None,
        }
    }

    /// A book read from its recording, synced up to entry `synced`. The entries after it wait
    /// for the sync. Text the synced part lacks (see [`aoide_sync::lacking`]) is said by the
    /// speech engine when `speech` gives a plan for the book; it is asked only when something
    /// is lacking.
    pub fn recording(
        book: &BookModel,
        recording: Arc<RecordingPlan>,
        synced: usize,
        speech: impl FnOnce() -> Option<Plan>,
    ) -> Self {
        let synced = synced.min(recording.clips.len());
        let mut entries: Vec<Entry> = recording
            .clips
            .iter()
            .enumerate()
            .map(|(index, clip)| Entry {
                fate: if index >= synced {
                    Fate::AwaitingSync
                } else if clip.audio.is_empty() {
                    Fate::Unrecorded
                } else {
                    Fate::Clip
                },
                key: format!("clip:{index}"),
                duration_ms: (clip.audio.end - clip.audio.start) as u32,
                // The clips run on into each other: their pauses are in the recording.
                pause_after_ms: 0,
            })
            .collect();
        let speaking: Vec<u64> = (0..synced)
            .map(|i| {
                book.segment(i)
                    .map_or(0, |s| u64::from(estimate_ms(s.speech.chars().count())))
            })
            .collect();
        let lacking = aoide_sync::lacking(&recording.clips[..synced], &speaking);
        if let Some(plan) = lacking.contains(&true).then(speech).flatten() {
            let spoken = plan.items.into_iter().zip(plan.requests).enumerate();
            for (index, (item, request)) in spoken {
                let (true, Some(request)) = (lacking.get(index) == Some(&true), request) else {
                    continue;
                };
                entries[index] = Entry {
                    fate: Fate::Speech {
                        request,
                        made: item.cached,
                    },
                    key: item.key,
                    duration_ms: item.duration_ms,
                    pause_after_ms: pause_after(book, index),
                };
            }
        }
        Self {
            entries,
            recording: Some(recording),
        }
    }

    pub fn recording_plan(&self) -> Option<&Arc<RecordingPlan>> {
        self.recording.as_ref()
    }

    pub fn fate(&self, index: usize) -> Option<&Fate> {
        self.entries.get(index).map(|e| &e.fate)
    }

    /// The player's playlist.
    pub fn player_entries(&self, book: &BookModel) -> Vec<PlayerEntry> {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| PlayerEntry {
                id: book
                    .segment(index)
                    .map(|s| s.id.clone())
                    .unwrap_or_default(),
                speakable: !matches!(entry.fate, Fate::Silent | Fate::Unrecorded),
                pause_after_ms: entry.pause_after_ms,
            })
            .collect()
    }

    /// Whether the speech engine says any entry.
    pub fn speaks(&self) -> bool {
        self.entries
            .iter()
            .any(|e| matches!(e.fate, Fate::Speech { .. }))
    }

    /// The entries as the speech engine's planner sees them: what it does not say counts as
    /// made, so it is never asked for but its length still counts towards what lies ahead.
    pub fn speech_items(&self) -> Vec<PlanItem> {
        self.entries
            .iter()
            .map(|entry| PlanItem {
                key: entry.key.clone(),
                speakable: !matches!(entry.fate, Fate::Silent | Fate::Unrecorded),
                cached: !matches!(entry.fate, Fate::Speech { made: false, .. } | Fate::Silent),
                duration_ms: entry.duration_ms,
            })
            .collect()
    }

    /// What the speech engine is asked to say for `index`.
    pub fn request(&self, index: usize) -> Option<&SynthesisRequest> {
        match &self.entries.get(index)?.fate {
            Fate::Speech { request, .. } => Some(request),
            _ => None,
        }
    }

    /// What the speech engine is asked to say for `index`, with its cache key.
    pub fn speech_at(&self, index: usize) -> Option<(&SynthesisRequest, &str)> {
        let entry = self.entries.get(index)?;
        match &entry.fate {
            Fate::Speech { request, .. } => Some((request, entry.key.as_str())),
            _ => None,
        }
    }

    /// Entry `index` is said again by `request` (a new take), its audio under `key` once made.
    /// Returns false when the speech engine does not say that entry.
    pub fn retake(&mut self, index: usize, request: SynthesisRequest, key: String) -> bool {
        let Some(entry) = self.entries.get_mut(index) else {
            return false;
        };
        if !matches!(entry.fate, Fate::Speech { .. }) {
            return false;
        }
        entry.duration_ms = estimate_ms(request.text.chars().count());
        entry.fate = next(entry.fate.clone(), Event::Retaken(request));
        entry.key = key;
        true
    }

    /// Entries in `range` whose speech is made, with their cache keys.
    pub fn made_speech(&self, range: Range<usize>) -> Vec<(usize, String)> {
        self.within(range)
            .filter(|(_, e)| matches!(e.fate, Fate::Speech { made: true, .. }))
            .map(|(i, e)| (i, e.key.clone()))
            .collect()
    }

    /// Entries in `range` played from their clip of the recording.
    pub fn clips(&self, range: Range<usize>) -> Vec<usize> {
        self.within(range)
            .filter(|(_, e)| e.fate == Fate::Clip)
            .map(|(i, _)| i)
            .collect()
    }

    /// Entries in `range` the recording has no audio for.
    pub fn unrecorded(&self, range: Range<usize>) -> Vec<usize> {
        self.within(range)
            .filter(|(_, e)| e.fate == Fate::Unrecorded)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn waits_for_sync(&self, index: usize) -> bool {
        self.fate(index) == Some(&Fate::AwaitingSync)
    }

    /// The length of `index`'s audio, when it has audio to play.
    pub fn audio_ms(&self, index: usize) -> Option<u32> {
        let entry = self.entries.get(index)?;
        entry.has_audio().then_some(entry.duration_ms)
    }

    /// How long `range` plays for, pauses included; `None` while part of it waits for the
    /// sync. Speech not made yet counts at its estimated length.
    pub fn playing_ms(&self, range: Range<usize>) -> Option<u64> {
        let mut total = 0u64;
        for (_, entry) in self.within(range) {
            match entry.fate {
                Fate::AwaitingSync => return None,
                Fate::Unrecorded => {}
                Fate::Silent => total += u64::from(entry.pause_after_ms),
                Fate::Clip | Fate::Speech { .. } => {
                    total += u64::from(entry.duration_ms) + u64::from(entry.pause_after_ms);
                }
            }
        }
        Some(total)
    }

    /// The speech with cache key `key` is made, `duration_ms` long.
    pub fn segment_made(&mut self, key: &str, duration_ms: u32) {
        for entry in self.entries.iter_mut().filter(|e| e.key == key) {
            entry.fate = next(entry.fate.clone(), Event::Made);
            if entry.has_audio() {
                entry.duration_ms = duration_ms;
            }
        }
    }

    /// The speech with these cache keys was deleted.
    pub fn audio_lost(&mut self, keys: &HashSet<String>) {
        for entry in self.entries.iter_mut().filter(|e| keys.contains(&e.key)) {
            entry.fate = next(entry.fate.clone(), Event::Lost);
        }
    }

    /// The recording's sync went further: plays by `newer` from now on.
    pub fn follow(&mut self, newer: NarrationPlan) -> Followed {
        let mut followed = Followed::default();
        let speech_before = self.speech_keys();
        for (index, (entry, newer)) in self.entries.iter_mut().zip(newer.entries).enumerate() {
            if entry.fate == Fate::AwaitingSync && newer.fate == Fate::Unrecorded {
                followed.passed_over.push(index);
            }
            *entry = Entry {
                fate: next(entry.fate.clone(), Event::Resynced(newer.fate)),
                ..newer
            };
        }
        self.recording = newer.recording;
        followed.speech_changed = self.speech_keys() != speech_before;
        followed
    }

    fn speech_keys(&self) -> HashSet<(usize, String)> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e.fate, Fate::Speech { .. }))
            .map(|(i, e)| (i, e.key.clone()))
            .collect()
    }

    fn within(&self, range: Range<usize>) -> impl Iterator<Item = (usize, &Entry)> {
        let end = range.end.min(self.entries.len());
        let start = range.start.min(end);
        self.entries[start..end]
            .iter()
            .enumerate()
            .map(move |(i, e)| (start + i, e))
    }
}

fn pause_after(book: &BookModel, index: usize) -> u32 {
    book.segment(index).map_or(0, |s| s.pause_after_ms)
}

/// The ids of the segments of `book` at `entries` that have something to say.
pub(super) fn spoken_ids(book: &BookModel, entries: Vec<usize>) -> Vec<String> {
    entries
        .into_iter()
        .filter_map(|index| book.segment(index))
        .filter(|segment| !segment.speech.is_empty())
        .map(|segment| segment.id.clone())
        .collect()
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use aoide_sync::Clip;

    use super::*;

    /// The test book: four entries (heading, paragraph, heading, paragraph).
    fn book() -> BookModel {
        BookModel::parse("b", &crate::core::tests::epub(), false).unwrap()
    }

    /// A speech plan for the book with entry `silent` having nothing to say and only the entries
    /// in `made` made.
    fn speech_plan(book: &BookModel, silent: Option<usize>, made: &[usize]) -> Plan {
        let requests: Vec<Option<SynthesisRequest>> = (0..book.total_entries())
            .map(|i| {
                (Some(i) != silent).then(|| SynthesisRequest {
                    text: format!("entry {i}"),
                    voice_id: "v".into(),
                    language: None,
                    params: Default::default(),
                    take: 0,
                })
            })
            .collect();
        let items = requests
            .iter()
            .enumerate()
            .map(|(i, request)| PlanItem {
                key: format!("k{i}"),
                speakable: request.is_some(),
                cached: made.contains(&i),
                duration_ms: 2_000,
            })
            .collect();
        Plan { items, requests }
    }

    /// A clip spoken over `audio` (empty: not in the recording), with `before` of narration
    /// that matched no text ahead of its words.
    fn clip(audio: Range<u64>, before: u64) -> Clip {
        let text = (!audio.is_empty()).then(|| audio.start + before..audio.end);
        Clip {
            off_text: if before > 0 {
                vec![audio.start..audio.start + before]
            } else {
                vec![]
            },
            audio,
            text,
            words: vec![],
        }
    }

    fn recording(clips: Vec<Clip>) -> Arc<RecordingPlan> {
        Arc::new(RecordingPlan {
            files: vec![],
            clips,
        })
    }

    fn speakable(narration: &NarrationPlan, book: &BookModel) -> Vec<bool> {
        narration
            .player_entries(book)
            .iter()
            .map(|e| e.speakable)
            .collect()
    }

    #[test]
    fn a_book_without_a_recording_is_said_by_the_speech_engine_as_its_audio_is_made() {
        let book = book();
        let mut n = NarrationPlan::speech(&book, speech_plan(&book, Some(1), &[0, 3]));
        assert_eq!(speakable(&n, &book), [true, false, true, true]);
        assert!(n.speaks());
        assert!(n.recording_plan().is_none());
        assert_eq!(n.made_speech(0..4), [(0, "k0".into()), (3, "k3".into())]);
        assert!(n.clips(0..4).is_empty());
        assert_eq!(n.request(1), None);
        assert_eq!(n.request(2).unwrap().text, "entry 2");
        let wanted: Vec<bool> = n
            .speech_items()
            .iter()
            .map(|i| i.speakable && !i.cached)
            .collect();
        assert_eq!(wanted, [false, false, true, false]);

        n.segment_made("k2", 1_500);
        assert_eq!(n.audio_ms(2), Some(1_500));
        n.audio_lost(&HashSet::from(["k0".to_string()]));
        assert_eq!(n.audio_ms(0), None);
        assert_eq!(
            n.made_speech(0..4),
            [(2, "k2".into()), (3, "k3".into())],
            "deleted audio is not played until it is made again"
        );
    }

    #[test]
    fn a_recording_plays_its_clips_passes_over_what_it_lacks_and_waits_for_the_sync() {
        let book = book();
        let clips = vec![
            clip(0..1_000, 0),
            clip(1_000..1_000, 0),
            clip(1_000..3_000, 0),
            clip(3_000..3_000, 0),
        ];
        let mut asked = false;
        let n = NarrationPlan::recording(&book, recording(clips), 3, || {
            asked = true;
            None
        });
        assert!(asked, "entry 1 lacks narration, so speech was asked for");
        assert_eq!(n.fate(0), Some(&Fate::Clip));
        assert_eq!(n.fate(1), Some(&Fate::Unrecorded));
        assert_eq!(n.fate(3), Some(&Fate::AwaitingSync));
        assert_eq!(speakable(&n, &book), [true, false, true, true]);
        assert!(n
            .player_entries(&book)
            .iter()
            .all(|e| e.pause_after_ms == 0));
        assert!(!n.speaks());
        assert!(n.speech_items().iter().all(|i| !i.speakable || i.cached));
        assert_eq!(n.clips(0..4), [0, 2]);
        assert_eq!(n.unrecorded(0..4), [1]);
        assert!(n.waits_for_sync(3) && !n.waits_for_sync(2));
        assert_eq!(n.audio_ms(1), None);
        assert_eq!(n.playing_ms(0..3), Some(3_000));
        assert_eq!(n.playing_ms(1..4), None, "entry 3 waits for the sync");
    }

    #[test]
    fn what_a_recording_lacks_is_said_by_the_speech_engine_when_given_a_plan() {
        let book = book();
        let clips = vec![
            clip(0..1_000, 0),
            clip(1_000..1_000, 0),
            clip(1_000..3_000, 0),
            clip(3_000..4_000, 0),
        ];
        let plan = speech_plan(&book, None, &[]);
        let n = NarrationPlan::recording(&book, recording(clips), 4, || Some(plan));
        assert!(matches!(n.fate(1), Some(Fate::Speech { made: false, .. })));
        assert_eq!(n.fate(2), Some(&Fate::Clip));
        assert!(n.speaks());
        assert_eq!(speakable(&n, &book), [true, true, true, true]);
        assert_eq!(
            n.player_entries(&book)[1].pause_after_ms,
            book.segment(1).unwrap().pause_after_ms
        );
        let wanted: Vec<usize> = n
            .speech_items()
            .iter()
            .enumerate()
            .filter(|(_, i)| i.speakable && !i.cached)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(wanted, [1]);
        assert_eq!(n.request(1).unwrap().text, "entry 1");
        assert!(n.unrecorded(0..4).is_empty());
        assert_eq!(
            n.playing_ms(0..4).unwrap(),
            4_000 + 2_000 + u64::from(book.segment(1).unwrap().pause_after_ms),
            "speech not made yet counts at its estimate"
        );
    }

    #[test]
    fn text_with_narration_beside_it_is_passed_over_even_when_speech_is_given() {
        let book = book();
        // Entry 2 begins with a minute of narration that matched no text: entry 1 was most
        // likely read there.
        let clips = vec![
            clip(0..1_000, 0),
            clip(1_000..1_000, 0),
            clip(1_000..63_000, 60_000),
            clip(63_000..64_000, 0),
        ];
        let plan = speech_plan(&book, None, &[]);
        let n = NarrationPlan::recording(&book, recording(clips), 4, || Some(plan));
        assert_eq!(n.fate(1), Some(&Fate::Unrecorded));
        assert!(!n.speaks());
    }

    #[test]
    fn following_the_sync_passes_over_what_it_lacks_and_reports_new_speech() {
        let book = book();
        let clips = vec![
            clip(0..1_000, 0),
            clip(1_000..1_000, 0),
            clip(1_000..3_000, 0),
            clip(3_000..3_000, 0),
        ];
        let mut n = NarrationPlan::recording(&book, recording(clips.clone()), 1, || None);
        assert!((1..4).all(|i| n.waits_for_sync(i)));

        // Synced: entry 1 is lacking and said, entry 3 is simply not in the recording (two
        // seconds of unmatched narration follow the last words).
        let mut synced = clips;
        synced[2].off_text = vec![3_000..5_000];
        synced[2].audio.end = 5_000;
        let plan = speech_plan(&book, None, &[]);
        let newer = NarrationPlan::recording(&book, recording(synced), 4, || Some(plan));
        let followed = n.follow(newer);
        assert_eq!(
            followed,
            Followed {
                passed_over: vec![3],
                speech_changed: true
            }
        );
        assert!(matches!(n.fate(1), Some(Fate::Speech { .. })));
        assert!(!n.waits_for_sync(1));
        assert_eq!(n.recording_plan().unwrap().clips[2].audio, 1_000..5_000);

        let again = NarrationPlan::recording(
            &book,
            recording(n.recording_plan().unwrap().clips.clone()),
            4,
            || Some(speech_plan(&book, None, &[])),
        );
        assert_eq!(n.follow(again), Followed::default(), "nothing changed");
    }

    #[test]
    fn the_state_machine_changes_only_speech_except_when_resynced() {
        let request = SynthesisRequest {
            text: "t".into(),
            voice_id: "v".into(),
            language: None,
            params: Default::default(),
            take: 0,
        };
        let unmade = Fate::Speech {
            request: request.clone(),
            made: false,
        };
        let made = Fate::Speech {
            request: request.clone(),
            made: true,
        };
        let take = SynthesisRequest { take: 1, ..request };
        let retaken = Fate::Speech {
            request: take.clone(),
            made: false,
        };
        assert_eq!(next(made.clone(), Event::Retaken(take.clone())), retaken);
        assert_eq!(next(unmade.clone(), Event::Retaken(take.clone())), retaken);
        assert_eq!(next(Fate::Clip, Event::Retaken(take.clone())), Fate::Clip);
        assert_eq!(next(Fate::Silent, Event::Retaken(take)), Fate::Silent);
        assert_eq!(next(unmade.clone(), Event::Made), made);
        assert_eq!(next(made.clone(), Event::Lost), unmade);
        assert_eq!(next(Fate::Clip, Event::Made), Fate::Clip);
        assert_eq!(next(Fate::AwaitingSync, Event::Lost), Fate::AwaitingSync);
        assert_eq!(
            next(Fate::AwaitingSync, Event::Resynced(Fate::Unrecorded)),
            Fate::Unrecorded
        );
    }
}
