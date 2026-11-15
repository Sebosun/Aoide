//! The speech-recognition models: which one a book uses, and downloading them.

use super::*;

impl Recordings {
    /// The model a book's recording is transcribed with.
    pub(super) fn model_for_book(&self, host: &dyn SyncHost, id: &str) -> &'static ModelInfo {
        let size = host.recognition().model_size.into();
        let language = lock(&self.library)
            .book(&BookId(id.into()))
            .ok()
            .flatten()
            .and_then(|b| b.language);
        models::model_for(size, language.as_deref())
    }

    pub(super) fn model_path(&self, model: &ModelInfo) -> PathBuf {
        self.models_dir.join(model.file_name())
    }

    pub(in crate::core) fn recognition_options(&self, host: &dyn SyncHost) -> RecognitionOptions {
        RecognitionOptions {
            settings: host.recognition(),
            gpu_build: models::GPU_BUILD,
            models: models::MODELS
                .iter()
                .map(|m| ModelView {
                    id: m.id.into(),
                    size: match m.size {
                        ModelSize::Tiny => ModelSizeChoice::Tiny,
                        ModelSize::Base => ModelSizeChoice::Base,
                        ModelSize::Small => ModelSizeChoice::Small,
                    },
                    english_only: m.english_only,
                    bytes: m.bytes,
                    downloaded: self.model_path(m).is_file(),
                })
                .collect(),
            models_folder: self.models_dir.to_string_lossy().into_owned(),
        }
    }

    /// A model arrived (downloaded or put in place): recordings waiting for it carry on.
    pub(in crate::core) fn models_changed(&self, host: &dyn SyncHost) {
        let waiting: Vec<String> = lock(&self.state)
            .live
            .iter()
            .filter(|(_, l)| l.stage == SyncStage::NeedsModel)
            .map(|(id, _)| id.clone())
            .collect();
        for id in waiting {
            self.sync_event(host, &id, SyncEvent::ModelReady, None);
        }
        lock(&self.state).transcriber = None;
        self.wake_sync();
    }

    /// Downloads a model from its fixed address and keeps it only if its checksum matches.
    pub(in crate::core) async fn download_model(
        &self,
        host: &dyn SyncHost,
        id: &str,
    ) -> Result<(), String> {
        let model = models::model(id).ok_or("no such model")?;
        if !lock(&self.state).downloading.insert(id.to_owned()) {
            return Ok(());
        }
        let started = Instant::now();
        let result = self.fetch_model(host, model, &model.url()).await;
        lock(&self.state).downloading.remove(id);
        match &result {
            Ok(()) => tracing::info!(
                event = "model.download",
                model = id,
                bytes = model.bytes,
                took_ms = crate::logging::took(started),
                outcome = "ok",
                "Downloaded a recognition model"
            ),
            Err(error) => {
                tracing::warn!(event = "model.download", model = id, took_ms = crate::logging::took(started), outcome = "failed", %error, "Downloading a recognition model failed")
            }
        }
        let (stage, error) = match &result {
            Ok(()) => (DownloadStage::Done, None),
            Err(error) => (DownloadStage::Failed, Some(error.clone())),
        };
        host.emit(CoreEvent::ModelDownload(ModelDownloadView {
            id: id.into(),
            stage,
            done_bytes: if result.is_ok() { model.bytes } else { 0 },
            total_bytes: model.bytes,
            error,
        }));
        if result.is_ok() {
            self.models_changed(host);
        }
        result
    }

    pub(super) async fn fetch_model(
        &self,
        host: &dyn SyncHost,
        model: &ModelInfo,
        url: &str,
    ) -> Result<(), String> {
        use sha2::{Digest, Sha256};
        use tokio::io::AsyncWriteExt;
        std::fs::create_dir_all(&self.models_dir).map_err(|e| e.to_string())?;
        let path = self.model_path(model);
        let partial = path.with_extension("bin.part");
        let response = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?
            .get(url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| format!("download failed: {e}"))?;
        let mut file = tokio::fs::File::create(&partial)
            .await
            .map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut done = 0u64;
        let mut last_report = Instant::now();
        let mut response = response;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| format!("download failed: {e}"))?
        {
            hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            done += chunk.len() as u64;
            if last_report.elapsed() > Duration::from_millis(250) {
                last_report = Instant::now();
                host.emit(CoreEvent::ModelDownload(ModelDownloadView {
                    id: model.id.into(),
                    stage: DownloadStage::Downloading,
                    done_bytes: done,
                    total_bytes: model.bytes,
                    error: None,
                }));
            }
        }
        file.flush().await.map_err(|e| e.to_string())?;
        drop(file);
        let digest: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if digest != model.sha256 {
            let _ = std::fs::remove_file(&partial);
            return Err("the downloaded model is not the expected file (checksum mismatch)".into());
        }
        std::fs::rename(&partial, &path).map_err(|e| e.to_string())
    }
}
