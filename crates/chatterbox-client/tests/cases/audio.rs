use super::*;

fn wave(format: u16, bits: u16, channels: u16, data: &[u8]) -> Vec<u8> {
    let mut bytes = support::wav();
    bytes.truncate(44);
    bytes[20..22].copy_from_slice(&format.to_le_bytes());
    bytes[22..24].copy_from_slice(&channels.to_le_bytes());
    let alignment = channels * (bits / 8);
    bytes[28..32].copy_from_slice(&(24000 * u32::from(alignment)).to_le_bytes());
    bytes[32..34].copy_from_slice(&alignment.to_le_bytes());
    bytes[34..36].copy_from_slice(&bits.to_le_bytes());
    bytes[40..44].copy_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(data);
    bytes[4..8].copy_from_slice(&(36 + data.len() as u32).to_le_bytes());
    bytes
}

#[tokio::test]
async fn accepts_pcm_formats_and_stereo_with_correct_frame_counts() {
    for (bits, channels, data, frames) in [
        (8, 1, vec![0, 128, 255, 128], 4),
        (16, 2, vec![0; 16], 4),
        (24, 1, vec![0xff, 0xff, 0x7f, 0, 0, 0x80], 2),
        (32, 1, vec![0; 8], 2),
    ] {
        let bytes = wave(1, bits, channels, &data);
        let server = Server::start(Response::wav(bytes.clone())).await;
        let client = ChatterboxClient::new(ClientOptions {
            base_url: server.url.clone(),
            ..Default::default()
        })
        .unwrap();
        let audio = client
            .generate(&SpeechRequest::new("A sentence.", "Emily.wav"))
            .await
            .unwrap();
        assert_eq!(audio.as_bytes(), bytes);
        assert_eq!(
            audio.metadata(),
            &AudioMetadata {
                sample_rate: 24000,
                channels,
                bits_per_sample: bits,
                frames,
                encoding: SampleEncoding::Pcm
            }
        );
    }
}

#[tokio::test]
async fn accepts_finite_float_wav_and_rejects_nonfinite_samples() {
    let data = [0.25f32.to_le_bytes(), (-0.5f32).to_le_bytes()].concat();
    let bytes = wave(3, 32, 1, &data);
    let server = Server::start(Response::wav(bytes.clone())).await;
    let client = ChatterboxClient::new(ClientOptions {
        base_url: server.url.clone(),
        ..Default::default()
    })
    .unwrap();
    let audio = client
        .generate(&SpeechRequest::new("A sentence.", "Emily.wav"))
        .await
        .unwrap();
    assert_eq!(audio.metadata().encoding, SampleEncoding::Float);
    assert_eq!(audio.metadata().frames, 2);
    assert_eq!(audio.as_bytes(), bytes);
    for sample in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let server = Server::start(Response::wav(wave(3, 32, 1, &sample.to_le_bytes()))).await;
        let client = ChatterboxClient::new(ClientOptions {
            base_url: server.url.clone(),
            ..Default::default()
        })
        .unwrap();
        assert!(matches!(
            client
                .generate(&SpeechRequest::new("A sentence.", "Emily.wav"))
                .await,
            Err(Error::InvalidAudio(_))
        ));
    }
}

#[tokio::test]
async fn rejects_incomplete_stereo_frames_and_preserves_valid_padded_metadata_chunks() {
    let server = Server::start(Response::wav(wave(1, 16, 2, &[0; 6]))).await;
    let client = ChatterboxClient::new(ClientOptions {
        base_url: server.url.clone(),
        ..Default::default()
    })
    .unwrap();
    assert!(matches!(
        client
            .generate(&SpeechRequest::new("A sentence.", "Emily.wav"))
            .await,
        Err(Error::InvalidAudio(_))
    ));

    let mut bytes = support::wav();
    bytes.extend_from_slice(b"JUNK\x03\0\0\0abc\0");
    let length = (bytes.len() - 8) as u32;
    bytes[4..8].copy_from_slice(&length.to_le_bytes());
    let server = Server::start(Response::wav(bytes.clone())).await;
    let client = ChatterboxClient::new(ClientOptions {
        base_url: server.url.clone(),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        client
            .generate(&SpeechRequest::new("A sentence.", "Emily.wav"))
            .await
            .unwrap()
            .as_bytes(),
        bytes
    );
}
