use aoide_audio::{codec, resample, AudioError, Pcm};

fn sine(rate: u32, ms: u32, freq: f32) -> Pcm {
    let n = (rate * ms / 1000) as usize;
    Pcm {
        sample_rate: rate,
        samples: (0..n)
            .map(|i| 0.4 * (i as f32 * freq * std::f32::consts::TAU / rate as f32).sin())
            .collect(),
    }
}

fn rms(s: &[f32]) -> f32 {
    (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
}

#[test]
fn opus_round_trip_keeps_rate_length_and_energy() {
    let input = sine(24_000, 1000, 220.0);
    let bytes = codec::encode(&input).unwrap();
    assert_eq!(&bytes[..4], b"OggS");
    assert!(
        bytes.len() < 8_000,
        "speech bitrate, got {} bytes",
        bytes.len()
    );
    let output = codec::decode(&bytes).unwrap();
    assert_eq!(output.sample_rate, 24_000);
    assert_eq!(output.samples.len(), input.samples.len());
    let ratio = rms(&output.samples) / rms(&input.samples);
    assert!((0.8..1.2).contains(&ratio), "energy ratio {ratio}");
}

#[test]
fn unsupported_rates_are_resampled_to_24k_before_encoding() {
    let input = sine(22_050, 500, 220.0);
    let output = codec::decode(&codec::encode(&input).unwrap()).unwrap();
    assert_eq!(output.sample_rate, 24_000);
    assert!(
        (output.samples.len() as i64 - 12_000).abs() <= 2,
        "{}",
        output.samples.len()
    );
}

#[test]
fn short_and_empty_inputs_round_trip() {
    let tiny = Pcm {
        sample_rate: 24_000,
        samples: vec![0.1; 100],
    };
    assert_eq!(
        codec::decode(&codec::encode(&tiny).unwrap())
            .unwrap()
            .samples
            .len(),
        100
    );
    let empty = Pcm {
        sample_rate: 24_000,
        samples: vec![],
    };
    assert!(codec::decode(&codec::encode(&empty).unwrap())
        .unwrap()
        .samples
        .is_empty());
}

#[test]
fn garbage_is_a_decode_error() {
    assert!(matches!(
        codec::decode(b"not an ogg file"),
        Err(AudioError::Decode(_))
    ));
}

#[test]
fn resampling_scales_length_and_keeps_constant_signals() {
    let pcm = Pcm {
        sample_rate: 24_000,
        samples: vec![0.5; 2400],
    };
    let up = resample(&pcm, 48_000);
    assert_eq!(up.sample_rate, 48_000);
    assert_eq!(up.samples.len(), 4800);
    assert!(up.samples.iter().all(|s| (s - 0.5).abs() < 1e-6));
    assert_eq!(resample(&pcm, 24_000), pcm);
}
