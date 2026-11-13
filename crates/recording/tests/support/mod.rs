#![allow(dead_code)]

use std::path::{Path, PathBuf};

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Writes a 16-bit PCM WAV file with `channels` interleaved channels.
pub fn wav(path: &Path, rate: u32, channels: u16, frames: &[Vec<f32>]) {
    let samples: Vec<i16> = frames
        .iter()
        .flat_map(|frame| {
            frame
                .iter()
                .map(|s| (s.clamp(-1.0, 1.0) * 32767.0).round() as i16)
        })
        .collect();
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, bytes).unwrap();
}

/// A mono WAV whose samples count up, so any sample's position can be read off its value.
pub fn ramp(path: &Path, rate: u32, frames: usize, from: usize) {
    let frames: Vec<Vec<f32>> = (0..frames)
        .map(|i| vec![((from + i) % 30_000) as f32 / 32767.0])
        .collect();
    wav(path, rate, 1, &frames);
}

/// The ramp value written for position `i`, as read back (16-bit samples decode as n / 32768).
pub fn ramp_value(i: usize) -> f32 {
    (((i % 30_000) as f32 / 32767.0) * 32767.0).round() / 32768.0
}

pub fn sine(path: &Path, rate: u32, hz: f32, seconds: f32) {
    let n = (rate as f32 * seconds) as usize;
    let frames: Vec<Vec<f32>> = (0..n)
        .map(|i| vec![0.5 * (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin()])
        .collect();
    wav(path, rate, 1, &frames);
}

/// An MP4 box: big-endian size, type, body.
pub fn mp4_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out
}

/// A minimal MP4 with the given brand and one audio sample entry of type `entry`.
pub fn mp4(brand: &[u8; 4], entry: &[u8; 4]) -> Vec<u8> {
    let mut ftyp = brand.to_vec();
    ftyp.extend_from_slice(&[0, 0, 0, 0]);
    ftyp.extend_from_slice(b"isom");
    let mut stsd = vec![0, 0, 0, 0, 0, 0, 0, 1];
    stsd.extend(mp4_box(entry, &[0u8; 28]));
    let stbl = mp4_box(b"stbl", &mp4_box(b"stsd", &stsd));
    let minf = mp4_box(b"minf", &stbl);
    let mdia = mp4_box(b"mdia", &minf);
    let trak = mp4_box(b"trak", &mdia);
    let moov = mp4_box(b"moov", &trak);
    let mut file = mp4_box(b"ftyp", &ftyp);
    file.extend(moov);
    file
}
