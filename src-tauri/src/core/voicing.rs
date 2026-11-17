//! How a book is voiced: the voice, language and engine parameters every request for it uses.
//! Generation and the voice preview both come here, so a preview sounds like the book will.

use std::sync::Arc;

use aoide_text::{Role, Rule};
use aoide_tts::{
    resolve_params, supports_language, EngineDescriptor, Params, SynthesisRequest, Voice,
};

use super::BookSettings;

/// Picked when neither the book nor the profile names a voice and the engine has it.
const PREFERRED_VOICE: &str = "Emily.wav";

/// The voice, language and parameters for a book's requests.
#[derive(Debug, Clone, PartialEq)]
pub struct Voicing {
    pub voice_id: String,
    /// Primary language subtag the engine supports, lowercase; `None` lets the engine decide.
    pub language: Option<String>,
    pub params: Params,
    /// The book's own voice when the engine does not offer it (chosen with another engine).
    pub missing_voice: Option<String>,
    /// The voice for dialogue, when the book has one the engine offers.
    pub dialogue_voice_id: Option<String>,
    /// How words are said (the book's pronunciation list, then the one for all books); none
    /// until [`Voicing::with_rules`].
    pub rules: Arc<[Rule]>,
}

impl Voicing {
    /// Book settings win over the profile; the profile's voice wins over `fallback_voice`.
    /// With the engine's voice list (`offered`), voices it does not offer are passed over.
    pub fn resolve(
        descriptor: &EngineDescriptor,
        profile_params: &Params,
        profile_voice: Option<&str>,
        book: &BookSettings,
        book_language: Option<&str>,
        fallback_voice: Option<&str>,
        offered: Option<&[Voice]>,
    ) -> Result<Self, String> {
        let mut overrides = profile_params.clone();
        overrides.extend(book.params.clone());
        let params = resolve_params(descriptor, &overrides).map_err(|e| e.to_string())?;
        let named = |v: &&str| !v.trim().is_empty();
        let on_offer = |v: &&str| offered.map_or(true, |voices| voices.iter().any(|o| o.id == *v));
        let book_voice = book.voice_id.as_deref().filter(named);
        let voice_id = [book_voice, profile_voice, fallback_voice]
            .into_iter()
            .flatten()
            .filter(named)
            .find(on_offer)
            .ok_or("no voice selected")?
            .to_owned();
        let missing_voice = book_voice.filter(|v| !on_offer(v)).map(str::to_owned);
        let dialogue_voice_id = book
            .dialogue_voice_id
            .as_deref()
            .filter(named)
            .filter(on_offer)
            .map(str::to_owned);
        let language = book
            .language
            .as_deref()
            .or(book_language)
            .filter(|l| supports_language(descriptor, l))
            .map(|l| l.split(['-', '_']).next().unwrap_or(l).to_ascii_lowercase());
        Ok(Self {
            voice_id,
            language,
            params,
            missing_voice,
            dialogue_voice_id,
            rules: Arc::new([]),
        })
    }

    /// The same voicing, saying words as `rules` say.
    pub fn with_rules(self, rules: Arc<[Rule]>) -> Self {
        Self { rules, ..self }
    }

    /// The request for `text` with the rules applied, said for the `take`th time (0 for the
    /// first; see [`SynthesisRequest::take`]), in `role`: dialogue in the second voice, when the book has one.
    pub fn request_as(&self, text: String, take: u32, role: Role) -> SynthesisRequest {
        let voice_id = match (role, &self.dialogue_voice_id) {
            (Role::Dialogue, Some(voice)) => voice.clone(),
            _ => self.voice_id.clone(),
        };
        let text = if self.rules.is_empty() {
            text
        } else {
            aoide_text::pronounce(&text, &self.rules).text
        };
        SynthesisRequest {
            text,
            voice_id,
            language: self.language.clone(),
            params: self.params.clone(),
            take,
        }
    }
}

/// The voice to use when nothing names one: the preferred voice if the engine has it, else its first.
pub fn fallback_voice(voices: &[Voice]) -> Option<String> {
    voices
        .iter()
        .find(|v| v.id == PREFERRED_VOICE)
        .or_else(|| voices.first())
        .map(|v| v.id.clone())
}

#[cfg(test)]
mod tests {
    use aoide_tts::{ParamKind, ParamSpec, ParamValue};

    use super::*;

    fn descriptor(languages: Option<&[&str]>) -> EngineDescriptor {
        EngineDescriptor {
            kind: "fake".into(),
            name: "Fake".into(),
            adapter_version: 1,
            languages: languages.map(|l| l.iter().map(|s| s.to_string()).collect()),
            word_timings: false,
            max_chars: 300,
            cheap_voice_switch: true,
            params: vec![ParamSpec {
                key: "speed".into(),
                label: "Speed".into(),
                kind: ParamKind::Float {
                    min: 0.5,
                    max: 2.0,
                    step: 0.1,
                },
                default: ParamValue::Float(1.0),
            }],
        }
    }

    fn speed(value: f64) -> Params {
        Params::from([("speed".to_string(), ParamValue::Float(value))])
    }

    fn voice(id: &str) -> Voice {
        Voice {
            id: id.into(),
            display_name: id.into(),
        }
    }

    #[test]
    fn book_settings_win_over_the_profile() {
        let book = BookSettings {
            voice_id: Some("Book.wav".into()),
            params: speed(1.5),
            language: Some("pt-BR".into()),
            read_notes: false,
            narration: None,
            speak_missing: false,
            mode: None,
            left_out: vec![],
            playback_speed: None,
            shorten_silences: None,
            dialogue_voice_id: None,
        };
        let voicing = Voicing::resolve(
            &descriptor(None),
            &speed(0.8),
            Some("Profile.wav"),
            &book,
            Some("en"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            voicing,
            Voicing {
                voice_id: "Book.wav".into(),
                language: Some("pt".into()),
                params: speed(1.5),
                missing_voice: None,
                dialogue_voice_id: None,
                rules: Arc::new([]),
            }
        );
        assert_eq!(
            voicing
                .request_as("Hello.".into(), 0, Role::Narration)
                .voice_id,
            "Book.wav"
        );
    }

    #[test]
    fn falls_back_to_the_profile_voice_and_the_book_language() {
        let voicing = Voicing::resolve(
            &descriptor(Some(&["en"])),
            &speed(0.8),
            Some("Profile.wav"),
            &BookSettings::default(),
            Some("EN_gb"),
            Some("First.wav"),
            None,
        )
        .unwrap();
        assert_eq!(
            voicing,
            Voicing {
                voice_id: "Profile.wav".into(),
                language: Some("en".into()),
                params: speed(0.8),
                missing_voice: None,
                dialogue_voice_id: None,
                rules: Arc::new([]),
            }
        );
    }

    #[test]
    fn leaves_out_a_language_the_engine_does_not_speak_and_fails_without_a_voice() {
        let voicing = Voicing::resolve(
            &descriptor(Some(&["en"])),
            &Params::new(),
            None,
            &BookSettings::default(),
            Some("fr"),
            Some("First.wav"),
            None,
        )
        .unwrap();
        assert_eq!(
            (voicing.voice_id.as_str(), voicing.language, voicing.params),
            ("First.wav", None, speed(1.0))
        );
        assert!(Voicing::resolve(
            &descriptor(None),
            &Params::new(),
            Some("  "),
            &BookSettings::default(),
            None,
            None,
            None
        )
        .is_err());
    }

    #[test]
    fn passes_over_voices_the_engine_does_not_offer_and_names_the_book_one() {
        let offered = [voice("Hazel.wav"), voice("Arthur.wav")];
        let book = BookSettings {
            voice_id: Some("Abigail.wav".into()),
            ..BookSettings::default()
        };
        let resolve = |profile_voice| {
            Voicing::resolve(
                &descriptor(None),
                &Params::new(),
                profile_voice,
                &book,
                None,
                Some("Hazel.wav"),
                Some(&offered),
            )
            .unwrap()
        };
        let arthur = resolve(Some("Arthur.wav"));
        assert_eq!(arthur.voice_id, "Arthur.wav");
        assert_eq!(arthur.missing_voice.as_deref(), Some("Abigail.wav"));
        // A profile voice from another engine is passed over too.
        assert_eq!(resolve(Some("Emily.wav")).voice_id, "Hazel.wav");
    }

    #[test]
    fn rejects_parameters_outside_the_engine_range() {
        assert!(Voicing::resolve(
            &descriptor(None),
            &speed(9.0),
            Some("v"),
            &BookSettings::default(),
            None,
            None,
            None
        )
        .is_err());
    }

    #[test]
    fn the_fallback_voice_is_the_preferred_one_when_the_engine_has_it() {
        assert_eq!(
            fallback_voice(&[voice("A.wav"), voice("Emily.wav")]).as_deref(),
            Some("Emily.wav")
        );
        assert_eq!(
            fallback_voice(&[voice("A.wav"), voice("B.wav")]).as_deref(),
            Some("A.wav")
        );
        assert_eq!(fallback_voice(&[]), None);
    }
}

#[cfg(test)]
mod dialogue_tests {
    use crate::core::tests::{epub_of_chapters, library_folder_holding, start, until};
    use aoide_tts::testing::FakeEngine;
    use std::sync::Arc;

    const SPEECH: &str = "“I have walked this quay every evening for thirty years,” the harbour \
                          master said. She lit the last lamp. “And I will walk it again \
                          tomorrow night, and every night after that.”";

    /// The text of the sentence `id` of chapter `chapter`, as the reader shows it.
    fn sentence(core: &crate::core::Core, book: &str, chapter: usize, id: &str) -> String {
        core.chapter(book, chapter)
            .unwrap()
            .blocks
            .iter()
            .flat_map(|b| &b.segments)
            .find(|s| s.id == id)
            .map(|s| s.words.join(" "))
            .unwrap_or_default()
    }

    #[test]
    fn the_preview_says_a_line_of_narration_then_a_line_of_speech_from_the_book() {
        let bytes = epub_of_chapters(&[("The Quay", SPEECH)]);
        let book = crate::book::BookModel::parse("b", &bytes, &Default::default()).unwrap();
        assert_eq!(
            super::dialogue_sample(&book),
            Some((
                "the harbour master said. She lit the last lamp.".into(),
                "“I have walked this quay every evening for thirty years,”".into()
            ))
        );
        let plain = epub_of_chapters(&[("The Quay", "No one spoke at all that evening.")]);
        let book = crate::book::BookModel::parse("b", &plain, &Default::default()).unwrap();
        assert_eq!(super::dialogue_sample(&book), None);
    }

    #[tokio::test]
    async fn a_second_voice_for_dialogue_keeps_places_and_bookmarks_on_their_words() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = epub_of_chapters(&[("The Quay", SPEECH)]);
        let id = library_folder_holding(dir.path(), "", "voiceId = \"Test.wav\"", &bytes);
        let engine: Arc<FakeEngine> = FakeEngine::with_voices(&["Test.wav", "Other.wav"]);
        let (core, _) = start(dir.path(), &engine);
        until("the scan", || core.list_books().is_ok_and(|b| b.len() == 1)).await;

        // Read up to the last speech, bookmarked there.
        let last = core.chapter(&id, 0).unwrap().blocks[1]
            .segments
            .last()
            .unwrap()
            .id
            .clone();
        let words = sentence(&core, &id, 0, &last);
        assert!(words.starts_with("“And I will"), "{words}");
        core.set_reading_place(&id, 0, &last).unwrap();
        let mark = core.add_bookmark_at(&id, "", Some(&last)).unwrap();
        let narration_keys = core.speech_keys(&id);

        // A second voice: the speech is cut apart from the narration, and ids move.
        let mut settings = core.book_settings(&id);
        settings.dialogue_voice_id = Some("Other.wav".into());
        core.save_book_settings(&id, settings).await.unwrap();
        let view = core.open_book(&id).unwrap();
        let place = view.progress.unwrap().segment_id.unwrap();
        assert_ne!(place, last, "a new id for the same words");
        assert_eq!(sentence(&core, &id, 0, &place), words);
        let listed = core.bookmarks(&id).unwrap();
        let marked = listed.iter().find(|m| m.id == mark.id).unwrap();
        assert_eq!(
            sentence(&core, &id, 0, marked.segment_id.as_deref().unwrap()),
            words
        );
        // The saved place, bookmarked again, is the same words.
        let again = core.add_bookmark(&id, "").unwrap();
        assert_eq!(
            sentence(&core, &id, 0, again.segment_id.as_deref().unwrap()),
            words
        );

        // Each speech is asked for in the second voice; narration that was not cut keeps its key.
        let requests = core.speech_requests(&id);
        let voices: Vec<(&str, &str)> = requests
            .iter()
            .map(|(text, voice)| (text.as_str(), voice.as_str()))
            .collect();
        assert!(voices.contains(&(
            "“I have walked this quay every evening for thirty years,”",
            "Other.wav"
        )));
        assert!(voices.contains(&(
            "the harbour master said. She lit the last lamp.",
            "Test.wav"
        )));
        // An exported audiobook reads the same plan: both voices.
        let (_, exported) = crate::core::export::ExportHost::speech_plan(&*core, &id).unwrap();
        let exported: Vec<String> = exported
            .requests
            .into_iter()
            .flatten()
            .map(|r| r.voice_id)
            .collect();
        assert!(
            exported.iter().any(|v| v == "Other.wav") && exported.iter().any(|v| v == "Test.wav")
        );
        let heading = narration_keys[0].clone();
        assert!(
            core.speech_keys(&id).contains(&heading),
            "the heading is said as it was"
        );

        // Off again: the old segments, and their audio, come back.
        let mut settings = core.book_settings(&id);
        settings.dialogue_voice_id = None;
        core.save_book_settings(&id, settings).await.unwrap();
        assert_eq!(core.speech_keys(&id), narration_keys);
    }
}

#[cfg(test)]
impl crate::core::Core {
    /// Book `id`'s speech plan's cache keys, in order.
    pub fn speech_keys(&self, id: &str) -> Vec<String> {
        let book = self.load_book(id).unwrap();
        let plan = self.speech_plan(&book).unwrap();
        plan.items.into_iter().map(|item| item.key).collect()
    }

    /// Book `id`'s speech plan's requests, as text and voice.
    pub fn speech_requests(&self, id: &str) -> Vec<(String, String)> {
        let book = self.load_book(id).unwrap();
        let plan = self.speech_plan(&book).unwrap();
        plan.requests
            .into_iter()
            .flatten()
            .map(|r| (r.text, r.voice_id))
            .collect()
    }
}

/// A narration line and a quoted line from `book`, to hear a second voice for dialogue: the
/// first quote long enough to get its own voice, with the narration of its sentence (or the one
/// before).
pub fn dialogue_sample(book: &crate::book::BookModel) -> Option<(String, String)> {
    let mut before: Option<String> = None;
    for segment in book.chapters.iter().flat_map(|c| &c.segments) {
        let chars: Vec<char> = segment.display.chars().collect();
        let spans = aoide_text::quoted_spans(&chars);
        let quote = spans.iter().find(|s| s.len() >= aoide_text::SHORT_SPAN);
        if let Some(quote) = quote {
            let outside: String = chars
                .iter()
                .enumerate()
                .filter(|(i, _)| !spans.iter().any(|s| s.contains(i)))
                .map(|(_, c)| *c)
                .collect();
            let outside = outside.split_whitespace().collect::<Vec<_>>().join(" ");
            let narration = Some(outside)
                .filter(|o| o.chars().filter(|c| c.is_alphabetic()).count() >= 8)
                .or(before.clone())?;
            return Some((narration, chars[quote.clone()].iter().collect()));
        }
        if aoide_text::quoted_spans(&chars).is_empty()
            && segment.display.chars().any(char::is_alphabetic)
        {
            before = Some(segment.display.clone());
        }
    }
    None
}

impl super::Core {
    /// A narration sentence and a quoted one from book `id`, said in `voices` (the narrator's,
    /// then the dialogue's), as one WAV file; as [`super::engine::Engine::preview`] otherwise.
    pub async fn preview_dialogue(
        &self,
        id: &str,
        profile: aoide_tts::EngineProfile,
        voices: (Option<String>, String),
        params: aoide_tts::Params,
        language: Option<String>,
    ) -> Result<Vec<u8>, String> {
        let book = self.load_book(id)?;
        let (narration, quote) =
            dialogue_sample(&book).ok_or("This book has no dialogue in quotation marks.")?;
        if profile.launch.is_some() && profile.launch == self.engine.profile().launch {
            self.speech_server.want();
            self.wait_for_speech_server().await?;
        }
        let lines = [
            (narration, 400, Role::Narration),
            (quote, 0, Role::Dialogue),
        ];
        self.engine
            .say_roles(
                profile,
                (voices.0, Some(voices.1)),
                params,
                language,
                &lines,
            )
            .await
    }
}
