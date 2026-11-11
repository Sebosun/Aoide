#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleEncoding {
    Pcm,
    Float,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioMetadata {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub frames: u64,
    pub encoding: SampleEncoding,
}

pub struct GeneratedAudio {
    pub(crate) bytes: Vec<u8>,
    pub(crate) metadata: AudioMetadata,
}

impl GeneratedAudio {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn metadata(&self) -> &AudioMetadata {
        &self.metadata
    }
}

pub(crate) fn read(bytes: Vec<u8>) -> Result<GeneratedAudio, crate::Error> {
    validate_riff(&bytes)?;
    let mut reader = hound::WavReader::new(bytes.as_slice())
        .map_err(|e| crate::Error::InvalidAudio(e.to_string()))?;
    let spec = reader.spec();
    if spec.channels == 0 || spec.sample_rate == 0 {
        return Err(invalid("sample rate and channel count must be positive"));
    }
    let frames = u64::from(reader.duration());
    let mut samples = 0u64;
    match spec.sample_format {
        hound::SampleFormat::Int => {
            for sample in reader.samples::<i32>() {
                sample.map_err(|e| crate::Error::InvalidAudio(e.to_string()))?;
                samples += 1;
            }
        }
        hound::SampleFormat::Float => {
            for sample in reader.samples::<f32>() {
                if !sample
                    .map_err(|e| crate::Error::InvalidAudio(e.to_string()))?
                    .is_finite()
                {
                    return Err(invalid("non-finite floating-point sample"));
                }
                samples += 1;
            }
        }
    }
    if samples == 0 || !samples.is_multiple_of(u64::from(spec.channels)) {
        return Err(invalid("WAV must contain complete, nonempty sample frames"));
    }
    let metadata = AudioMetadata {
        sample_rate: spec.sample_rate,
        channels: spec.channels,
        bits_per_sample: spec.bits_per_sample,
        frames,
        encoding: match spec.sample_format {
            hound::SampleFormat::Int => SampleEncoding::Pcm,
            hound::SampleFormat::Float => SampleEncoding::Float,
        },
    };
    Ok(GeneratedAudio { bytes, metadata })
}

fn validate_riff(bytes: &[u8]) -> Result<(), crate::Error> {
    if !bytes.starts_with(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(invalid("expected a RIFF/WAVE container"));
    }
    if u64::from(word(bytes, 4)?) + 8 != bytes.len() as u64 {
        return Err(invalid("RIFF length does not match the response body"));
    }
    let mut offset = 12usize;
    let mut format_chunks = 0;
    let mut data_chunks = 0;
    while offset < bytes.len() {
        let size = word(bytes, offset + 4)? as usize;
        let end = offset
            .checked_add(8)
            .and_then(|s| s.checked_add(size))
            .and_then(|s| s.checked_add(size % 2))
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| invalid("truncated RIFF chunk"))?;
        match bytes.get(offset..offset + 4) {
            Some(b"fmt ") => format_chunks += 1,
            Some(b"data") => data_chunks += 1,
            _ => {}
        }
        offset = end;
    }
    if format_chunks != 1 || data_chunks != 1 {
        return Err(invalid("expected one format chunk and one data chunk"));
    }
    Ok(())
}

fn word(bytes: &[u8], offset: usize) -> Result<u32, crate::Error> {
    let array = bytes
        .get(offset..offset + 4)
        .and_then(|slice| slice.try_into().ok())
        .ok_or_else(|| invalid("truncated WAV header"))?;
    Ok(u32::from_le_bytes(array))
}

fn invalid(message: &str) -> crate::Error {
    crate::Error::InvalidAudio(message.into())
}
