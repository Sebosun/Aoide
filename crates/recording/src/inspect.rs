use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use symphonia::core::errors::Error;
use symphonia::core::meta::{
    ChapterGroup, ChapterGroupItem, StandardTag, StandardVisualKey, Tag, Visual,
};

use crate::decode::{codec_name, Source};
use crate::{drm, AudioFile, Cover, FileProblem, Inspection, Problem, RecordingChapter, Tags};

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "m4a", "m4b", "mp4", "aac", "flac", "ogg", "oga", "opus", "wav", "wma", "aa", "aax",
    "aaxc", "m4p",
];
const MP4_EXTENSIONS: &[&str] = &["m4a", "m4b", "mp4", "m4p", "aax", "aaxc"];

/// Finds the audio in `paths` (files, or folders and their immediate subfolders), checks each
/// file, and lays the playable ones out in order: by disc and track tags when every file has
/// them, otherwise by natural file-name order.
pub fn inspect(paths: &[PathBuf]) -> Inspection {
    let (mut files, problems) = probe_all(candidates(paths));
    order(&mut files);
    lay_out(files, problems)
}

/// Like [`inspect`] for audio files put in order by hand: they stay in the order given.
pub fn inspect_as_listed(files: &[PathBuf]) -> Inspection {
    let (files, problems) = probe_all(files.to_vec());
    lay_out(files, problems)
}

fn probe_all(paths: Vec<PathBuf>) -> (Vec<Probed>, Vec<FileProblem>) {
    let mut files = Vec::new();
    let mut problems = Vec::new();
    for path in paths {
        match probe(&path) {
            Ok(found) => files.push(found),
            Err(problem) => problems.push(FileProblem { path, problem }),
        }
    }
    (files, problems)
}

/// The files one after another, with their chapters on the joined timeline.
fn lay_out(files: Vec<Probed>, problems: Vec<FileProblem>) -> Inspection {
    let mut chapters = Vec::new();
    let mut start_us = 0u64;
    for found in &files {
        let start_ms = start_us / 1_000;
        if found.chapters.is_empty() {
            let title = found.file.tags.title.clone().unwrap_or_else(|| {
                found
                    .file
                    .path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
            chapters.push(RecordingChapter { title, start_ms });
        } else {
            chapters.extend(found.chapters.iter().map(|c| RecordingChapter {
                title: c.title.clone(),
                start_ms: start_ms + c.start_ms,
            }));
        }
        start_us += found.file.duration_us();
    }
    let first = files
        .first()
        .map(|f| f.file.tags.clone())
        .unwrap_or_default();
    let cover = files.first().and_then(|f| f.cover.clone());
    Inspection {
        cover,
        files: files.into_iter().map(|f| f.file).collect(),
        problems,
        chapters,
        title: first.album.or(first.title),
        author: first.album_artist.or(first.artist),
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Files named directly, and audio files in named folders and their immediate subfolders.
fn candidates(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for path in paths {
        if path.is_dir() {
            for entry in entries(path) {
                if entry.is_dir() {
                    out.extend(
                        entries(&entry)
                            .into_iter()
                            .filter(|p| p.is_file() && is_audio(p)),
                    );
                } else if is_audio(&entry) {
                    out.push(entry);
                }
            }
        } else {
            out.push(path.clone());
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|p| seen.insert(p.clone()));
    out
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default()
}

struct Probed {
    file: AudioFile,
    /// Chapters within the file.
    chapters: Vec<RecordingChapter>,
    cover: Option<Cover>,
}

/// The front cover among `visuals`, else the first.
fn cover_of<'a>(visuals: impl Iterator<Item = &'a Visual> + Clone) -> Option<Cover> {
    let front = visuals
        .clone()
        .find(|v| v.usage == Some(StandardVisualKey::FrontCover));
    let visual = front.or_else(|| visuals.clone().next())?;
    Some(Cover {
        media_type: visual
            .media_type
            .clone()
            .unwrap_or_else(|| "image/jpeg".into()),
        data: visual.data.to_vec(),
    })
}

fn probe(path: &Path) -> Result<Probed, Problem> {
    if let Some(kind) = drm::check(path).map_err(|e| Problem::Unreadable(e.to_string()))? {
        return Err(Problem::Drm(kind));
    }
    let size = std::fs::metadata(path)
        .map_err(|e| Problem::Unreadable(e.to_string()))?
        .len();
    let mut source = Source::open(path)?;
    let mut decoder = source.decoder()?;
    let mut chapters = source
        .format
        .chapters()
        .map(flatten_chapters)
        .unwrap_or_default();
    let (mut tags, mut cover) = read_tags(&mut source, true);

    // Count the playable frames from the packets, as the reader will decode them. The first
    // packet is decoded too, to catch streams this decoder cannot handle.
    let mut rate = source.params.sample_rate.unwrap_or(0);
    let mut channels = source.params.channels.as_ref().map_or(0, |c| c.count()) as u32;
    let mut frames = 0i64;
    let mut decoded_one = false;
    loop {
        let packet = match source.format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) | Err(Error::ResetRequired) => break,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(Problem::Unreadable(e.to_string())),
        };
        if packet.track_id != source.track_id {
            continue;
        }
        if !decoded_one {
            let buffer = decoder.decode(&packet).map_err(|e| match e {
                Error::Unsupported(m) => Problem::Unsupported(m.into()),
                other => Problem::Unreadable(other.to_string()),
            })?;
            rate = buffer.spec().rate();
            channels = buffer.spec().channels().count() as u32;
            decoded_one = true;
        }
        let end =
            source.frames(packet.pts.get(), rate) + source.frames(packet.dur.get() as i64, rate);
        frames = frames.max(end);
    }
    if !decoded_one || rate == 0 || frames <= 0 {
        return Err(Problem::Unreadable("no audio in the file".into()));
    }
    let codec = codec_name(source.params.codec);
    let mp4 = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MP4_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    if mp4 {
        mp4_details(path, &mut tags, &mut chapters, &mut cover);
    }
    Ok(Probed {
        file: AudioFile {
            path: path.to_owned(),
            size,
            codec,
            sample_rate: rate,
            channels,
            frames: frames as u64,
            tags,
        },
        chapters,
        cover,
    })
}

/// The tags of a file, and its cover when `cover` is asked for.
fn read_tags(source: &mut Source, cover: bool) -> (Tags, Option<Cover>) {
    let mut tags = Tags::default();
    let mut found = None;
    let mut metadata = source.format.metadata();
    loop {
        if let Some(revision) = metadata.current() {
            let per_track = revision.per_track.iter().flat_map(|t| &t.metadata.tags);
            for tag in revision.media.tags.iter().chain(per_track) {
                apply(&mut tags, tag);
            }
            if cover {
                let per_track = revision.per_track.iter().flat_map(|t| &t.metadata.visuals);
                found = found.or_else(|| cover_of(revision.media.visuals.iter().chain(per_track)));
            }
        }
        if metadata.is_latest() {
            break;
        }
        metadata.pop();
    }
    (tags, found)
}

/// What a file's headers say about it, read without decoding its audio.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Glance {
    /// From the headers, or estimated from the first packets and the file's size.
    pub duration_ms: Option<u64>,
    /// 0 when the headers do not say.
    pub channels: u32,
    pub tags: Tags,
}

/// No lossy codec gets near this many kilobits a second.
const MAX_LOSSY_KBPS: u64 = 640;
/// Packets read at most to estimate a length the headers leave out.
const GLANCE_PACKETS: usize = 64;

/// A quick look at an audio file: its length, channels and tags, from its headers (and a few
/// packets when they leave out the length). None when it is not audio this reader knows.
pub fn glance(path: &Path) -> Option<Glance> {
    let mut source = Source::open(path).ok()?;
    let (mut tags, _) = read_tags(&mut source, false);
    let channels = source.params.channels.as_ref().map_or(0, |c| c.count()) as u32;
    let size = std::fs::metadata(path).map_or(0, |m| m.len());
    let lossy = matches!(
        codec_name(source.params.codec).as_str(),
        "mp3" | "aac" | "vorbis" | "opus"
    );
    let declared = source
        .num_frames
        .filter(|&frames| frames > 0 && source.time_base.is_some())
        .map(|frames| source.frames(frames as i64, 1_000).max(0) as u64)
        .filter(|&ms| ms > 0)
        // A header that leaves most of the file out (a joined MP3 keeps its first part's): the
        // bitrate it implies is beyond any lossy codec's.
        .filter(|&ms| !lossy || size * 8 / ms <= MAX_LOSSY_KBPS);
    let duration_ms = declared.or_else(|| estimate_ms(&mut source, size));
    let mp4 = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MP4_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    if mp4 {
        mp4_tags(path, &mut tags);
    }
    Some(Glance {
        duration_ms,
        channels,
        tags,
    })
}

/// The length from the bytes per millisecond of the first packets and the file's size.
fn estimate_ms(source: &mut Source, size: u64) -> Option<u64> {
    let (mut bytes, mut first, mut end) = (0u64, None, 0i64);
    for _ in 0..GLANCE_PACKETS {
        let Ok(Some(packet)) = source.format.next_packet() else {
            break;
        };
        if packet.track_id != source.track_id {
            continue;
        }
        bytes += packet.data.len() as u64;
        let start = packet.pts.get();
        first.get_or_insert(start);
        end = end.max(start + packet.dur.get() as i64);
    }
    let ms = source.frames(end - first?, 1_000);
    (bytes > 0 && ms > 0).then(|| (u128::from(size) * ms as u128 / u128::from(bytes)) as u64)
}

/// Tags symphonia may miss in an MP4 file.
fn mp4_tags(path: &Path, tags: &mut Tags) {
    let config = mp4ameta::ReadConfig {
        read_meta_items: true,
        ..mp4ameta::ReadConfig::NONE
    };
    let Ok(tag) = mp4ameta::Tag::read_with_path(path, &config) else {
        return;
    };
    let text = |s: Option<&str>| {
        s.map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    tags.album = tags.album.take().or(text(tag.album()));
    tags.artist = tags.artist.take().or(text(tag.artist()));
    tags.album_artist = tags.album_artist.take().or(text(tag.album_artist()));
    tags.genre = tags.genre.take().or(text(tag.genre()));
}

fn apply(tags: &mut Tags, tag: &Tag) {
    let text = |s: &std::sync::Arc<String>| Some(s.trim().to_owned()).filter(|s| !s.is_empty());
    match &tag.std {
        Some(StandardTag::TrackTitle(s)) => tags.title = tags.title.take().or(text(s)),
        Some(StandardTag::Album(s)) => tags.album = tags.album.take().or(text(s)),
        Some(StandardTag::Artist(s)) => tags.artist = tags.artist.take().or(text(s)),
        Some(StandardTag::AlbumArtist(s)) => {
            tags.album_artist = tags.album_artist.take().or(text(s))
        }
        Some(StandardTag::Genre(s)) => tags.genre = tags.genre.take().or(text(s)),
        Some(StandardTag::TrackNumber(n)) => tags.track = tags.track.or(Some(*n)),
        Some(StandardTag::DiscNumber(n)) => tags.disc = tags.disc.or(Some(*n)),
        _ => {}
    }
}

fn flatten_chapters(group: &ChapterGroup) -> Vec<RecordingChapter> {
    let mut out = Vec::new();
    for item in &group.items {
        match item {
            ChapterGroupItem::Group(inner) => out.extend(flatten_chapters(inner)),
            ChapterGroupItem::Chapter(chapter) => {
                let title = chapter
                    .tags
                    .iter()
                    .find_map(|t| match &t.std {
                        Some(StandardTag::ChapterTitle(s) | StandardTag::TrackTitle(s)) => {
                            Some(s.to_string())
                        }
                        _ => None,
                    })
                    .unwrap_or_default();
                let start = chapter.start_time;
                let ms = (start.as_secs_f64().max(0.0) * 1_000.0).round() as u64;
                out.push(RecordingChapter {
                    title,
                    start_ms: ms,
                });
            }
        }
    }
    out
}

/// MP4 chapters (a QuickTime chapter track or a Nero chapter list), which symphonia does not
/// read, and tags it may have missed.
fn mp4_details(
    path: &Path,
    tags: &mut Tags,
    chapters: &mut Vec<RecordingChapter>,
    cover: &mut Option<Cover>,
) {
    let config = mp4ameta::ReadConfig {
        read_meta_items: true,
        read_image_data: cover.is_none(),
        read_chapter_list: true,
        read_chapter_track: true,
        ..mp4ameta::ReadConfig::NONE
    };
    let Ok(tag) = mp4ameta::Tag::read_with_path(path, &config) else {
        return;
    };
    let found = if tag.chapter_track().is_empty() {
        tag.chapter_list()
    } else {
        tag.chapter_track()
    };
    if chapters.is_empty() {
        *chapters = found
            .iter()
            .map(|c| RecordingChapter {
                title: c.title.clone(),
                start_ms: c.start.as_millis() as u64,
            })
            .collect();
    }
    let text = |s: Option<&str>| {
        s.map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    tags.title = tags.title.take().or(text(tag.title()));
    tags.album = tags.album.take().or(text(tag.album()));
    tags.artist = tags.artist.take().or(text(tag.artist()));
    tags.album_artist = tags.album_artist.take().or(text(tag.album_artist()));
    tags.genre = tags.genre.take().or(text(tag.genre()));
    tags.track = tags.track.or(tag.track_number().map(u64::from));
    tags.disc = tags.disc.or(tag.disc_number().map(u64::from));
    if cover.is_none() {
        *cover = tag.artwork().map(|art| Cover {
            media_type: match art.fmt {
                mp4ameta::ImgFmt::Png => "image/png",
                mp4ameta::ImgFmt::Bmp => "image/bmp",
                mp4ameta::ImgFmt::Jpeg => "image/jpeg",
            }
            .into(),
            data: art.data.to_vec(),
        });
    }
}

/// By (disc, track) when every file has a track number and no two share one; otherwise by
/// natural path order.
fn order(files: &mut [Probed]) {
    let mut keys: Vec<(u64, u64)> = files
        .iter()
        .filter_map(|f| Some((f.file.tags.disc.unwrap_or(1), f.file.tags.track?)))
        .collect();
    let tagged = keys.len() == files.len() && {
        keys.sort_unstable();
        keys.windows(2).all(|w| w[0] != w[1])
    };
    if tagged {
        files.sort_by(|a, b| {
            let key = |f: &Probed| {
                (
                    f.file.tags.disc.unwrap_or(1),
                    f.file.tags.track.unwrap_or(0),
                )
            };
            key(a)
                .cmp(&key(b))
                .then_with(|| natural(&a.file.path, &b.file.path))
        });
    } else {
        files.sort_by(|a, b| natural(&a.file.path, &b.file.path));
    }
}

/// Compares paths as people read them: letters without regard to case, digit runs by value.
fn natural(a: &Path, b: &Path) -> Ordering {
    let (a, b) = (
        a.to_string_lossy().to_lowercase(),
        b.to_string_lossy().to_lowercase(),
    );
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut digits = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        it.next();
                    }
                    digits
                };
                let (m, n) = (take(&mut x), take(&mut y));
                let (m, n) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                let order = m.len().cmp(&n.len()).then_with(|| m.cmp(n));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(c), Some(d)) => {
                if c != d {
                    return c.cmp(&d);
                }
                x.next();
                y.next();
            }
        }
    }
}
