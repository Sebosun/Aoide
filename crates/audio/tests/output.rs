use std::sync::{Arc, Mutex};
use std::time::Duration;

use aoide_audio::output::{OutputCommand, PipeOutput};
use aoide_audio::{Entry, Pcm, PlayerCore, Status};

#[test]
fn pipe_output_streams_rendered_samples_in_real_time() {
    let dir = std::env::temp_dir().join(format!("aoide-out-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("out.raw");
    let rate = 8_000;
    let player = Arc::new(Mutex::new(PlayerCore::new(rate)));
    {
        let mut p = player.lock().unwrap();
        p.load(
            vec![Entry {
                id: "a".into(),
                speakable: true,
                pause_after_ms: 0,
            }],
            0,
        );
        p.provide(
            0,
            &Pcm {
                sample_rate: rate,
                samples: vec![0.5; rate as usize / 5],
            },
        );
        p.play();
    }
    let command = OutputCommand {
        program: "sh".into(),
        args: vec!["-c".into(), format!("cat > '{}'", file.display())],
        device_latency_ms: 0,
    };
    let missing = OutputCommand {
        program: "aoide-no-such-player".into(),
        ..command.clone()
    };
    let output = PipeOutput::start(Arc::clone(&player), &[missing, command]).unwrap();
    assert!(output.is_running());
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(player.lock().unwrap().position().status, Status::Finished);
    drop(output);

    let raw = std::fs::read(&file).unwrap();
    let samples: Vec<f32> = raw
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    let audible = samples.iter().filter(|s| **s == 0.5).count();
    assert_eq!(
        audible,
        rate as usize / 5,
        "every sample written exactly once"
    );
    // Paced: roughly 400 ms of samples (plus the allowed lead), not a burst of seconds.
    assert!(samples.len() < rate as usize, "{} samples", samples.len());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn starting_without_any_working_player_is_an_error() {
    let player = Arc::new(Mutex::new(PlayerCore::new(8_000)));
    let missing = OutputCommand {
        program: "aoide-no-such-player".into(),
        args: vec![],
        device_latency_ms: 0,
    };
    assert!(PipeOutput::start(player, &[missing]).is_err());
}

#[test]
fn players_that_exit_immediately_fall_back_to_the_next_candidate() {
    let player = Arc::new(Mutex::new(PlayerCore::new(8_000)));
    let failing = OutputCommand {
        program: "false".into(),
        args: vec![],
        device_latency_ms: 0,
    };
    let sink = OutputCommand {
        program: "sh".into(),
        args: vec!["-c".into(), "cat > /dev/null".into()],
        device_latency_ms: 0,
    };
    let output = PipeOutput::start(Arc::clone(&player), &[failing.clone(), sink]).unwrap();
    assert!(output.is_running());
    drop(output);
    assert!(PipeOutput::start(player, &[failing]).is_err());
}

#[test]
fn candidates_prefer_pipewire() {
    let c = OutputCommand::candidates(24_000);
    assert_eq!(c[0].program, "pw-cat");
    assert!(c[0].args.contains(&"24000".to_string()));
    assert!(
        c[0].args.contains(&"--raw".to_string()),
        "stdin is raw PCM, not a sound file"
    );
}
