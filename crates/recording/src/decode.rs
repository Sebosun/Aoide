//! Opening files with symphonia, shared by inspection and the reader.

use std::fs::File;
use std::path::Path;

use symphonia::core::codecs::audio::well_known::*;
use symphonia::core::codecs::audio::{
    AudioCodecId, AudioCodecParameters, AudioDecoder, AudioDecoderOptions,
};
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::TimeBase;

use crate::Problem;

pub(crate) struct Source {
    pub format: Box<dyn FormatReader>,
    pub track_id: u32,
    pub params: AudioCodecParameters,
    pub time_base: Option<TimeBase>,
}

impl Source {
    pub fn open(path: &Path) -> Result<Source, Problem> {
        let file = File::open(path).map_err(|e| Problem::Unreadable(e.to_string()))?;
        let stream = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(&extension.to_ascii_lowercase());
        }
        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                stream,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| Problem::Unreadable(e.to_string()))?;
        let track = format
            .default_track(TrackType::Audio)
            .ok_or_else(|| Problem::Unreadable("no audio track".into()))?;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .cloned()
            .ok_or_else(|| Problem::Unreadable("no audio track".into()))?;
        let (track_id, time_base) = (track.id, track.time_base);
        Ok(Source {
            format,
            track_id,
            params,
            time_base,
        })
    }

    pub fn decoder(&self) -> Result<Box<dyn AudioDecoder>, Problem> {
        let codecs = symphonia::default::get_codecs();
        if codecs.get_audio_decoder(self.params.codec).is_none() {
            return Err(Problem::Unsupported(format!(
                "no decoder for {}",
                codec_name(self.params.codec)
            )));
        }
        codecs
            .make_audio_decoder(&self.params, &AudioDecoderOptions::default())
            .map_err(|e| Problem::Unsupported(e.to_string()))
    }

    /// Converts a timestamp or duration in the track's time base to frames at `rate`.
    pub fn frames(&self, ts: i64, rate: u32) -> i64 {
        match self.time_base {
            Some(tb) => {
                let (numer, denom) = (i128::from(tb.numer.get()), i128::from(tb.denom.get()));
                if numer == 1 && denom == i128::from(rate) {
                    ts
                } else {
                    (i128::from(ts) * numer * i128::from(rate) / denom) as i64
                }
            }
            None => ts,
        }
    }

    /// The inverse of [`Source::frames`].
    pub fn timestamp(&self, frames: i64, rate: u32) -> i64 {
        match self.time_base {
            Some(tb) => {
                let (numer, denom) = (i128::from(tb.numer.get()), i128::from(tb.denom.get()));
                if numer == 1 && denom == i128::from(rate) {
                    frames
                } else {
                    (i128::from(frames) * denom / (numer * i128::from(rate))) as i64
                }
            }
            None => frames,
        }
    }
}

/// A short name for a codec, also for codecs without a decoder here.
pub(crate) fn codec_name(id: AudioCodecId) -> String {
    let known = match id {
        CODEC_ID_OPUS => Some("opus"),
        CODEC_ID_WMA => Some("wma"),
        CODEC_ID_AC3 => Some("ac3"),
        CODEC_ID_EAC3 => Some("eac3"),
        CODEC_ID_SPEEX => Some("speex"),
        CODEC_ID_MUSEPACK => Some("musepack"),
        CODEC_ID_ALAC => Some("alac"),
        CODEC_ID_FLAC => Some("flac"),
        CODEC_ID_AAC => Some("aac"),
        CODEC_ID_MP3 => Some("mp3"),
        CODEC_ID_VORBIS => Some("vorbis"),
        _ => None,
    };
    if let Some(name) = known {
        return name.into();
    }
    symphonia::default::get_codecs()
        .get_audio_decoder(id)
        .map(|d| d.codec.info.short_name.to_owned())
        .unwrap_or_else(|| format!("codec {id}"))
}
