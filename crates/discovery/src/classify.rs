//! Telling audiobooks from music, clips and sounds by what their files say: their names, their
//! headers (length, channels, tags; read by the caller, see [`AudioFacts`]) and their size.
//!
//! Spoken audio is long, often mono or at a low bitrate, and says so in its genre or its name
//! ("Unabridged", "Read by"); an M4B is an audiobook. Music comes in tracks of a few minutes at a
//! high bitrate, in stereo, and says so in its genre or its folder ("Soundtrack"). What fits
//! neither is doubtful: listed apart, never dropped.

use crate::names::plain_words;

/// What a file's headers say, as far as the reader could tell without decoding it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioFacts {
    /// Its length, when known.
    pub duration_ms: Option<u64>,
    /// 1 for mono; 0 when unknown.
    pub channels: u32,
    pub album: Option<String>,
    /// The artist, or the album artist: for an audiobook, its author or narrator.
    pub artist: Option<String>,
    pub genre: Option<String>,
}

/// Whether some audio is an audiobook.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Verdict {
    Audiobook,
    /// Not known either way (also before its files were looked at).
    #[default]
    Doubtful,
    NotAudiobook,
}

/// How many audiobooks a [`crate::AudiobookFolder::classify`] kept, and how many it dropped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Classified {
    pub audiobooks: usize,
    pub doubtful: usize,
    pub dropped: usize,
}

/// Shorter audio, all of its files together, is a clip or a song rather than a book.
pub const MIN_AUDIOBOOK_MS: u64 = 10 * 60_000;
/// This long, it is a book (or a lecture course) rather than an album.
const LONG_MS: u64 = 2 * 60 * 60_000;
/// This long and spoken-sounding (mono, or a low bitrate), it is a book.
const SPOKEN_MS: u64 = 30 * 60_000;
/// Speech is rarely kept at a higher bitrate, music rarely at a lower one.
const SPOKEN_KBPS: u64 = 96;
const MUSIC_KBPS: u64 = 160;

/// A genre is at most this many words.
const GENRE_WORDS: usize = 3;
/// Genres that say spoken word, as words.
const SPOKEN_GENRES: &[&str] = &[
    "audiobook",
    "audio book",
    "speech",
    "spoken",
    "literature",
    "lecture",
    "fiction",
    "nonfiction",
    "non fiction",
    "biography",
    "history",
    "book",
];
/// Genres that say music, as words.
const MUSIC_GENRES: &[&str] = &[
    "rock",
    "pop",
    "metal",
    "jazz",
    "blue",
    "classical",
    "electronic",
    "electronica",
    "techno",
    "house",
    "trance",
    "dubstep",
    "edm",
    "hip hop",
    "rap",
    "country",
    "folk",
    "punk",
    "indie",
    "soundtrack",
    "ost",
    "score",
    "ambient",
    "dance",
    "reggae",
    "soul",
    "funk",
    "disco",
    "alternative",
    "instrumental",
    "chill",
    "lofi",
    "lo fi",
    "game",
    "anime",
    "k pop",
    "j pop",
    "synthwave",
    "grunge",
    "ska",
    "gospel",
    "latin pop",
];
/// Words in a file's or its folders' names that say audiobook.
const SPOKEN_WORDS: &[&str] = &["audiobook", "unabridged", "abridged", "narrated", "audible"];
/// Words in a file's or its folders' names that say music.
const MUSIC_WORDS: &[&str] = &[
    "soundtrack",
    "ost",
    "discography",
    "music",
    "remastered",
    "remaster",
    "lossless",
    "vinyl",
    "karaoke",
    "instrumental",
    "remix",
    "playlist",
    "mixtape",
    "flac",
];
/// Words that say music only as a folder's name ("Music/Artist/Album"); a book may be called
/// *The Music of the Spheres*.
const MUSIC_FOLDERS: &[&str] = &["music", "musics", "song"];

/// What is known of one audiobook's audio.
pub(crate) struct Evidence<'a> {
    /// Its names, as plain words.
    pub names: Vec<String>,
    /// The names of the folders it is in, below the searched one, as plain words.
    pub place: Vec<String>,
    /// Its files' extensions, lower case.
    pub extensions: Vec<String>,
    pub files: usize,
    pub bytes: u64,
    /// Some of its files: their size and what their headers say.
    pub heard: &'a [(u64, AudioFacts)],
    /// Shorter is not an audiobook.
    pub min_ms: u64,
}

impl Evidence<'_> {
    /// The whole length, from the files heard and how many bytes they hold per second.
    pub fn duration_ms(&self) -> Option<u64> {
        let (bytes, ms) = self.heard_rate()?;
        Some((u128::from(self.bytes) * u128::from(ms) / u128::from(bytes)) as u64)
    }

    /// The bytes and milliseconds of the files whose length is known.
    fn heard_rate(&self) -> Option<(u64, u64)> {
        let (bytes, ms) = self
            .heard
            .iter()
            .filter_map(|(bytes, facts)| facts.duration_ms.map(|ms| (*bytes, ms)))
            .fold((0u64, 0u64), |(b, m), (bytes, ms)| (b + bytes, m + ms));
        (bytes > 0 && ms > 0).then_some((bytes, ms))
    }

    /// Kilobits per second (bits per millisecond).
    fn kbps(&self) -> Option<u64> {
        self.heard_rate().map(|(bytes, ms)| bytes * 8 / ms)
    }

    fn channels(&self) -> u32 {
        self.heard
            .iter()
            .map(|(_, f)| f.channels)
            .max()
            .unwrap_or(0)
    }

    /// The genres, as plain words. A long one ("Latin 101: Learning a Classical Language") is a
    /// title put in the genre tag, and says nothing.
    fn genres(&self) -> impl Iterator<Item = String> + '_ {
        self.heard
            .iter()
            .filter_map(|(_, f)| f.genre.as_deref())
            .map(plain_words)
            .filter(|words| words.len() <= GENRE_WORDS)
            .map(|words| words.join(" "))
    }
}

/// Whether `text` (plain words joined by spaces) holds `phrase` as whole words.
fn says(text: &str, phrase: &str) -> bool {
    format!(" {text} ").contains(&format!(" {phrase} "))
}

/// The verdict on one audiobook's audio.
pub(crate) fn verdict(evidence: &Evidence) -> Verdict {
    let duration = evidence.duration_ms();
    let names = [evidence.names.as_slice(), evidence.place.as_slice()]
        .concat()
        .join(" ");
    let place = evidence.place.join(" ");
    let spoken_genre = evidence
        .genres()
        .any(|g| SPOKEN_GENRES.iter().any(|s| says(&g, s)));
    let music_genre = evidence
        .genres()
        .any(|g| MUSIC_GENRES.iter().any(|s| says(&g, s)));
    let spoken_name = SPOKEN_WORDS.iter().any(|w| says(&names, w)) || says(&names, "read by");
    let m4b = evidence.extensions.iter().any(|e| e == "m4b");
    let too_short = duration.is_some_and(|ms| ms < evidence.min_ms);

    if (spoken_genre || spoken_name || m4b) && !music_genre && !too_short {
        return Verdict::Audiobook;
    }
    let music_name = MUSIC_WORDS.iter().any(|w| says(&names, w))
        || MUSIC_FOLDERS.iter().any(|w| says(&place, w));
    if music_genre || music_name || too_short {
        return Verdict::NotAudiobook;
    }
    let Some(duration) = duration else {
        return Verdict::Doubtful;
    };
    let kbps = evidence.kbps().unwrap_or(0);
    let stereo = evidence.channels() >= 2;
    let track_ms = duration / evidence.files.max(1) as u64;
    // An album: a handful of songs, a few minutes each, in stereo at a music bitrate.
    let album = evidence.files >= 3
        && (90_000..=9 * 60_000).contains(&track_ms)
        && kbps >= MUSIC_KBPS
        && stereo
        && duration < 100 * 60_000;
    if album {
        return Verdict::NotAudiobook;
    }
    let spoken_sound = evidence.channels() == 1 || kbps <= SPOKEN_KBPS;
    if duration >= LONG_MS || (duration >= SPOKEN_MS && spoken_sound) {
        return Verdict::Audiobook;
    }
    Verdict::Doubtful
}
