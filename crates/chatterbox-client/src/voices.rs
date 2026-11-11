use crate::{request::valid_voice_id, Error, Voice};
use std::collections::HashSet;

#[derive(serde::Deserialize)]
struct WireVoice {
    filename: String,
    display_name: String,
}

pub(crate) fn read(bytes: &[u8]) -> Result<Vec<Voice>, Error> {
    let voices = serde_json::from_slice::<Vec<WireVoice>>(bytes)
        .map_err(|_| Error::InvalidResponse("invalid voice-list JSON".into()))?;
    let mut ids = HashSet::new();
    let mut result = Vec::new();
    for voice in voices {
        if !valid_voice_id(&voice.filename)
            || voice.display_name.trim().is_empty()
            || !ids.insert(voice.filename.clone())
        {
            return Err(Error::InvalidResponse(
                "voice list contains an invalid filename, blank name, or duplicate ID".into(),
            ));
        }
        result.push(Voice {
            id: voice.filename,
            display_name: voice.display_name,
        });
    }
    Ok(result)
}
