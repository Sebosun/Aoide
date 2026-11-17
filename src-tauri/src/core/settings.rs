//! App settings: what they hold, and saving a change to them.

use super::*;

/// User preferences that are not part of an engine profile.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AppSettings {
    /// Look books up on Open Library when the user asks for details.
    pub metadata_lookup: bool,
    /// Delete the oldest generated audio above this many bytes.
    pub cache_limit_bytes: Option<u64>,
    /// Local forced-alignment service for exact word timings; `None` uses Aoide's estimate.
    pub aligner: Option<AlignerSettings>,
    /// Library order: "recent", "title", "author" or "progress".
    pub library_sort: LibrarySort,
    /// The tags the library shows only, and those it leaves out.
    pub library_tags: TagFilter,
    /// Tags from a local language model, when turned on.
    pub tagging: super::TaggingSettings,
    /// Folders searched by "Find books".
    pub scan: super::discovery::ScanSettings,
    /// Carry on preparing queued books when Aoide starts.
    pub resume_prepare: bool,
    /// Speech recognition for syncing recordings.
    pub recognition: super::recording::RecognitionSettings,
    /// Where "Find audiobooks" looks.
    pub audiobooks: super::audiobooks::AudiobookSearchSettings,
    /// Pair found audiobooks without asking: each book's closest audiobooks are tried in turn
    /// until one passes the quick check.
    pub verify_audiobooks: bool,
    /// Reading speed in words per minute, for reading-time estimates (100–600).
    pub reading_wpm: u32,
    /// How often feeds are checked and how long their entries are kept.
    pub feeds: super::feeds::FeedSettings,
    /// Whether and how a browser fetches articles plain HTTP cannot read.
    pub browser: super::browsing::BrowserSettings,
    /// The silences narration leaves between paragraphs, at scene breaks and after headings.
    pub pauses: PauseSettings,
    /// After a break, play on a little earlier: from the sentence's start, or a sentence or two
    /// back, the longer the break.
    pub step_back_on_resume: bool,
    /// Tell the desktop when a long job ends, or one needs the user, while Aoide is not in front.
    pub notices: bool,
    /// A folder (Downloads, or one a sync tool fills) whose books are added to the library as
    /// they appear; never the library folder or one inside it.
    pub inbox: Option<PathBuf>,
    /// Archive an article once it is finished; else finishing one offers to.
    pub archive_finished_articles: bool,
    /// What closing the window while working does.
    pub close: super::CloseBehaviour,
    /// Automatic backups.
    pub backup: super::backup::BackupSettings,
}

/// The silences narration leaves, in milliseconds (each within [`PAUSE_MS`]). The largest that
/// applies after a sentence is the one heard; they never add up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PauseSettings {
    /// After each paragraph.
    pub paragraph_ms: u32,
    /// At a scene break (`* * *`, a rule).
    pub break_ms: u32,
    /// After a heading, and at a chapter's end.
    pub heading_ms: u32,
}

/// Lengths a pause can be set to, in milliseconds.
pub const PAUSE_MS: std::ops::RangeInclusive<u32> = 0..=3000;

impl Default for PauseSettings {
    fn default() -> Self {
        let options = aoide_text::SegmentOptions::default();
        Self {
            paragraph_ms: options.paragraph_pause_ms,
            break_ms: options.break_pause_ms,
            heading_ms: options.heading_pause_ms,
        }
    }
}

impl PauseSettings {
    /// How books are split into segments with these pauses.
    pub fn segment_options(&self) -> aoide_text::SegmentOptions {
        aoide_text::SegmentOptions {
            paragraph_pause_ms: self.paragraph_ms,
            break_pause_ms: self.break_ms,
            heading_pause_ms: self.heading_ms,
            ..Default::default()
        }
    }
}

/// Reading speeds Settings offers, in words per minute.
pub const READING_WPM: std::ops::RangeInclusive<u32> = 100..=600;

/// A change to some of the [`AppSettings`]: fields left out keep their value; `null` clears an
/// optional one. The core merges it into the saved settings, so changes from different places
/// never overwrite each other.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AppSettingsPatch {
    #[cfg_attr(test, ts(optional))]
    pub metadata_lookup: Option<bool>,
    #[serde(deserialize_with = "present")]
    #[cfg_attr(test, ts(optional))]
    pub cache_limit_bytes: Option<Option<u64>>,
    #[serde(deserialize_with = "present")]
    #[cfg_attr(test, ts(optional))]
    pub aligner: Option<Option<AlignerSettings>>,
    #[cfg_attr(test, ts(optional))]
    pub library_sort: Option<LibrarySort>,
    #[cfg_attr(test, ts(optional))]
    pub library_tags: Option<TagFilter>,
    #[cfg_attr(test, ts(optional))]
    pub tagging: Option<super::TaggingSettings>,
    #[cfg_attr(test, ts(optional))]
    pub scan: Option<super::discovery::ScanSettings>,
    #[cfg_attr(test, ts(optional))]
    pub resume_prepare: Option<bool>,
    #[cfg_attr(test, ts(optional))]
    pub recognition: Option<super::recording::RecognitionSettings>,
    #[cfg_attr(test, ts(optional))]
    pub audiobooks: Option<super::audiobooks::AudiobookSearchSettings>,
    #[cfg_attr(test, ts(optional))]
    pub verify_audiobooks: Option<bool>,
    #[cfg_attr(test, ts(optional))]
    pub reading_wpm: Option<u32>,
    #[cfg_attr(test, ts(optional))]
    pub feeds: Option<super::feeds::FeedSettings>,
    #[cfg_attr(test, ts(optional))]
    pub browser: Option<super::browsing::BrowserSettings>,
    #[cfg_attr(test, ts(optional))]
    pub pauses: Option<PauseSettings>,
    #[cfg_attr(test, ts(optional))]
    pub step_back_on_resume: Option<bool>,
    #[cfg_attr(test, ts(optional))]
    pub notices: Option<bool>,
    #[serde(deserialize_with = "present")]
    #[cfg_attr(test, ts(optional))]
    pub inbox: Option<Option<PathBuf>>,
    #[cfg_attr(test, ts(optional))]
    pub archive_finished_articles: Option<bool>,
    #[cfg_attr(test, ts(optional))]
    pub close: Option<super::CloseBehaviour>,
    #[cfg_attr(test, ts(optional))]
    pub backup: Option<super::backup::BackupSettings>,
}

/// A field that is present, even as `null` (which a plain `Option<Option<_>>` reads as absent).
pub(super) fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    <Option<T> as serde::Deserialize>::deserialize(deserializer).map(Some)
}

impl AppSettingsPatch {
    /// `settings` with this change made.
    pub(super) fn apply(self, mut settings: AppSettings) -> AppSettings {
        fn set<T>(field: &mut T, value: Option<T>) {
            if let Some(value) = value {
                *field = value;
            }
        }
        set(&mut settings.metadata_lookup, self.metadata_lookup);
        set(&mut settings.cache_limit_bytes, self.cache_limit_bytes);
        set(&mut settings.aligner, self.aligner);
        set(&mut settings.library_sort, self.library_sort);
        set(&mut settings.library_tags, self.library_tags);
        set(&mut settings.tagging, self.tagging);
        set(&mut settings.scan, self.scan);
        set(&mut settings.resume_prepare, self.resume_prepare);
        set(&mut settings.recognition, self.recognition);
        set(&mut settings.audiobooks, self.audiobooks);
        set(&mut settings.verify_audiobooks, self.verify_audiobooks);
        set(&mut settings.reading_wpm, self.reading_wpm);
        set(&mut settings.feeds, self.feeds);
        set(&mut settings.browser, self.browser);
        set(&mut settings.pauses, self.pauses);
        set(&mut settings.step_back_on_resume, self.step_back_on_resume);
        set(&mut settings.notices, self.notices);
        set(&mut settings.inbox, self.inbox);
        set(
            &mut settings.archive_finished_articles,
            self.archive_finished_articles,
        );
        set(&mut settings.close, self.close);
        set(&mut settings.backup, self.backup);
        settings
    }
}

impl From<AppSettings> for AppSettingsPatch {
    /// A change to every field.
    fn from(settings: AppSettings) -> Self {
        let AppSettings {
            metadata_lookup,
            cache_limit_bytes,
            aligner,
            library_sort,
            library_tags,
            tagging,
            scan,
            resume_prepare,
            recognition,
            audiobooks,
            verify_audiobooks,
            reading_wpm,
            feeds,
            browser,
            pauses,
            step_back_on_resume,
            notices,
            inbox,
            archive_finished_articles,
            close,
            backup,
        } = settings;
        Self {
            metadata_lookup: Some(metadata_lookup),
            cache_limit_bytes: Some(cache_limit_bytes),
            aligner: Some(aligner),
            library_sort: Some(library_sort),
            library_tags: Some(library_tags),
            tagging: Some(tagging),
            scan: Some(scan),
            resume_prepare: Some(resume_prepare),
            recognition: Some(recognition),
            audiobooks: Some(audiobooks),
            verify_audiobooks: Some(verify_audiobooks),
            reading_wpm: Some(reading_wpm),
            feeds: Some(feeds),
            browser: Some(browser),
            pauses: Some(pauses),
            step_back_on_resume: Some(step_back_on_resume),
            notices: Some(notices),
            inbox: Some(inbox),
            archive_finished_articles: Some(archive_finished_articles),
            close: Some(close),
            backup: Some(backup),
        }
    }
}

/// The library by its items' tags: with `include`, only items with one of them; never items
/// with one of `exclude`. Names compare ignoring case.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TagFilter {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

/// Library order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum LibrarySort {
    Title,
    Author,
    Progress,
    /// By series and place in it; books in no series after.
    Series,
    /// Also used for an order this version does not know.
    #[default]
    #[serde(other)]
    Recent,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            metadata_lookup: true,
            cache_limit_bytes: None,
            aligner: None,
            library_sort: LibrarySort::Recent,
            library_tags: TagFilter::default(),
            tagging: Default::default(),
            scan: Default::default(),
            resume_prepare: true,
            recognition: Default::default(),
            audiobooks: Default::default(),
            verify_audiobooks: false,
            reading_wpm: 230,
            feeds: Default::default(),
            browser: Default::default(),
            pauses: Default::default(),
            step_back_on_resume: true,
            notices: true,
            inbox: None,
            archive_finished_articles: false,
            close: super::CloseBehaviour::Ask,
            backup: Default::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AlignerSettings {
    pub base_url: String,
    #[serde(default)]
    pub network: NetworkScope,
    /// Optional command that starts the aligner when it is not running.
    #[serde(default)]
    pub launch: Option<LaunchCommand>,
}

impl Core {
    pub fn app_settings(&self) -> AppSettings {
        self.settings.app()
    }

    /// Replaces every app setting (for tests and smoke runs; the window sends patches).
    #[cfg(any(test, debug_assertions))]
    pub fn save_app_settings(&self, settings: AppSettings) -> Result<(), String> {
        self.update_app_settings(settings.into()).map(drop)
    }

    /// Merges `patch` into the saved app settings, checks and saves them, and does what the
    /// change asks for (see [`super::config::ConfigHost::app_changed`]). Returns the settings
    /// now saved; an invalid change saves nothing.
    pub fn update_app_settings(&self, patch: AppSettingsPatch) -> Result<AppSettings, String> {
        self.settings.update_app(patch, self)
    }
}

/// The names of the settings that differ between `before` and `after` (never their values).
pub(super) fn changed_keys(before: &impl Serialize, after: &impl Serialize) -> String {
    let (Ok(serde_json::Value::Object(before)), Ok(serde_json::Value::Object(after))) =
        (serde_json::to_value(before), serde_json::to_value(after))
    else {
        return String::new();
    };
    after
        .iter()
        .filter(|(key, value)| before.get(*key) != Some(value))
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    #[test]
    fn settings_saved_before_resume_prepare_existed_keep_resuming() {
        let old: super::AppSettings =
            serde_json::from_str(r#"{"metadataLookup":false,"librarySort":"title"}"#).unwrap();
        assert!(old.resume_prepare);
        assert!(!old.metadata_lookup);
        let off: super::AppSettings = serde_json::from_str(r#"{"resumePrepare":false}"#).unwrap();
        assert!(!off.resume_prepare);
        assert!(old.step_back_on_resume && off.step_back_on_resume);
        assert!(old.notices, "notifications are on unless turned off");
    }
}
