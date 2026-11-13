//! The speech-recognition models Aoide offers, with pinned checksums. Nothing is downloaded
//! here; the app fetches a model only when the user asks.

/// How large a model to use. Larger ones hear more words right but are slower, and on audiobook
/// narration their word times are no tighter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelSize {
    Tiny,
    Base,
    Small,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelInfo {
    /// Also the model's name in whisper.cpp ("base.en").
    pub id: &'static str,
    pub size: ModelSize,
    /// Trained on English only (better for English, useless for anything else).
    pub english_only: bool,
    pub bytes: u64,
    /// Lower-case hex SHA-256 of the file.
    pub sha256: &'static str,
}

impl ModelInfo {
    /// The file name, as whisper.cpp names it.
    pub fn file_name(&self) -> String {
        format!("ggml-{}.bin", self.id)
    }

    pub fn url(&self) -> String {
        format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            self.file_name()
        )
    }
}

pub const MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "tiny.en",
        size: ModelSize::Tiny,
        english_only: true,
        bytes: 77_704_715,
        sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
    },
    ModelInfo {
        id: "tiny",
        size: ModelSize::Tiny,
        english_only: false,
        bytes: 77_691_713,
        sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
    },
    ModelInfo {
        id: "base.en",
        size: ModelSize::Base,
        english_only: true,
        bytes: 147_964_211,
        sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
    },
    ModelInfo {
        id: "base",
        size: ModelSize::Base,
        english_only: false,
        bytes: 147_951_465,
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
    },
    ModelInfo {
        id: "small.en",
        size: ModelSize::Small,
        english_only: true,
        bytes: 487_614_201,
        sha256: "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
    },
    ModelInfo {
        id: "small",
        size: ModelSize::Small,
        english_only: false,
        bytes: 487_601_967,
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
    },
];

/// The model of `size` for a book in `language` (BCP-47): the English-only one for English.
pub fn model_for(size: ModelSize, language: Option<&str>) -> &'static ModelInfo {
    let english = language.is_none_or(|l| {
        l.split(['-', '_'])
            .next()
            .is_some_and(|p| p.eq_ignore_ascii_case("en"))
    });
    MODELS
        .iter()
        .find(|m| m.size == size && m.english_only == english)
        .expect("every size has both kinds")
}

pub fn model(id: &str) -> Option<&'static ModelInfo> {
    MODELS.iter().find(|m| m.id == id)
}

/// Whether this build can run recognition on a GPU (CUDA or Vulkan).
pub const GPU_BUILD: bool = cfg!(any(feature = "whisper-cuda", feature = "whisper-vulkan"));
