//! The speech engine in use: its profile, what it has said about its voices and model, and its
//! status as the window shows it, with the state of the server Aoide starts for it folded in
//! (the server itself is [`super::engine_process`]'s).
//!
//! A component: its state is behind its own locks, each method computes under them and returns
//! a value, and nothing outside reaches the state but through these methods. It tells the window
//! about status changes itself; what follows from them (planning again once voices are known) is
//! the caller's, from [`Engine::poll`]'s answer.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aoide_tts::{resolve_params, EngineProfile, EngineStatus, Params, Registry, SpeechEngine};

use super::engine_process::{Answer, FailureKind, ServerState, ServerView, StoppedKind};
use super::voicing::{fallback_voice, Voicing};
use super::{lock, BookSettings, CoreEvent, EngineOptions, EngineState, EngineStatusView, Events};
use aoide_text::Role;

/// Appends `audio` to `said` (at `said`'s rate, the first audio's), then `pause_ms` of silence.
fn follow_with(said: &mut aoide_tts::Synthesis, audio: aoide_tts::Synthesis, pause_ms: u32) {
    if said.sample_rate == 0 {
        said.sample_rate = audio.sample_rate;
    }
    let rate = said.sample_rate;
    let samples = if audio.sample_rate == rate {
        audio.samples
    } else {
        let pcm = aoide_audio::Pcm {
            sample_rate: audio.sample_rate,
            samples: audio.samples,
        };
        aoide_audio::resample(&pcm, rate).samples
    };
    said.samples.extend(samples);
    let silence = (u64::from(pause_ms) * u64::from(rate) / 1000) as usize;
    said.samples.resize(said.samples.len() + silence, 0.0);
}

/// Spoken by a voice preview.
const PREVIEW_TEXT: &str =
    "The lantern swung gently as the little boat drifted home across the quiet harbour.";

/// The engine in use and what is known about it.
struct Active {
    profile: EngineProfile,
    engine: Arc<dyn SpeechEngine>,
    model: Option<String>,
    /// The voice for books that do not choose one.
    voice: Option<String>,
    voices: VoiceList,
}

/// What the active engine has said about its voices.
#[derive(Debug, Clone, PartialEq)]
enum VoiceList {
    /// Not asked yet, or it could not be reached when asked.
    Unchecked,
    Known(Vec<aoide_tts::Voice>),
    /// It was ready but listed none; saved voice names are used as they are.
    Unlisted,
}

impl VoiceList {
    fn offered(&self) -> Option<&[aoide_tts::Voice]> {
        match self {
            VoiceList::Known(voices) => Some(voices),
            VoiceList::Unchecked | VoiceList::Unlisted => None,
        }
    }
}

/// What one look at the engine found that the caller acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Polled {
    Nothing,
    /// Its voices became known: plans made without them should be made again.
    VoicesKnown,
}

/// A book's voice, with the engine and its loaded model, for planning its speech.
pub(super) struct Speech {
    pub voicing: Voicing,
    pub descriptor: aoide_tts::EngineDescriptor,
    pub model: Option<String>,
}

pub(super) struct Engine {
    registry: Registry,
    active: Mutex<Active>,
    /// What the engine answered last (or that it is busy), before the server is folded in.
    answered: Mutex<Option<EngineStatusView>>,
    /// The server Aoide starts for the profile's launch command, if it has one.
    server: Mutex<Option<ServerView>>,
    /// The status last sent to the window.
    status: Mutex<Option<EngineStatusView>>,
    /// Something in this run has needed the engine (see [`EngineStatusView::wanted`]).
    wanted: AtomicBool,
    events: Arc<dyn Events>,
}

impl Engine {
    pub fn new(
        registry: Registry,
        profile: EngineProfile,
        engine: Arc<dyn SpeechEngine>,
        events: Arc<dyn Events>,
    ) -> Self {
        Self {
            registry,
            active: Mutex::new(Active {
                voice: profile.voice_id.clone(),
                voices: VoiceList::Unchecked,
                profile,
                engine,
                model: None,
            }),
            answered: Mutex::new(None),
            server: Mutex::new(None),
            status: Mutex::new(None),
            wanted: AtomicBool::new(false),
            events,
        }
    }

    /// An engine for `profile`, built through the registry (so its network rules apply), with
    /// its parameters checked. Nothing changes until it is [switched to](Engine::switch).
    pub fn create(&self, profile: &EngineProfile) -> Result<Arc<dyn SpeechEngine>, String> {
        let engine = self.registry.create(profile).map_err(|e| e.to_string())?;
        resolve_params(engine.descriptor(), &profile.params).map_err(|e| e.to_string())?;
        Ok(engine)
    }

    /// Makes `engine` (built from `profile`) the one in use; nothing is known about it yet.
    pub fn switch(&self, profile: EngineProfile, engine: Arc<dyn SpeechEngine>) {
        {
            let mut active = lock(&self.active);
            active.voice = profile.voice_id.clone();
            active.voices = VoiceList::Unchecked;
            active.profile = profile;
            active.engine = engine;
            active.model = None;
        }
        *lock(&self.answered) = None;
        *lock(&self.status) = None;
    }

    pub fn profile(&self) -> EngineProfile {
        lock(&self.active).profile.clone()
    }

    pub fn profile_name(&self) -> String {
        lock(&self.active).profile.name.clone()
    }

    pub fn options(&self) -> EngineOptions {
        EngineOptions {
            descriptors: self.registry.descriptors(),
            profile: self.profile(),
        }
    }

    /// Whether the engine in use accepts `params`.
    pub fn check_params(&self, params: &Params) -> Result<(), String> {
        let active = lock(&self.active);
        resolve_params(active.engine.descriptor(), params)
            .map(drop)
            .map_err(|e| e.to_string())
    }

    /// The voice for books that do not choose one, once known.
    pub fn default_voice(&self) -> Option<String> {
        lock(&self.active).voice.clone()
    }

    /// The voices the engine in use offers, asked now; when it does not answer (its server
    /// may be stopped, and a voice menu does not start it), the voices it offered last.
    pub async fn voices(&self) -> Result<Vec<aoide_tts::Voice>, String> {
        let engine = Arc::clone(&lock(&self.active).engine);
        match engine.voices().await {
            Ok(voices) => Ok(voices),
            Err(error) => {
                let active = lock(&self.active);
                match &active.voices {
                    VoiceList::Known(voices) if Arc::ptr_eq(&active.engine, &engine) => {
                        Ok(voices.clone())
                    }
                    _ => Err(error.to_string()),
                }
            }
        }
    }

    /// How a book with `settings` in `language` is spoken, and the loaded model.
    pub fn speech(
        &self,
        settings: &BookSettings,
        language: Option<&str>,
    ) -> Result<Speech, String> {
        let active = lock(&self.active);
        let offered = active.voices.offered();
        let voicing = Voicing::resolve(
            active.engine.descriptor(),
            &active.profile.params,
            active.voice.as_deref(),
            settings,
            language,
            offered.and_then(fallback_voice).as_deref(),
            offered,
        )?;
        Ok(Speech {
            voicing,
            descriptor: active.engine.descriptor().clone(),
            model: active.model.clone(),
        })
    }

    /// Why the book's `language` may sound wrong with the engine in use, if it may.
    pub fn language_warning(&self, language: Option<&str>) -> Option<String> {
        let language = language?;
        let active = lock(&self.active);
        let descriptor = active.engine.descriptor();
        (!aoide_tts::supports_language(descriptor, language)).then(|| {
            format!(
                "{} does not list the book's language ({language}); narration may sound wrong.",
                active.profile.name
            )
        })
    }

    /// Why a book with `settings` is read in another voice than it chose, if it is.
    pub fn voice_warning(&self, settings: &BookSettings) -> Option<String> {
        let active = lock(&self.active);
        let voices = active.voices.offered()?;
        let voicing = Voicing::resolve(
            active.engine.descriptor(),
            &active.profile.params,
            active.voice.as_deref(),
            settings,
            None,
            fallback_voice(voices).as_deref(),
            Some(voices),
        )
        .ok()?;
        let missing = voicing.missing_voice?;
        let name = |id: &str| {
            voices.iter().find(|v| v.id == id).map_or_else(
                || id.trim_end_matches(".wav").to_owned(),
                |v| v.display_name.clone(),
            )
        };
        Some(format!(
            "{} is not one of {}'s voices, so {} reads this book. Choose a voice in the book's settings.",
            name(&missing),
            active.profile.name,
            name(&voicing.voice_id)
        ))
    }

    /// Learns the engine's voices and settles the default voice before anything is planned.
    pub async fn ensure_voice(&self) -> Result<String, String> {
        let checked = self.check_voices(false).await;
        if let Some(voice) = self.default_voice() {
            return Ok(voice);
        }
        checked?;
        Err("the speech engine has no voices".into())
    }

    /// Asks the engine for its voices once. The default voice becomes the profile's if the
    /// engine offers it, otherwise the engine's own. `ready`: the engine just reported ready,
    /// so a failed listing means it cannot list voices rather than that it is down.
    pub async fn check_voices(&self, ready: bool) -> Result<(), String> {
        let engine = {
            let active = lock(&self.active);
            if active.voices != VoiceList::Unchecked {
                return Ok(());
            }
            Arc::clone(&active.engine)
        };
        let listed = engine.voices().await;
        let mut active = lock(&self.active);
        if !Arc::ptr_eq(&active.engine, &engine) {
            return Ok(()); // Switched meanwhile; the new engine is asked on its own.
        }
        match listed {
            Ok(voices) if voices.is_empty() => active.voices = VoiceList::Unlisted,
            Ok(voices) => {
                let named = active
                    .profile
                    .voice_id
                    .clone()
                    .filter(|v| !v.trim().is_empty());
                let offered = named.clone().filter(|v| voices.iter().any(|o| &o.id == v));
                if let (Some(voice), None) = (&named, &offered) {
                    tracing::warn!(
                        event = "engine.voice",
                        engine = %active.profile.name,
                        %voice,
                        "The engine has no such voice; using its default"
                    );
                }
                active.voice = offered.or_else(|| fallback_voice(&voices));
                active.voices = VoiceList::Known(voices);
            }
            Err(error) => {
                if ready {
                    active.voices = VoiceList::Unlisted;
                }
                return Err(error.to_string());
            }
        }
        Ok(())
    }

    /// Learns the loaded model's name, if the engine says it within a few seconds.
    pub async fn ensure_model(&self) {
        let engine = {
            let active = lock(&self.active);
            if active.model.is_some() {
                return;
            }
            Arc::clone(&active.engine)
        };
        if let Ok(EngineStatus::Ready(model)) =
            tokio::time::timeout(Duration::from_secs(3), engine.status()).await
        {
            lock(&self.active).model = model;
        }
    }

    /// One look at the engine (skipped while it is `busy` making audio): learns its model and
    /// voices once it is ready, and sends its status if that changed. Returns what it answered,
    /// for the server Aoide starts for it.
    pub async fn poll(&self, busy: bool) -> (Polled, Option<Answer>) {
        if busy {
            return (Polled::Nothing, None);
        }
        let (engine, name) = {
            let active = lock(&self.active);
            (Arc::clone(&active.engine), active.profile.name.clone())
        };
        let status = engine.status().await;
        let mut polled = Polled::Nothing;
        if let EngineStatus::Ready(model) = &status {
            {
                let mut active = lock(&self.active);
                if active.model.is_none() {
                    active.model = model.clone();
                }
            }
            if lock(&self.active).voices == VoiceList::Unchecked {
                let _ = self.check_voices(true).await;
                if lock(&self.active).voices.offered().is_some() {
                    polled = Polled::VoicesKnown;
                }
            }
        }
        let (state, detail, answer) = match status {
            EngineStatus::Offline => (EngineState::Offline, None, Answer::Offline),
            EngineStatus::Loading => (EngineState::Loading, None, Answer::Loading),
            EngineStatus::Ready(model) => (EngineState::Ready, model, Answer::Ready),
            EngineStatus::Error(message) => (EngineState::Error, Some(message), Answer::Error),
        };
        self.set_status(EngineStatusView {
            profile_name: name,
            state,
            detail,
            wanted: false,
            server: None,
        });
        (polled, Some(answer))
    }

    /// The server Aoide starts for the engine changed (`None`: there is none).
    pub fn set_server(&self, view: Option<ServerView>) {
        *lock(&self.server) = view;
        self.publish();
    }

    /// The engine started making audio.
    pub fn mark_busy(&self) {
        // Making audio is always for something the user asked for.
        self.wanted.store(true, Ordering::SeqCst);
        self.set_status(EngineStatusView {
            profile_name: self.profile_name(),
            state: EngineState::Busy,
            detail: None,
            wanted: true,
            server: None,
        });
    }

    /// The status last sent, or "checking" before the first look.
    pub fn status(&self) -> EngineStatusView {
        lock(&self.status).clone().unwrap_or_else(|| {
            self.folded().unwrap_or_else(|| EngineStatusView {
                profile_name: self.profile_name(),
                state: EngineState::Offline,
                detail: Some("checking…".into()),
                wanted: self.wanted.load(Ordering::SeqCst),
                server: None,
            })
        })
    }

    /// The user asked for something the engine does (listening, preparing, a voice sample):
    /// from now on its status, problems included, is theirs to see.
    pub fn want(&self) {
        if self.wanted.swap(true, Ordering::SeqCst) {
            return;
        }
        self.publish();
    }

    /// Speaks a short sample sentence with a profile that may not be saved yet (the settings
    /// form) or with a book's voice overrides, and returns it as a WAV file. The engine is
    /// built through the registry, so the profile's network rules apply as for generation.
    pub async fn preview(
        &self,
        profile: EngineProfile,
        voice_id: Option<String>,
        params: Params,
        language: Option<String>,
    ) -> Result<Vec<u8>, String> {
        let lines = [(PREVIEW_TEXT.to_owned(), 0)];
        self.say(profile, voice_id, params, language, &lines).await
    }

    /// Says each line in turn, each followed by its silence in milliseconds, as one WAV file;
    /// otherwise as [`Engine::preview`].
    pub async fn say(
        &self,
        profile: EngineProfile,
        voice_id: Option<String>,
        params: Params,
        language: Option<String>,
        lines: &[(String, u32)],
    ) -> Result<Vec<u8>, String> {
        let lines: Vec<(String, u32, Role)> = lines
            .iter()
            .map(|(text, pause)| (text.clone(), *pause, Role::Narration))
            .collect();
        self.say_roles(profile, (voice_id, None), params, language, &lines)
            .await
    }

    /// [`Engine::say`] with each line said in its role: dialogue in `voices.1`, when given.
    pub async fn say_roles(
        &self,
        profile: EngineProfile,
        (voice_id, dialogue_voice_id): (Option<String>, Option<String>),
        params: Params,
        language: Option<String>,
        lines: &[(String, u32, Role)],
    ) -> Result<Vec<u8>, String> {
        self.want();
        let engine = self.registry.create(&profile).map_err(|e| e.to_string())?;
        let descriptor = engine.descriptor().clone();
        let book = BookSettings {
            voice_id: voice_id.filter(|v| !v.trim().is_empty()),
            dialogue_voice_id: dialogue_voice_id.filter(|v| !v.trim().is_empty()),
            params,
            ..BookSettings::default()
        };
        let named = profile
            .voice_id
            .as_deref()
            .is_some_and(|v| !v.trim().is_empty());
        let fallback = if book.voice_id.is_none() && !named {
            fallback_voice(&engine.voices().await.map_err(|e| e.to_string())?)
        } else {
            None
        };
        // The preview is told the book's language (it may not be the open book).
        let request = Voicing::resolve(
            &descriptor,
            &profile.params,
            profile.voice_id.as_deref(),
            &book,
            language.as_deref(),
            fallback.as_deref(),
            None, // The settings form previews exactly the voice it names.
        )?;
        let mut said = aoide_tts::Synthesis {
            sample_rate: 0,
            samples: Vec::new(),
            word_timings: None,
        };
        for (text, pause_ms, role) in lines {
            let audio = engine
                .synthesize(&request.request_as(text.clone(), 0, *role))
                .await
                .map_err(|e| e.to_string())?;
            follow_with(&mut said, audio, *pause_ms);
        }
        aoide_tts::encode_wav(&said).map_err(|e| e.to_string())
    }

    fn set_status(&self, status: EngineStatusView) {
        *lock(&self.answered) = Some(status);
        self.publish();
    }

    /// What the window is shown: the engine's answer, with the server folded in.
    fn folded(&self) -> Option<EngineStatusView> {
        let answered = lock(&self.answered).clone();
        let server = lock(&self.server).clone();
        let mut status = match (answered, &server) {
            (Some(status), _) => status,
            (None, Some(_)) => EngineStatusView {
                profile_name: self.profile_name(),
                state: EngineState::Offline,
                detail: None,
                wanted: false,
                server: None,
            },
            (None, None) => return None,
        };
        status.wanted = self.wanted.load(Ordering::SeqCst);
        if let Some(server) = &server {
            if let Some((state, detail)) = server_status(server) {
                status.state = state;
                status.detail = Some(detail);
            }
        }
        status.server = server;
        Some(status)
    }

    /// Sends the status to the window if it changed.
    fn publish(&self) {
        let Some(status) = self.folded() else { return };
        let mut last = lock(&self.status);
        let before = last.as_ref().map(|s| s.state);
        if before != Some(status.state) {
            tracing::info!(
                event = "engine.state",
                engine = %status.profile_name,
                from = crate::logging::name(before),
                to = crate::logging::name(status.state),
                detail = status.detail.as_deref(),
                "Speech engine state changed"
            );
        }
        if last.as_ref() != Some(&status) {
            let event = CoreEvent::EngineStatus(status.clone());
            *last = Some(status);
            drop(last);
            if let Ok(payload) = event.payload() {
                self.events.emit(event.name(), payload);
            }
        }
    }
}

/// The engine status the window shows for the server's state, when it is not the engine's own
/// answer (a server that runs answers for itself).
fn server_status(server: &ServerView) -> Option<(EngineState, String)> {
    let detail = |text: &str| text.to_owned();
    Some(match server.state {
        ServerState::Running | ServerState::External => return None,
        ServerState::Stopped => (
            EngineState::Offline,
            match (server.stopped, server.idle_minutes) {
                (Some(StoppedKind::Idle), Some(minutes)) => {
                    format!("Stopped after {minutes} minutes unused; starts when needed")
                }
                (Some(StoppedKind::Asked), _) => detail("Stopped; starts when needed"),
                _ => detail("Starts when needed"),
            },
        ),
        ServerState::Starting => (EngineState::Loading, detail("Starting the voice…")),
        ServerState::Stopping => (EngineState::Offline, detail("Stopping…")),
        ServerState::Waiting => (
            EngineState::Loading,
            detail("The voice stopped; starting it again"),
        ),
        ServerState::Failed => (EngineState::Error, failure_message(server)),
    })
}

/// Why the server failed, in words.
fn failure_message(server: &ServerView) -> String {
    let ended = match (server.exit_code, server.signal) {
        (Some(code), _) => format!("exit code {code}"),
        (None, Some(signal)) => format!("signal {signal}"),
        (None, None) => "no exit code".into(),
    };
    match server.failure {
        Some(FailureKind::Spawn) => format!(
            "The speech server could not be started: {}.",
            server.error.as_deref().unwrap_or("unknown error")
        ),
        Some(FailureKind::PortTaken) => "Something else listens at the speech engine's address, \
             so Aoide did not start its server."
            .into(),
        Some(FailureKind::StartTimedOut) => format!(
            "The speech server did not answer or print anything for {} seconds, so Aoide stopped it.",
            server.start_timeout_seconds
        ),
        Some(FailureKind::TooManyRestarts) => format!(
            "The speech server kept stopping ({ended}), so Aoide stopped starting it. \
             Its last lines are in Settings › Speech engine."
        ),
        Some(FailureKind::Exited) | None => format!(
            "The speech server ended ({ended}). Its last lines are in Settings › Speech engine."
        ),
    }
}

impl super::Core {
    pub(super) async fn ensure_voice(&self) -> Result<String, String> {
        self.engine.ensure_voice().await
    }

    pub(super) async fn check_voices(&self, ready: bool) -> Result<(), String> {
        self.engine.check_voices(ready).await
    }

    pub(super) async fn ensure_model(&self) {
        self.engine.ensure_model().await
    }

    pub(super) fn language_warning(&self, language: Option<&str>) -> Option<String> {
        self.engine.language_warning(language)
    }

    pub(super) fn voice_warning(&self, book: &crate::book::BookModel) -> Option<String> {
        self.engine.voice_warning(&self.book_settings(&book.id))
    }

    pub fn engine_options(&self) -> EngineOptions {
        self.engine.options()
    }

    pub async fn engine_voices(&self) -> Result<Vec<aoide_tts::Voice>, String> {
        self.engine.voices().await
    }

    /// Speaks a short sample sentence; see [`engine::Engine::preview`].
    pub async fn preview_voice(
        &self,
        profile: EngineProfile,
        voice_id: Option<String>,
        params: Params,
        language: Option<String>,
    ) -> Result<Vec<u8>, String> {
        if profile.launch.is_some() && profile.launch == self.engine.profile().launch {
            self.speech_server.want();
            self.wait_for_speech_server().await?;
        }
        self.engine
            .preview(profile, voice_id, params, language)
            .await
    }

    /// Says a few short paragraphs in the voice books are read in, with `pauses` between them
    /// (a paragraph's, then a scene break's), as one WAV file. Nothing is saved or cached.
    pub async fn preview_pauses(
        &self,
        pauses: super::settings::PauseSettings,
    ) -> Result<Vec<u8>, String> {
        let profile = self.engine.profile();
        if profile.launch.is_some() {
            self.speech_server.want();
            self.wait_for_speech_server().await?;
        }
        let block = |text: &str, break_before| aoide_text::Block {
            kind: aoide_text::BlockKind::Paragraph,
            text: text.into(),
            break_before,
        };
        let blocks = [
            block(
                "The ferry left before dawn. Nobody on the quay waved.",
                false,
            ),
            block("By noon the island was a grey line behind us.", false),
            block("Three summers passed before I went back.", true),
        ];
        let segments = aoide_text::segment_chapter(0, &blocks, &pauses.segment_options());
        let last = segments.len().saturating_sub(1);
        let lines: Vec<(String, u32)> = segments
            .into_iter()
            .enumerate()
            .map(|(i, s)| (s.speech, if i == last { 0 } else { s.pause_after_ms }))
            .collect();
        self.engine
            .say(profile, None, Params::default(), None, &lines)
            .await
    }

    /// Waits while the speech server Aoide starts is on its way: until it answers, or fails.
    pub(super) async fn wait_for_speech_server(&self) -> Result<(), String> {
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(600) {
            let Some(view) = self.speech_server.view() else {
                return Ok(());
            };
            match view.state {
                ServerState::Running | ServerState::External => return Ok(()),
                ServerState::Failed => {
                    return Err(failure_message(&view));
                }
                // Not launched: nothing answered yet to say the engine is offline.
                ServerState::Stopped if started.elapsed() > Duration::from_secs(5) => return Ok(()),
                _ => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
        Ok(())
    }

    /// Validates, persists and activates a new engine profile. Must run inside the runtime.
    pub async fn save_engine_profile(&self, profile: EngineProfile) -> Result<(), String> {
        let engine = self.engine.create(&profile)?;
        self.settings.save(super::config::ENGINE, &profile)?;
        self.activate_profile(profile, engine).await
    }

    /// Switches generation to a validated profile.
    pub(super) async fn activate_profile(
        &self,
        profile: EngineProfile,
        engine: Arc<dyn SpeechEngine>,
    ) -> Result<(), String> {
        self.speech_server.switch(super::engine_process::Spec::of(
            &profile.launch,
            &profile.base_url,
        ));
        self.engine.switch(profile, Arc::clone(&engine));
        self.plan(super::PlanEvent::EngineSwitched(engine));
        self.restart_session().await
    }

    pub fn engine_status(&self) -> EngineStatusView {
        self.engine.status()
    }

    /// Try again: launches a speech server that failed once more.
    pub fn retry_speech_server(&self) {
        self.speech_server.retry();
        self.speech_server.want();
    }

    /// Stop now: stops the speech server Aoide started, freeing the graphics card; the next
    /// want starts it again.
    pub fn stop_speech_server(&self) {
        self.speech_server.stop_now();
    }

    /// Shows the speech server's log in the file manager.
    pub fn show_speech_server_log(&self) -> Result<(), String> {
        let path = self.speech_server.log_path();
        if !path.exists() {
            return Err("The speech server has not written a log yet.".into());
        }
        (self.show_in_folder)(&path)
    }
}

/// The speech server's view of the rest of the app.
pub(super) struct SpeechServer<'a>(pub &'a super::Core);

impl super::engine_process::ServerHost for SpeechServer<'_> {
    fn now_ms(&self) -> i64 {
        super::now_ms()
    }

    fn wanted(&self) -> bool {
        self.0.generation.wants_speech()
    }

    fn server_changed(&self, view: Option<ServerView>) {
        let held = view
            .as_ref()
            .is_some_and(|v| !matches!(v.state, ServerState::Running | ServerState::External));
        self.0.engine.set_server(view);
        if held != self.0.generation.held() {
            self.0.plan(if held {
                super::PlanEvent::EngineHeld
            } else {
                super::PlanEvent::EngineReady
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;
    use crate::core::engine_tests::{profile, Scripted};
    use crate::core::tests::Recorder;

    fn engine(
        first: &Arc<Scripted>,
        others: &[&Arc<Scripted>],
        profile: EngineProfile,
    ) -> (Arc<Engine>, Arc<Recorder>) {
        let mut registry = Registry::new();
        for engine in std::iter::once(first).chain(others.iter().copied()) {
            let engine = Arc::clone(engine);
            registry.register(engine.descriptor.clone(), move |_| {
                Ok(Arc::clone(&engine) as Arc<dyn SpeechEngine>)
            });
        }
        let events = Arc::new(Recorder::default());
        let engine = Engine::new(
            registry,
            profile,
            Arc::clone(first) as Arc<dyn SpeechEngine>,
            Arc::clone(&events) as Arc<dyn Events>,
        );
        (Arc::new(engine), events)
    }

    fn choosing(voice: &str) -> BookSettings {
        BookSettings {
            voice_id: Some(voice.into()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn a_listing_from_the_engine_switched_away_from_is_ignored() {
        let first = Scripted::new("first", EngineStatus::Ready(None), &["First.wav"], true);
        let second = Scripted::new("second", EngineStatus::Offline, &["Second.wav"], false);
        let (engine, _events) = engine(&first, &[&second], profile("first"));
        let listing = tokio::spawn({
            let engine = Arc::clone(&engine);
            async move { engine.check_voices(true).await }
        });
        while first.voice_calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }

        engine.switch(
            profile("second"),
            engine.create(&profile("second")).unwrap(),
        );
        first.release();
        listing.await.unwrap().unwrap();

        assert_eq!(second.voice_calls.load(Ordering::SeqCst), 0);
        assert_eq!(engine.voice_warning(&choosing("Nowhere.wav")), None);
    }

    #[tokio::test]
    async fn a_ready_engine_listing_no_voices_keeps_the_saved_voice_as_it_is() {
        let plain = Scripted::new("plain", EngineStatus::Ready(None), &[], false);
        let mut saved = profile("plain");
        saved.voice_id = Some("Saved.wav".into());
        let (engine, _events) = engine(&plain, &[], saved);

        assert_eq!(engine.ensure_voice().await, Ok("Saved.wav".into()));
        let speech = engine.speech(&BookSettings::default(), None).unwrap();
        assert_eq!(speech.voicing.voice_id, "Saved.wav");
        assert_eq!(engine.voice_warning(&choosing("Other.wav")), None);
    }

    #[tokio::test]
    async fn its_status_is_sent_only_when_it_changes_and_not_while_busy() {
        let steady = Scripted::new("steady", EngineStatus::Ready(None), &["A.wav"], false);
        let (engine, events) = engine(&steady, &[], profile("steady"));

        assert_eq!(engine.poll(false).await.0, Polled::VoicesKnown);
        assert_eq!(engine.poll(false).await.0, Polled::Nothing);
        assert_eq!(engine.poll(true).await, (Polled::Nothing, None));
        assert_eq!(events.count(CoreEvent::ENGINE_STATUS), 1);
        assert_eq!(
            steady.status_calls.load(Ordering::SeqCst),
            2,
            "not asked while busy"
        );

        steady.set_status(EngineStatus::Loading);
        engine.poll(false).await;
        let sent = events.payloads(CoreEvent::ENGINE_STATUS);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1]["state"], "loading");
        assert_eq!(engine.status().state, EngineState::Loading);
    }
}
