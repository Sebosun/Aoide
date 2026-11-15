//! Downloading a browser for a user who has none: the pinned Chrome for Testing build in
//! `chrome-for-testing.toml`, into `<data>/browser/chrome/`. Resumable (a cancelled or broken
//! download continues where it stopped), kept only when its checksum matches, and removable.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use super::lock;

/// Where a download comes from and what it must be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub version: String,
    pub url: Url,
    pub bytes: u64,
    pub sha256: String,
    /// The program, relative to the unpacked folder.
    pub program: PathBuf,
}

#[derive(Deserialize)]
struct Pins {
    version: String,
    base: String,
    platforms: std::collections::HashMap<String, Pin>,
}

#[derive(Deserialize)]
struct Pin {
    bytes: u64,
    sha256: String,
    program: String,
}

/// Chrome for Testing's name for this platform; `None` where it has no build.
fn platform() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("linux64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        ("macos", "aarch64") => Some("mac-arm64"),
        ("macos", "x86_64") => Some("mac-x64"),
        ("windows", "x86_64") => Some("win64"),
        _ => None,
    }
}

impl Source {
    /// The pinned build for this platform, if there is one.
    pub fn pinned() -> Option<Self> {
        Self::from_pins(include_str!("chrome-for-testing.toml"), platform()?)
    }

    fn from_pins(text: &str, platform: &str) -> Option<Self> {
        let pins: Pins = toml::from_str(text).ok()?;
        let pin = pins.platforms.get(platform)?;
        let url = format!(
            "{}/{}/{platform}/chrome-{platform}.zip",
            pins.base, pins.version
        );
        Some(Self {
            version: pins.version,
            url: Url::parse(&url).ok()?,
            bytes: pin.bytes,
            sha256: pin.sha256.clone(),
            program: PathBuf::from(&pin.program),
        })
    }
}

/// Where a download is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum BrowserDownloadStage {
    Downloading,
    /// All of it arrived; its checksum is being checked.
    Checking,
    Unpacking,
    Done,
    /// Stopped by the user; downloading again continues from where it stopped.
    Cancelled,
    Failed,
}

/// What happens to a download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadEvent {
    Received,
    Verified,
    Unpacked,
    Cancelled,
    Failed,
}

/// The one place a download's stage changes. Only a download under way can be cancelled or
/// fail; a finished one stays as it is.
pub fn next(stage: BrowserDownloadStage, event: DownloadEvent) -> BrowserDownloadStage {
    use BrowserDownloadStage as S;
    use DownloadEvent as E;
    match (stage, event) {
        (S::Downloading, E::Received) => S::Checking,
        (S::Checking, E::Verified) => S::Unpacking,
        (S::Unpacking, E::Unpacked) => S::Done,
        (S::Downloading, E::Cancelled) => S::Cancelled,
        (S::Downloading | S::Checking | S::Unpacking, E::Failed) => S::Failed,
        (stage, _) => stage,
    }
}

impl BrowserDownloadStage {
    fn name(self) -> &'static str {
        match self {
            Self::Downloading => "downloading",
            Self::Checking => "checking",
            Self::Unpacking => "unpacking",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    fn active(self) -> bool {
        matches!(self, Self::Downloading | Self::Checking | Self::Unpacking)
    }
}

/// A browser download, as Settings shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BrowserDownloadView {
    pub stage: BrowserDownloadStage,
    /// The version being downloaded.
    pub version: String,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub error: Option<String>,
}

/// Told about every change to the download (progress at most four times a second).
pub(super) trait DownloadHost: Send + Sync {
    fn download_changed(&self, view: BrowserDownloadView);
}

/// How often progress is told.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// The downloaded browser: getting it, and removing it.
pub(super) struct Download {
    source: Option<Source>,
    /// `<data>/browser`.
    dir: PathBuf,
    view: Mutex<Option<BrowserDownloadView>>,
    cancel: AtomicBool,
}

/// Why a download stopped before its end.
enum Stop {
    Cancelled,
    Failed(String),
}

impl From<std::io::Error> for Stop {
    fn from(error: std::io::Error) -> Self {
        Stop::Failed(error.to_string())
    }
}

impl Download {
    pub fn new(source: Option<Source>, dir: PathBuf) -> Self {
        Self {
            source,
            dir,
            view: Mutex::new(None),
            cancel: AtomicBool::new(false),
        }
    }

    /// The version a download would get; `None` on a platform with no build.
    pub fn offered(&self) -> Option<&str> {
        self.source.as_ref().map(|s| s.version.as_str())
    }

    /// The unpacked browser's folder.
    pub fn folder(&self) -> PathBuf {
        self.dir.join("chrome")
    }

    /// Where the downloaded program is (whether or not it is there).
    pub fn program(&self) -> PathBuf {
        let relative = self.source.as_ref().map(|s| s.program.clone());
        self.folder().join(relative.unwrap_or_default())
    }

    fn partial(&self) -> PathBuf {
        self.dir.join("chrome.zip.part")
    }

    pub fn view(&self) -> Option<BrowserDownloadView> {
        lock(&self.view).clone()
    }

    fn set(&self, host: &dyn DownloadHost, change: impl FnOnce(&mut BrowserDownloadView)) {
        let view = {
            let mut view = lock(&self.view);
            let Some(view) = view.as_mut() else { return };
            change(view);
            view.clone()
        };
        host.download_changed(view);
    }

    /// Moves the download on by `event`, logging the transition.
    fn step(&self, host: &dyn DownloadHost, event: DownloadEvent, error: Option<String>) {
        self.set(host, |view| {
            let to = next(view.stage, event);
            tracing::info!(
                event = "browser.download.stage",
                from = view.stage.name(),
                to = to.name(),
                "Browser download moved on"
            );
            view.stage = to;
            if error.is_some() {
                view.error = error;
            }
        });
    }

    /// Downloads, checks and unpacks the pinned browser. Returns once it is done, cancelled or
    /// failed; a second call while one runs does nothing.
    pub async fn download(&self, host: &dyn DownloadHost) -> Result<(), String> {
        let source = self
            .source
            .clone()
            .ok_or("No browser download is offered for this computer.")?;
        {
            let mut view = lock(&self.view);
            if view.as_ref().is_some_and(|v| v.stage.active()) {
                return Ok(());
            }
            *view = Some(BrowserDownloadView {
                stage: BrowserDownloadStage::Downloading,
                version: source.version.clone(),
                done_bytes: 0,
                total_bytes: source.bytes,
                error: None,
            });
        }
        self.cancel.store(false, Ordering::SeqCst);
        let started = Instant::now();
        let resumed_from = std::fs::metadata(self.partial()).map_or(0, |m| m.len());
        let result = self.run(host, &source).await;
        let outcome = match &result {
            Ok(()) => "ok",
            Err(Stop::Cancelled) => "cancelled",
            Err(Stop::Failed(_)) => "failed",
        };
        tracing::info!(
            event = "browser.download",
            version = source.version,
            resumed_from,
            outcome,
            took_ms = crate::logging::took(started),
            error = match &result {
                Err(Stop::Failed(error)) => Some(error.as_str()),
                _ => None,
            },
            "Downloaded a browser"
        );
        match result {
            Ok(()) => Ok(()),
            Err(Stop::Cancelled) => {
                self.step(host, DownloadEvent::Cancelled, None);
                Ok(())
            }
            Err(Stop::Failed(error)) => {
                self.step(host, DownloadEvent::Failed, Some(error.clone()));
                Err(error)
            }
        }
    }

    async fn run(&self, host: &dyn DownloadHost, source: &Source) -> Result<(), Stop> {
        std::fs::create_dir_all(&self.dir)?;
        let partial = self.partial();
        self.fetch(host, source, &partial).await?;
        self.step(host, DownloadEvent::Received, None);

        let digest = {
            let partial = partial.clone();
            tokio::task::spawn_blocking(move || sha256_of(&partial))
                .await
                .map_err(|e| Stop::Failed(e.to_string()))??
        };
        if digest != source.sha256 {
            let _ = std::fs::remove_file(&partial);
            return Err(Stop::Failed(
                "The downloaded browser is not the expected file (its checksum differs). \
                 Try again."
                    .into(),
            ));
        }
        self.step(host, DownloadEvent::Verified, None);

        let (folder, program) = (self.folder(), self.program());
        let version = source.version.clone();
        tokio::task::spawn_blocking(move || unpack(&partial, &folder, &program, &version))
            .await
            .map_err(|e| Stop::Failed(e.to_string()))?
            .map_err(Stop::Failed)?;
        self.step(host, DownloadEvent::Unpacked, None);
        Ok(())
    }

    /// Gets the zip into `partial`, continuing a partial one where the server allows.
    async fn fetch(
        &self,
        host: &dyn DownloadHost,
        source: &Source,
        partial: &Path,
    ) -> Result<(), Stop> {
        let mut have = std::fs::metadata(partial).map_or(0, |m| m.len());
        if have > source.bytes {
            std::fs::remove_file(partial)?;
            have = 0;
        }
        if have == source.bytes {
            return Ok(());
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| Stop::Failed(e.to_string()))?;
        let mut request = client.get(source.url.clone());
        if have > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let failed = |e: reqwest::Error| Stop::Failed(format!("The download failed: {e}"));
        let mut response = request
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(failed)?;
        let continued = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(continued)
            .write(true)
            .truncate(!continued)
            .open(partial)?;
        let mut done = if continued { have } else { 0 };
        let mut told = Instant::now();
        self.set(host, |view| view.done_bytes = done);
        while let Some(chunk) = response.chunk().await.map_err(failed)? {
            if self.cancel.load(Ordering::SeqCst) {
                return Err(Stop::Cancelled);
            }
            done += chunk.len() as u64;
            if done > source.bytes {
                drop(file);
                let _ = std::fs::remove_file(partial);
                return Err(Stop::Failed(
                    "The download is larger than the expected file.".into(),
                ));
            }
            file.write_all(&chunk)?;
            if told.elapsed() >= PROGRESS_EVERY {
                told = Instant::now();
                self.set(host, |view| view.done_bytes = done);
            }
        }
        file.flush()?;
        self.set(host, |view| view.done_bytes = done);
        if done < source.bytes {
            return Err(Stop::Failed(
                "The download ended early. Downloading again continues it.".into(),
            ));
        }
        Ok(())
    }

    /// Stops a download under way; what arrived is kept for the next try.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    /// Deletes the downloaded browser and any partial download.
    pub fn remove(&self) -> Result<(), String> {
        if lock(&self.view).as_ref().is_some_and(|v| v.stage.active()) {
            return Err("The browser is still downloading. Cancel it first.".into());
        }
        for dir in [self.folder(), self.dir.join("chrome.unpacking")] {
            match std::fs::remove_dir_all(&dir) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
                _ => {}
            }
        }
        match std::fs::remove_file(self.partial()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
            _ => {}
        }
        *lock(&self.view) = None;
        tracing::info!(event = "browser.remove", "Removed the downloaded browser");
        Ok(())
    }

    /// The version of the downloaded browser, when one is unpacked.
    pub fn installed_version(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.folder().join(VERSION_FILE)).ok()?;
        Some(text.trim().to_owned()).filter(|v| !v.is_empty())
    }
}

/// Written beside the unpacked browser: the version it is (read without running it).
const VERSION_FILE: &str = "aoide-version.txt";

fn sha256_of(path: &Path) -> Result<String, Stop> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Unpacks `zip` into `folder` (replacing what is there only once all of it unpacked), then
/// deletes the zip.
fn unpack(zip: &Path, folder: &Path, program: &Path, version: &str) -> Result<(), String> {
    let staging = folder.with_extension("unpacking");
    let _ = std::fs::remove_dir_all(&staging);
    let file = std::fs::File::open(zip).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("The downloaded browser cannot be unpacked: {e}"))?;
    archive
        .extract(&staging)
        .map_err(|e| format!("The downloaded browser cannot be unpacked: {e}"))?;
    let relative = program.strip_prefix(folder).unwrap_or(program);
    if !staging.join(relative).is_file() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err("The downloaded browser has no program where it should be.".into());
    }
    std::fs::write(staging.join(VERSION_FILE), version).map_err(|e| e.to_string())?;
    match std::fs::remove_dir_all(folder) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
        _ => {}
    }
    std::fs::rename(&staging, folder).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(zip);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// How the stand-in download host answers.
    #[derive(Default)]
    struct Serving {
        /// Send only this many bytes of the body the first time, then close.
        cut_first_at: Option<usize>,
        ignore_range: bool,
        /// Pause between 8 KiB pieces.
        slow: bool,
        /// The `Range` header of each request.
        ranges: Mutex<Vec<Option<String>>>,
        requests: AtomicUsize,
    }

    /// Serves `body` on loopback as `serving` says; returns its address.
    async fn host(body: Vec<u8>, serving: Arc<Serving>) -> Url {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let url = format!("http://{}/chrome.zip", listener.local_addr().unwrap());
        let body = Arc::new(body);
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let (body, serving) = (Arc::clone(&body), Arc::clone(&serving));
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut buffer = [0u8; 2048];
                    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                        match stream.read(&mut buffer).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => head.extend_from_slice(&buffer[..n]),
                        }
                    }
                    let head = String::from_utf8_lossy(&head).to_lowercase();
                    let range = head
                        .lines()
                        .find_map(|l| l.strip_prefix("range: "))
                        .map(str::to_owned);
                    let first = serving.requests.fetch_add(1, Ordering::SeqCst) == 0;
                    serving.ranges.lock().unwrap().push(range.clone());
                    let from = range
                        .filter(|_| !serving.ignore_range)
                        .and_then(|r| r.strip_prefix("bytes=")?.strip_suffix('-')?.parse().ok());
                    let (status, from) = match from {
                        Some(from) => ("206 Partial Content", from),
                        None => ("200 OK", 0usize),
                    };
                    let part = &body[from..];
                    let reply = format!(
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        part.len()
                    );
                    let _ = stream.write_all(reply.as_bytes()).await;
                    let end = match serving.cut_first_at {
                        Some(cut) if first => cut,
                        _ => part.len(),
                    };
                    for piece in part[..end].chunks(8192) {
                        if stream.write_all(piece).await.is_err() {
                            return;
                        }
                        if serving.slow {
                            tokio::time::sleep(Duration::from_millis(20)).await;
                        }
                    }
                });
            }
        });
        Url::parse(&url).unwrap()
    }

    /// A small zip shaped like Chrome for Testing's: a program, and a file that does not
    /// compress (so the download takes many pieces).
    fn chrome_zip() -> Vec<u8> {
        use zip::write::SimpleFileOptions;
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let stored =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("chrome-test/chrome", stored.unix_permissions(0o755))
            .unwrap();
        zip.write_all(b"#!/bin/sh\necho Chromium 1.0\n").unwrap();
        zip.start_file("chrome-test/resources.pak", stored.unix_permissions(0o644))
            .unwrap();
        let mut noise = 0x2545_f491_u32;
        let bytes: Vec<u8> = (0..300_000)
            .map(|_| {
                noise ^= noise << 13;
                noise ^= noise >> 17;
                noise ^= noise << 5;
                noise as u8
            })
            .collect();
        zip.write_all(&bytes).unwrap();
        zip.finish().unwrap().into_inner()
    }

    fn sha256(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[derive(Default)]
    struct Told(Mutex<Vec<BrowserDownloadView>>);

    impl DownloadHost for Told {
        fn download_changed(&self, view: BrowserDownloadView) {
            self.0.lock().unwrap().push(view);
        }
    }

    impl Told {
        fn stages(&self) -> Vec<BrowserDownloadStage> {
            let mut stages: Vec<_> = self.0.lock().unwrap().iter().map(|v| v.stage).collect();
            stages.dedup();
            stages
        }

        fn last(&self) -> BrowserDownloadView {
            self.0.lock().unwrap().last().cloned().unwrap()
        }
    }

    struct Case {
        _dir: tempfile::TempDir,
        download: Arc<Download>,
        told: Arc<Told>,
        serving: Arc<Serving>,
        zip: Vec<u8>,
    }

    async fn case(serving: Serving, sha: Option<&str>) -> Case {
        let zip = chrome_zip();
        let serving = Arc::new(serving);
        let url = host(zip.clone(), Arc::clone(&serving)).await;
        let dir = tempfile::tempdir().unwrap();
        let source = Source {
            version: "1.0-test".into(),
            url,
            bytes: zip.len() as u64,
            sha256: sha.map_or_else(|| sha256(&zip), str::to_owned),
            program: "chrome-test/chrome".into(),
        };
        Case {
            download: Arc::new(Download::new(Some(source), dir.path().join("browser"))),
            _dir: dir,
            told: Arc::default(),
            serving,
            zip,
        }
    }

    #[tokio::test]
    async fn the_browser_is_downloaded_checked_and_unpacked_with_progress_then_removed() {
        use BrowserDownloadStage as S;
        let c = case(Serving::default(), None).await;
        assert_eq!(c.download.installed_version(), None);
        c.download.download(&*c.told).await.unwrap();
        assert_eq!(
            c.told.stages(),
            vec![S::Downloading, S::Checking, S::Unpacking, S::Done]
        );
        let done = c.told.last();
        assert_eq!(
            (done.done_bytes, done.total_bytes),
            (c.zip.len() as u64, c.zip.len() as u64)
        );
        assert_eq!(done.version, "1.0-test");
        let program = c.download.program();
        assert!(program.ends_with("browser/chrome/chrome-test/chrome"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&program).unwrap().permissions().mode();
            assert_ne!(mode & 0o111, 0, "the program can be run");
        }
        assert_eq!(c.download.installed_version().as_deref(), Some("1.0-test"));
        assert!(
            !c.download.partial().exists(),
            "the zip is deleted once unpacked"
        );

        c.download.remove().unwrap();
        assert!(!c.download.folder().exists());
        assert_eq!(c.download.view(), None);
    }

    #[tokio::test]
    async fn a_broken_download_continues_where_it_stopped() {
        let c = case(
            Serving {
                cut_first_at: Some(100_000),
                ..Serving::default()
            },
            None,
        )
        .await;
        let first = c.download.download(&*c.told).await;
        assert!(first.is_err(), "{first:?}");
        assert_eq!(c.told.last().stage, BrowserDownloadStage::Failed);
        let kept = std::fs::metadata(c.download.partial()).unwrap().len();
        assert!(kept > 0 && kept <= 100_000, "{kept}");

        c.download.download(&*c.told).await.unwrap();
        assert_eq!(c.told.last().stage, BrowserDownloadStage::Done);
        let ranges = c.serving.ranges.lock().unwrap().clone();
        assert_eq!(ranges, vec![None, Some(format!("bytes={kept}-"))]);
    }

    #[tokio::test]
    async fn a_host_that_ignores_the_range_is_downloaded_again_from_the_start() {
        let c = case(
            Serving {
                ignore_range: true,
                ..Serving::default()
            },
            None,
        )
        .await;
        std::fs::create_dir_all(&c.download.dir).unwrap();
        std::fs::write(c.download.partial(), vec![0u8; 5_000]).unwrap();
        c.download.download(&*c.told).await.unwrap();
        assert_eq!(c.told.last().stage, BrowserDownloadStage::Done);
        assert_eq!(
            c.serving.ranges.lock().unwrap()[0].as_deref(),
            Some("bytes=5000-")
        );
    }

    #[tokio::test]
    async fn a_file_with_the_wrong_checksum_is_not_kept() {
        let c = case(Serving::default(), Some(&"0".repeat(64))).await;
        let error = c.download.download(&*c.told).await.unwrap_err();
        assert!(error.contains("checksum"), "{error}");
        assert_eq!(
            c.told.stages(),
            vec![
                BrowserDownloadStage::Downloading,
                BrowserDownloadStage::Checking,
                BrowserDownloadStage::Failed
            ]
        );
        assert!(!c.download.partial().exists());
        assert!(!c.download.folder().exists());
    }

    #[tokio::test]
    async fn a_cancelled_download_keeps_what_arrived_and_continues_later() {
        let c = case(
            Serving {
                slow: true,
                ..Serving::default()
            },
            None,
        )
        .await;
        let (download, told) = (Arc::clone(&c.download), Arc::clone(&c.told));
        let running = tokio::spawn(async move { download.download(&*told).await });
        while c.download.view().map_or(0, |v| v.done_bytes) == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(c.download.remove().is_err(), "not while it downloads");
        assert!(
            c.download.download(&*c.told).await.is_ok(),
            "a second start does nothing"
        );
        assert_eq!(c.serving.requests.load(Ordering::SeqCst), 1);
        c.download.cancel();
        running.await.unwrap().unwrap();
        assert_eq!(c.told.last().stage, BrowserDownloadStage::Cancelled);
        let kept = std::fs::metadata(c.download.partial()).unwrap().len();
        assert!(kept > 0 && kept < c.zip.len() as u64);

        c.download.download(&*c.told).await.unwrap();
        assert_eq!(c.told.last().stage, BrowserDownloadStage::Done);
        let ranges = c.serving.ranges.lock().unwrap().clone();
        assert_eq!(
            ranges.last().unwrap().as_deref(),
            Some(format!("bytes={kept}-").as_str())
        );
    }

    #[test]
    fn every_platform_has_a_well_formed_pin() {
        let text = include_str!("chrome-for-testing.toml");
        for platform in ["linux64", "linux-arm64", "mac-arm64", "mac-x64", "win64"] {
            let source = Source::from_pins(text, platform).expect(platform);
            assert_eq!(source.sha256.len(), 64);
            assert!(source.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(source.bytes > 100_000_000);
            assert_eq!(source.url.scheme(), "https");
            assert!(source
                .url
                .path()
                .ends_with(&format!("/{platform}/chrome-{platform}.zip")));
            assert!(source.program.starts_with(format!("chrome-{platform}")));
        }
        assert!(Source::from_pins(text, "win32").is_none());
    }

    #[test]
    fn only_a_download_under_way_moves_on() {
        use BrowserDownloadStage as S;
        use DownloadEvent as E;
        assert_eq!(next(S::Downloading, E::Received), S::Checking);
        assert_eq!(next(S::Checking, E::Verified), S::Unpacking);
        assert_eq!(next(S::Unpacking, E::Unpacked), S::Done);
        assert_eq!(next(S::Downloading, E::Cancelled), S::Cancelled);
        assert_eq!(
            next(S::Checking, E::Cancelled),
            S::Checking,
            "checking runs to its end"
        );
        assert_eq!(next(S::Unpacking, E::Failed), S::Failed);
        assert_eq!(next(S::Done, E::Failed), S::Done);
        assert_eq!(next(S::Cancelled, E::Received), S::Cancelled);
    }
}
