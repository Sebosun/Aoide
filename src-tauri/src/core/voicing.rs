//! How a book is voiced: the voice, language and engine parameters every request for it uses.
//! Generation and the voice preview both come here, so a preview sounds like the book will.

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
}

impl Voicing {
    /// Book settings win over the profile; the profile's voice wins over `fallback_voice`.
    pub fn resolve(
        descriptor: &EngineDescriptor,
        profile_params: &Params,
        profile_voice: Option<&str>,
        book: &BookSettings,
        book_language: Option<&str>,
        fallback_voice: Option<&str>,
    ) -> Result<Self, String> {
        let mut overrides = profile_params.clone();
        overrides.extend(book.params.clone());
        let params = resolve_params(descriptor, &overrides).map_err(|e| e.to_string())?;
        let named = |v: &&str| !v.trim().is_empty();
        let voice_id = book
            .voice_id
            .as_deref()
            .filter(named)
            .or(profile_voice.filter(named))
            .or(fallback_voice.filter(named))
            .ok_or("no voice selected")?
            .to_owned();
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
        })
    }

    pub fn request(&self, text: String) -> SynthesisRequest {
        SynthesisRequest {
            text,
            voice_id: self.voice_id.clone(),
            language: self.language.clone(),
            params: self.params.clone(),
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
        };
        let voicing = Voicing::resolve(
            &descriptor(None),
            &speed(0.8),
            Some("Profile.wav"),
            &book,
            Some("en"),
            None,
        )
        .unwrap();
        assert_eq!(
            voicing,
            Voicing {
                voice_id: "Book.wav".into(),
                language: Some("pt".into()),
                params: speed(1.5)
            }
        );
        assert_eq!(voicing.request("Hello.".into()).voice_id, "Book.wav");
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
        )
        .unwrap();
        assert_eq!(
            voicing,
            Voicing {
                voice_id: "Profile.wav".into(),
                language: Some("en".into()),
                params: speed(0.8)
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
            None
        )
        .is_err());
    }

    #[test]
    fn rejects_parameters_outside_the_engine_range() {
        assert!(Voicing::resolve(
            &descriptor(None),
            &speed(9.0),
            Some("v"),
            &BookSettings::default(),
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
