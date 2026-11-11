//! Mono Ogg Opus files for cached segments.

use std::io::Cursor;

use ogg::{PacketReader, PacketWriteEndInfo, PacketWriter};
use opus::{Application, Bitrate, Channels, Decoder, Encoder};

use crate::{resample, AudioError, Pcm};

const OPUS_RATES: [u32; 5] = [8_000, 12_000, 16_000, 24_000, 48_000];
const FALLBACK_RATE: u32 = 24_000;
const FRAME_MS: u32 = 20;
const BITRATE: i32 = 32_000;
const SERIAL: u32 = 0x464f_4c49; // "FOLI"

fn encode_error(error: impl std::fmt::Display) -> AudioError {
    AudioError::Encode(error.to_string())
}

fn decode_error(error: impl std::fmt::Display) -> AudioError {
    AudioError::Decode(error.to_string())
}

/// Encodes mono PCM as an Ogg Opus file (speech-tuned, ~32 kbit/s). Inputs at rates Opus does
/// not support natively are resampled to 24 kHz first.
pub fn encode(pcm: &Pcm) -> Result<Vec<u8>, AudioError> {
    let pcm = if OPUS_RATES.contains(&pcm.sample_rate) {
        pcm.clone()
    } else {
        resample(pcm, FALLBACK_RATE)
    };
    let rate = pcm.sample_rate;
    let to_48k = |samples: u64| samples * 48_000 / u64::from(rate);
    let mut encoder =
        Encoder::new(rate, Channels::Mono, Application::Voip).map_err(encode_error)?;
    encoder
        .set_bitrate(Bitrate::Bits(BITRATE))
        .map_err(encode_error)?;
    let lookahead = encoder.get_lookahead().map_err(encode_error)?.max(0) as usize;
    let pre_skip = to_48k(lookahead as u64);

    let mut head = b"OpusHead".to_vec();
    head.push(1); // version
    head.push(1); // channels
    head.extend_from_slice(&(pre_skip as u16).to_le_bytes());
    head.extend_from_slice(&rate.to_le_bytes());
    head.extend_from_slice(&0i16.to_le_bytes()); // output gain
    head.push(0); // channel mapping family
    let mut tags = b"OpusTags".to_vec();
    let vendor = b"aoide";
    tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    tags.extend_from_slice(vendor);
    tags.extend_from_slice(&0u32.to_le_bytes());

    let mut writer = PacketWriter::new(Cursor::new(Vec::new()));
    let write = |w: &mut PacketWriter<'_, Cursor<Vec<u8>>>, data: Vec<u8>, end, granule| {
        w.write_packet(data, SERIAL, end, granule)
            .map_err(encode_error)
    };
    write(&mut writer, head, PacketWriteEndInfo::EndPage, 0)?;
    write(&mut writer, tags, PacketWriteEndInfo::EndPage, 0)?;

    let frame = (rate * FRAME_MS / 1000) as usize;
    let total = pcm.samples.len() as u64;
    // Pad by the encoder delay so the decoder can drop `pre_skip` and still return every sample.
    let frames = (pcm.samples.len() + lookahead).div_ceil(frame).max(1);
    let mut buffer = vec![0u8; 4000];
    for index in 0..frames {
        let start = index * frame;
        let mut input = vec![0.0f32; frame];
        let available = pcm.samples.len().saturating_sub(start).min(frame);
        input[..available].copy_from_slice(&pcm.samples[start..start + available]);
        let len = encoder
            .encode_float(&input, &mut buffer)
            .map_err(encode_error)?;
        let last = index + 1 == frames;
        let consumed = ((index + 1) * frame) as u64;
        let granule = pre_skip + to_48k(if last { total } else { consumed });
        let end = if last {
            PacketWriteEndInfo::EndStream
        } else {
            PacketWriteEndInfo::NormalPacket
        };
        write(&mut writer, buffer[..len].to_vec(), end, granule)?;
    }
    Ok(writer.into_inner().into_inner())
}

/// Decodes a file written by [`encode`] back to mono PCM at its original (Opus-supported) rate,
/// honouring pre-skip and the final granule position so lengths match the input.
pub fn decode(bytes: &[u8]) -> Result<Pcm, AudioError> {
    let mut reader = PacketReader::new(Cursor::new(bytes));
    let head = reader
        .read_packet()
        .map_err(decode_error)?
        .ok_or_else(|| decode_error("empty stream"))?;
    let data = &head.data;
    if data.len() < 19 || &data[..8] != b"OpusHead" || data[9] != 1 {
        return Err(decode_error("not a mono Ogg Opus stream"));
    }
    let pre_skip = u64::from(u16::from_le_bytes([data[10], data[11]]));
    let input_rate = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    let rate = if OPUS_RATES.contains(&input_rate) {
        input_rate
    } else {
        48_000
    };
    let from_48k = |samples: u64| (samples * u64::from(rate) / 48_000) as usize;
    reader
        .read_packet()
        .map_err(decode_error)?
        .ok_or_else(|| decode_error("missing OpusTags"))?;

    let mut decoder = Decoder::new(rate, Channels::Mono).map_err(decode_error)?;
    let mut samples = Vec::new();
    let mut buffer = vec![0f32; 5760];
    let mut final_granule = None;
    while let Some(packet) = reader.read_packet().map_err(decode_error)? {
        let n = decoder
            .decode_float(&packet.data, &mut buffer, false)
            .map_err(decode_error)?;
        samples.extend_from_slice(&buffer[..n]);
        if packet.last_in_stream() {
            final_granule = Some(packet.absgp_page());
        }
    }
    let final_granule = final_granule.ok_or_else(|| decode_error("truncated stream"))?;
    let skip = from_48k(pre_skip).min(samples.len());
    let length = from_48k(final_granule.saturating_sub(pre_skip));
    let mut samples = samples.split_off(skip);
    samples.truncate(length);
    for sample in &mut samples {
        *sample = sample.clamp(-1.0, 1.0);
    }
    Ok(Pcm {
        sample_rate: rate,
        samples,
    })
}
