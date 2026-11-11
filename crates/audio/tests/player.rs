use aoide_audio::{Entry, Pcm, PlayerCore, Position, Status};

const RATE: u32 = 1000; // 1 sample per millisecond keeps the arithmetic readable.

fn entry(id: &str, pause_after_ms: u32) -> Entry {
    Entry {
        id: id.into(),
        speakable: true,
        pause_after_ms,
    }
}

fn constant(value: f32, ms: usize) -> Pcm {
    Pcm {
        sample_rate: RATE,
        samples: vec![value; ms],
    }
}

fn render(player: &mut PlayerCore, ms: usize) -> Vec<f32> {
    let mut out = vec![9.0; ms];
    player.render(&mut out);
    out
}

#[test]
fn nothing_loaded_is_stopped_and_silent() {
    let mut player = PlayerCore::new(RATE);
    assert_eq!(player.position().status, Status::Stopped);
    player.play();
    assert!(render(&mut player, 10).iter().all(|s| *s == 0.0));
}

#[test]
fn entries_play_back_to_back_with_their_pauses() {
    let mut player = PlayerCore::new(RATE);
    player.load(vec![entry("a", 0), entry("b", 3), entry("c", 0)], 0);
    player.provide(0, &constant(0.25, 5));
    player.provide(1, &constant(-0.25, 4));
    player.provide(2, &constant(0.5, 2));
    assert_eq!(
        player.position(),
        Position {
            index: 0,
            offset_ms: 0,
            status: Status::Paused
        }
    );
    player.play();
    let out = render(&mut player, 14);
    assert_eq!(
        out,
        vec![0.25, 0.25, 0.25, 0.25, 0.25, -0.25, -0.25, -0.25, -0.25, 0.0, 0.0, 0.0, 0.5, 0.5]
    );
    assert_eq!(player.position().status, Status::Finished);
}

#[test]
fn position_tracks_entry_and_offset() {
    let mut player = PlayerCore::new(RATE);
    player.load(vec![entry("a", 2), entry("b", 0)], 0);
    player.provide(0, &constant(0.1, 10));
    player.provide(1, &constant(0.1, 10));
    player.play();
    render(&mut player, 4);
    assert_eq!(
        player.position(),
        Position {
            index: 0,
            offset_ms: 4,
            status: Status::Playing
        }
    );
    render(&mut player, 7); // 10 audio + 1 of the pause
    assert_eq!(
        player.position(),
        Position {
            index: 0,
            offset_ms: 10,
            status: Status::Playing
        }
    );
    render(&mut player, 4);
    assert_eq!(
        player.position(),
        Position {
            index: 1,
            offset_ms: 3,
            status: Status::Playing
        }
    );
}

#[test]
fn missing_audio_buffers_then_resumes_where_it_stopped() {
    let mut player = PlayerCore::new(RATE);
    player.load(vec![entry("a", 0), entry("b", 0)], 0);
    player.provide(0, &constant(0.25, 3));
    player.play();
    let out = render(&mut player, 6);
    assert_eq!(out, vec![0.25, 0.25, 0.25, 0.0, 0.0, 0.0]);
    assert_eq!(
        player.position(),
        Position {
            index: 1,
            offset_ms: 0,
            status: Status::Buffering
        }
    );
    assert!(!player.has_audio(1));
    player.provide(1, &constant(0.5, 2));
    assert_eq!(render(&mut player, 3), vec![0.5, 0.5, 0.0]);
    assert_eq!(player.position().status, Status::Finished);
}

#[test]
fn unspeakable_entries_need_no_audio() {
    let mut player = PlayerCore::new(RATE);
    player.load(
        vec![
            Entry {
                id: "rule".into(),
                speakable: false,
                pause_after_ms: 2,
            },
            entry("a", 0),
        ],
        0,
    );
    player.provide(1, &constant(0.5, 2));
    player.play();
    assert_eq!(render(&mut player, 4), vec![0.0, 0.0, 0.5, 0.5]);
}

#[test]
fn pause_holds_position_and_outputs_silence() {
    let mut player = PlayerCore::new(RATE);
    player.load(vec![entry("a", 0)], 0);
    player.provide(0, &constant(0.25, 10));
    player.play();
    render(&mut player, 3);
    player.pause();
    assert!(render(&mut player, 5).iter().all(|s| *s == 0.0));
    assert_eq!(
        player.position(),
        Position {
            index: 0,
            offset_ms: 3,
            status: Status::Paused
        }
    );
    player.play();
    assert_eq!(render(&mut player, 1), vec![0.25]);
}

#[test]
fn seek_moves_to_entry_start_and_keeps_audio() {
    let mut player = PlayerCore::new(RATE);
    player.load(vec![entry("a", 0), entry("b", 0)], 0);
    player.provide(0, &constant(0.25, 10));
    player.provide(1, &constant(0.5, 10));
    player.play();
    render(&mut player, 5);
    player.seek(1);
    assert_eq!(
        player.position(),
        Position {
            index: 1,
            offset_ms: 0,
            status: Status::Playing
        }
    );
    assert_eq!(render(&mut player, 1), vec![0.5]);
    player.seek(0);
    assert_eq!(render(&mut player, 1), vec![0.25]);
    player.seek(99);
    assert_eq!(player.position().index, 1);
}

#[test]
fn provided_audio_is_resampled_to_the_output_rate() {
    let mut player = PlayerCore::new(2000);
    player.load(vec![entry("a", 0)], 0);
    player.provide(0, &constant(0.25, 10)); // 10 ms at 1 kHz
    player.play();
    let out = render(&mut player, 25);
    assert_eq!(out.iter().filter(|s| **s != 0.0).count(), 20);
}

#[test]
fn faster_speed_consumes_audio_faster_without_silencing_it() {
    let rate = 24_000;
    let mut player = PlayerCore::new(rate);
    player.load(vec![entry("a", 0)], 0);
    let tone: Vec<f32> = (0..rate as usize * 2)
        .map(|i| 0.3 * (i as f32 * 220.0 * std::f32::consts::TAU / rate as f32).sin())
        .collect();
    player.provide(
        0,
        &Pcm {
            sample_rate: rate,
            samples: tone,
        },
    );
    player.set_speed(2.0);
    assert_eq!(player.speed(), 2.0);
    player.play();
    let mut out = vec![0.0; rate as usize / 2];
    player.render(&mut out);
    let pos = player.position();
    assert!((990..=1010).contains(&pos.offset_ms), "{pos:?}");
    let energy = out[out.len() / 2..].iter().map(|s| s * s).sum::<f32>();
    assert!(energy > 1.0, "stretched output is audible: {energy}");
    player.set_speed(10.0);
    assert_eq!(player.speed(), 3.0);
}

#[test]
fn releasing_old_audio_keeps_upcoming_entries() {
    let mut player = PlayerCore::new(RATE);
    player.load(vec![entry("a", 0), entry("b", 0)], 0);
    player.provide(0, &constant(0.1, 1));
    player.provide(1, &constant(0.1, 1));
    player.release_before(1);
    assert!(!player.has_audio(0));
    assert!(player.has_audio(1));
}
