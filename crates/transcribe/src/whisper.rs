//! whisper.cpp, built in.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Once;

use whisper_rs::{
    DtwMode, DtwModelPreset, DtwParameters, FullParams, SamplingStrategy, WhisperContext,
    WhisperContextParameters,
};

use crate::{words_from_tokens, Error, Request, TokenTimes, Transcriber, Word};

#[derive(Debug, Clone)]
pub struct WhisperOptions {
    /// A ggml model file, such as `ggml-base.en.bin`.
    pub model: PathBuf,
    /// CPU threads; 0 uses half the available cores.
    pub threads: usize,
    /// Use the GPU when Aoide was built with GPU support.
    pub use_gpu: bool,
    /// A Silero voice-activity model (`ggml-silero-*.bin`) to skip music and silence.
    pub vad_model: Option<PathBuf>,
}

pub struct WhisperCpp {
    context: WhisperContext,
    options: WhisperOptions,
    id: String,
}

static QUIET: Once = Once::new();

impl WhisperCpp {
    pub fn new(options: WhisperOptions) -> Result<Self, Error> {
        // Send whisper.cpp's console output to the `log` facade, which Aoide does not print.
        QUIET.call_once(whisper_rs::install_logging_hooks);
        let mut params = WhisperContextParameters::default();
        params.use_gpu(options.use_gpu);
        if let Some(model_preset) = dtw_preset(&options.model) {
            params.dtw_parameters(DtwParameters {
                mode: DtwMode::ModelPreset { model_preset },
                ..DtwParameters::default()
            });
        }
        let path = options
            .model
            .to_str()
            .ok_or_else(|| Error::Model("the model path is not valid UTF-8".into()))?;
        let context = WhisperContext::new_with_params(path, params)
            .map_err(|e| Error::Model(format!("{e:?}")))?;
        let name = options
            .model
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(WhisperCpp {
            context,
            id: format!("whisper.cpp:{name}"),
            options,
        })
    }
}

/// The alignment-head preset for a standard model file name, for DTW word times.
fn dtw_preset(model: &Path) -> Option<DtwModelPreset> {
    let name = model.file_name()?.to_str()?.to_ascii_lowercase();
    let name = name.strip_prefix("ggml-")?.trim_end_matches(".bin");
    // Quantised variants ("base.en-q5_1") share their model's heads.
    let base = name.split("-q").next()?;
    Some(match base {
        "tiny.en" => DtwModelPreset::TinyEn,
        "tiny" => DtwModelPreset::Tiny,
        "base.en" => DtwModelPreset::BaseEn,
        "base" => DtwModelPreset::Base,
        "small.en" => DtwModelPreset::SmallEn,
        "small" => DtwModelPreset::Small,
        "medium.en" => DtwModelPreset::MediumEn,
        "medium" => DtwModelPreset::Medium,
        "large-v1" => DtwModelPreset::LargeV1,
        "large-v2" => DtwModelPreset::LargeV2,
        "large-v3" => DtwModelPreset::LargeV3,
        "large-v3-turbo" => DtwModelPreset::LargeV3Turbo,
        _ => return None,
    })
}

/// whisper.cpp asks this between computations whether to stop. `data` is an `AtomicBool`.
unsafe extern "C" fn should_abort(data: *mut std::ffi::c_void) -> bool {
    // SAFETY: set above to the request's abort flag, alive for the whole recognition.
    unsafe { (*data.cast::<AtomicBool>()).load(Ordering::Relaxed) }
}

impl Transcriber for WhisperCpp {
    fn id(&self) -> &str {
        &self.id
    }

    fn transcribe(&self, request: &Request<'_>) -> Result<Vec<Word>, Error> {
        let failed = |e: whisper_rs::WhisperError| Error::Failed(format!("{e:?}"));
        let mut state = self.context.create_state().map_err(failed)?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        let threads = match self.options.threads {
            0 => std::thread::available_parallelism().map_or(4, |n| (n.get() / 2).max(1)),
            n => n,
        };
        params.set_n_threads(threads as i32);
        params.set_translate(false);
        params.set_language(request.language);
        // Each block stands alone: carrying text over invites repetition loops.
        params.set_no_context(true);
        params.set_token_timestamps(true);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        if let Some(abort) = &request.abort {
            // Not `set_abort_callback_safe`: in whisper-rs 0.16 its trampoline reads the boxed
            // closure as the wrong type, so the flag reads as garbage, usually "stop".
            // SAFETY: the user data is the request's flag, which outlives `full` below, and
            // `should_abort` only reads it.
            unsafe {
                params.set_abort_callback(Some(should_abort));
                params.set_abort_callback_user_data(Arc::as_ptr(abort).cast_mut().cast());
            }
        }
        let vad = self.options.vad_model.as_ref().and_then(|p| p.to_str());
        if let Some(vad) = vad {
            params.enable_vad(true);
            params.set_vad_model_path(Some(vad));
        }
        let result = state.full(params, request.samples);
        if request
            .abort
            .as_ref()
            .is_some_and(|a| a.load(Ordering::Relaxed))
        {
            return Err(Error::Aborted);
        }
        result.map_err(failed)?;
        let eot = self.context.token_eot();
        let mut tokens = Vec::new();
        for segment in state.as_iter() {
            for i in 0..segment.n_tokens() {
                let Some(token) = segment.get_token(i) else {
                    continue;
                };
                let data = token.token_data();
                tokens.push(TokenTimes {
                    bytes: token.to_bytes().map(<[u8]>::to_vec).unwrap_or_default(),
                    t0: data.t0,
                    t1: data.t1,
                    t_dtw: data.t_dtw,
                    probability: data.p,
                    special: token.token_id() >= eot,
                });
            }
        }
        Ok(words_from_tokens(&tokens))
    }
}
