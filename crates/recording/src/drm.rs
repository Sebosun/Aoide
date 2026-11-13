//! Recognising DRM-protected audiobooks from their containers, before any decoding (a decoder
//! would only report an obscure "no track").

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::DrmKind;

/// Audible's `.aa` files carry this at offset 4.
const AA_MAGIC: u32 = 0x5790_7536;
const ASF_HEADER: [u8; 16] = [
    0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11, 0xA6, 0xD9, 0x00, 0xAA, 0x00, 0x62, 0xCE, 0x6C,
];
/// ASF Content Encryption and Extended Content Encryption objects.
const ASF_ENCRYPTION: [[u8; 16]; 2] = [
    [
        0xFB, 0xB3, 0x11, 0x22, 0x23, 0xBD, 0xD2, 0x11, 0xB4, 0xB7, 0x00, 0xC0, 0x4F, 0xB0, 0xEE,
        0x2B,
    ],
    [
        0x14, 0xE6, 0x8A, 0x29, 0x22, 0x26, 0x17, 0x4C, 0xB9, 0x35, 0xDA, 0xE0, 0x7E, 0xE9, 0x28,
        0x9C,
    ],
];
/// Largest `moov` box read when looking for protected sample entries.
const MAX_MOOV: u64 = 64 * 1024 * 1024;
const MAX_ASF_HEADER: u64 = 1024 * 1024;

/// Whether the file at `path` is DRM-protected, judged from its container.
pub fn check(path: &Path) -> std::io::Result<Option<DrmKind>> {
    let mut file = File::open(path)?;
    let mut head = [0u8; 16];
    let n = read_up_to(&mut file, &mut head)?;
    if n >= 8 && u32::from_be_bytes([head[4], head[5], head[6], head[7]]) == AA_MAGIC {
        return Ok(Some(DrmKind::Audible));
    }
    if n == 16 && head == ASF_HEADER {
        return asf(&mut file);
    }
    if n >= 12 && &head[4..8] == b"ftyp" {
        if matches!(&head[8..12], b"aax " | b"aaxc") {
            return Ok(Some(DrmKind::Audible));
        }
        return mp4(&mut file);
    }
    Ok(None)
}

fn read_up_to(file: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

fn asf(file: &mut File) -> std::io::Result<Option<DrmKind>> {
    let mut size = [0u8; 8];
    file.seek(SeekFrom::Start(16))?;
    file.read_exact(&mut size)?;
    let size = u64::from_le_bytes(size).min(MAX_ASF_HEADER);
    let mut header = vec![0u8; size as usize];
    file.seek(SeekFrom::Start(0))?;
    let n = read_up_to(file, &mut header)?;
    let header = &header[..n];
    let encrypted = ASF_ENCRYPTION
        .iter()
        .any(|guid| header.windows(16).any(|w| w == guid));
    Ok(encrypted.then_some(DrmKind::WindowsMedia))
}

/// Walks the top-level boxes to `moov`, then its tracks' sample descriptions.
fn mp4(file: &mut File) -> std::io::Result<Option<DrmKind>> {
    let length = file.metadata()?.len();
    let mut at = 0u64;
    while at + 8 <= length {
        file.seek(SeekFrom::Start(at))?;
        let mut header = [0u8; 16];
        let n = read_up_to(file, &mut header)?;
        if n < 8 {
            break;
        }
        let (size, header_len) =
            match u32::from_be_bytes([header[0], header[1], header[2], header[3]]) {
                0 => (length - at, 8),
                1 if n == 16 => (u64::from_be_bytes(header[8..16].try_into().unwrap()), 16),
                size => (u64::from(size), 8),
            };
        if size < header_len {
            break;
        }
        if &header[4..8] == b"moov" {
            let body_len = (size - header_len).min(MAX_MOOV);
            let mut body = vec![0u8; body_len as usize];
            file.seek(SeekFrom::Start(at + header_len))?;
            let n = read_up_to(file, &mut body)?;
            return Ok(protected_entry(&body[..n]));
        }
        at += size;
    }
    Ok(None)
}

/// Iterates the boxes in `bytes` as (type, body).
fn boxes(bytes: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    let mut at = 0usize;
    std::iter::from_fn(move || {
        if at + 8 > bytes.len() {
            return None;
        }
        let size = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let (size, header) = match size {
            0 => (bytes.len() - at, 8),
            1 if at + 16 <= bytes.len() => (
                u64::from_be_bytes(bytes[at + 8..at + 16].try_into().unwrap()) as usize,
                16,
            ),
            size => (size, 8),
        };
        if size < header || at + size > bytes.len() {
            return None;
        }
        let item = (&bytes[at + 4..at + 8], &bytes[at + header..at + size]);
        at += size;
        Some(item)
    })
}

fn protected_entry(container: &[u8]) -> Option<DrmKind> {
    for (kind, body) in boxes(container) {
        let found = match kind {
            b"trak" | b"mdia" | b"minf" | b"stbl" => protected_entry(body),
            // Version and flags, entry count, then one box per sample entry.
            b"stsd" if body.len() >= 8 => boxes(&body[8..]).find_map(|(entry, _)| match entry {
                b"aavd" => Some(DrmKind::Audible),
                b"drms" => Some(DrmKind::FairPlay),
                b"enca" => Some(DrmKind::Encrypted),
                _ => None,
            }),
            b"adrm" => Some(DrmKind::Audible),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}
