//! An audiobook's identity, from its files' sizes and edges.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use sha2::{Digest, Sha256};

/// How much of each end of a file is read.
const EDGE: u64 = 64 * 1024;

/// SHA-256 (hex) over each file in the order given: its length, then its first and last
/// 64 KiB. The same audiobook found again, copied or moved, has the same fingerprint; a file
/// changed at either end, resized or put in another order gives another one. Only the edges
/// are read, so a long recording is identified quickly.
pub fn fingerprint(files: &[PathBuf]) -> std::io::Result<String> {
    let mut hash = Sha256::new();
    for path in files {
        let mut file = File::open(path)?;
        let len = file.metadata()?.len();
        hash.update(len.to_le_bytes());
        let mut head = Vec::new();
        (&mut file).take(EDGE.min(len)).read_to_end(&mut head)?;
        hash.update(&head);
        let tail_from = len.saturating_sub(EDGE).max(EDGE.min(len));
        file.seek(SeekFrom::Start(tail_from))?;
        let mut tail = Vec::new();
        file.take(len - tail_from).read_to_end(&mut tail)?;
        hash.update(&tail);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
