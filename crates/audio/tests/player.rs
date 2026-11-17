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
fn an_entry_changed_in_place_keeps_its_audio_and_plays_as_it_now_is() {
    let mut player = PlayerCore::new(RATE);
    let passed_over = |id: &str| Entry {
        id: id.into(),
        speakable: false,
        pause_after_ms: 0,
    };
    player.load(vec![passed_over("a"), entry("b", 0)], 0);
    player.provide(1, &constant(0.5, 2));
    player.update(0, entry("a", 1));
    player.update(9, entry("nowhere", 0));
    player.play();
    assert_eq!(
        player.position().status,
        Status::Buffering,
        "it waits for its audio now"
    );
    player.provide(0, &constant(0.25, 2));
    assert_eq!(render(&mut player, 5), vec![0.25, 0.25, 0.0, 0.5, 0.5]);
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

#[test]
fn volume_scales_the_output_and_is_clamped() {
    let mut player = PlayerCore::new(RATE);
    player.load(vec![entry("a", 0)], 0);
    player.provide(0, &constant(0.8, 100));
    player.play();

    player.set_volume(0.25);
    assert_eq!(render(&mut player, 10), vec![0.2; 10]);

    player.set_volume(4.0);
    assert_eq!(player.volume(), 1.0);
    player.set_volume(f32::NAN);
    assert_eq!(player.volume(), 1.0);
    player.set_volume(-1.0);
    assert_eq!(player.volume(), 0.0);
    assert_eq!(render(&mut player, 10), vec![0.0; 10]);
}

/// A 220 Hz tone at `rate`, `ms` long, starting `from_ms` into it (so pieces join without a jump).
fn tone(rate: u32, from_ms: usize, ms: usize) -> Pcm {
    let start = from_ms * rate as usize / 1000;
    let samples = (start..start + ms * rate as usize / 1000)
        .map(|i| 0.3 * (i as f32 * 220.0 * std::f32::consts::TAU / rate as f32).sin())
        .collect();
    Pcm {
        sample_rate: rate,
        samples,
    }
}

/// Renders in blocks of `block` samples until the playlist is finished (or `limit_ms` passes).
fn play_out(player: &mut PlayerCore, block: usize, limit_ms: usize) -> Vec<f32> {
    let rate = player.output_rate() as usize;
    let mut all = Vec::new();
    while player.position().status != Status::Finished && all.len() < limit_ms * rate / 1000 {
        let mut out = vec![0.0; block];
        player.render(&mut out);
        all.extend(out);
    }
    all
}

fn largest_step(samples: &[f32]) -> f32 {
    samples
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0, f32::max)
}

#[test]
fn stretched_speech_is_as_long_as_the_speed_makes_it_and_its_end_is_heard() {
    let rate = 24_000;
    let mut player = PlayerCore::new(rate);
    player.load(vec![entry("a", 0)], 0);
    player.provide(0, &tone(rate, 0, 3000));
    player.set_speed(1.5);
    player.play();
    let out = play_out(&mut player, 480, 5000);
    let ms = out.len() * 1000 / rate as usize;
    assert!((1900..=2100).contains(&ms), "3 s at 1.5x took {ms} ms");
    // The last of the tone is played, not cut off.
    let last = out.len() - out.iter().rev().position(|s| s.abs() > 0.05).unwrap();
    assert!(
        last * 1000 / rate as usize >= 1950,
        "sound ends at {} ms",
        last * 1000 / rate as usize
    );
}

#[test]
fn no_sound_from_before_a_seek_is_heard_after_it() {
    let rate = 24_000;
    let mut player = PlayerCore::new(rate);
    player.load(vec![entry("a", 0), entry("b", 0)], 0);
    player.provide(0, &tone(rate, 0, 2000));
    player.provide(
        1,
        &Pcm {
            sample_rate: rate,
            samples: vec![0.0; rate as usize * 2],
        },
    );
    player.set_speed(2.0);
    player.play();
    render(&mut player, 12_000);
    player.seek(1);
    let mut after = vec![1.0; 2400];
    player.render(&mut after);
    assert!(
        after.iter().all(|s| s.abs() < 0.01),
        "a tail of the old entry played after the seek"
    );
}

#[test]
fn stretched_entries_join_without_a_click_across_any_render_block() {
    let rate = 24_000;
    let mut player = PlayerCore::new(rate);
    player.load(vec![entry("a", 0), entry("b", 0)], 0);
    player.provide(0, &tone(rate, 0, 1000));
    player.provide(1, &tone(rate, 1000, 1000));
    player.set_speed(1.5);
    player.play();
    // Odd-sized blocks, as audio devices ask for.
    let out = play_out(&mut player, 397, 3000);
    let body = &out[..out.len() * 9 / 10];
    assert!(largest_step(body) < 0.1, "a jump of {}", largest_step(body));
    assert!(body.iter().all(|s| s.abs() < 0.35), "louder than the tone");
}

#[test]
fn changing_speed_mid_entry_goes_on_without_a_gap() {
    let rate = 24_000;
    let mut player = PlayerCore::new(rate);
    player.load(vec![entry("a", 0)], 0);
    player.provide(0, &tone(rate, 0, 6000));
    player.set_speed(1.5);
    player.play();
    let mut heard = Vec::new();
    for speed in [1.5, 1.0, 2.0, 1.25] {
        player.set_speed(speed);
        let mut out = vec![0.0; 6000];
        player.render(&mut out);
        heard.extend(out);
    }
    for (i, window) in heard.chunks(600).enumerate() {
        let energy: f32 = window.iter().map(|s| s * s).sum();
        assert!(energy > 5.0, "silence in window {i}: {energy}");
    }
    assert!(
        largest_step(&heard) < 0.1,
        "a jump of {}",
        largest_step(&heard)
    );
    // 250 ms at each speed: 375 + 250 + 500 + 312 ms of the tone, give or take.
    let at = player.position().offset_ms;
    assert!((1390..=1520).contains(&at), "at {at} ms");
}

#[test]
fn a_pause_set_before_an_entry_stops_where_the_one_before_it_ends() {
    let mut player = PlayerCore::new(RATE);
    player.load(vec![entry("a", 3), entry("b", 0), entry("c", 0)], 0);
    player.provide(0, &constant(0.25, 5));
    player.provide(1, &constant(0.5, 4));
    player.provide(2, &constant(0.75, 4));
    player.pause_before(Some(1));
    player.play();
    let out = render(&mut player, 12);
    assert_eq!(
        out,
        [vec![0.25; 5], vec![0.0; 7]].concat(),
        "its audio and pause, then silence"
    );
    assert_eq!(
        player.position(),
        Position {
            index: 1,
            offset_ms: 0,
            status: Status::Paused
        }
    );
    // Played again, it goes on; the pause was used up.
    player.play();
    assert_eq!(
        render(&mut player, 8),
        [vec![0.5; 4], vec![0.75; 4]].concat()
    );

    // Taken away, nothing stops.
    player.seek(0);
    player.pause_before(Some(1));
    player.pause_before(None);
    assert_eq!(render(&mut player, 9)[8], 0.5);
}

#[test]
fn a_pause_set_before_an_entry_lets_stretched_speech_end_and_plays_none_of_the_next() {
    let rate = 24_000;
    let mut player = PlayerCore::new(rate);
    player.load(vec![entry("a", 100), entry("b", 0)], 0);
    player.provide(0, &tone(rate, 0, 1000));
    player.provide(1, &constant(0.9, 1000));
    player.set_speed(1.5);
    player.pause_before(Some(1));
    player.play();
    let mut all = Vec::new();
    while player.position().status == Status::Playing && all.len() < rate as usize * 2 {
        let mut out = vec![0.0; 480];
        player.render(&mut out);
        all.extend(out);
    }
    let pos = player.position();
    assert_eq!(
        (pos.index, pos.offset_ms, pos.status),
        (1, 0, Status::Paused)
    );
    assert!(
        all.iter().all(|s| s.abs() < 0.5),
        "nothing of the next entry"
    );
    let last = all.len() - all.iter().rev().position(|s| s.abs() > 0.05).unwrap();
    assert!(
        last * 1000 / rate as usize >= 620,
        "the tone's end is heard: it stops at {} ms",
        last * 1000 / rate as usize
    );
}

#[test]
fn a_fade_ramps_the_gain_and_back_without_touching_the_volume() {
    let mut player = PlayerCore::new(RATE);
    player.load(vec![entry("a", 0)], 0);
    player.provide(0, &constant(1.0, 2000));
    player.set_volume(0.5);
    player.play();

    player.set_fade(0.2);
    let down = render(&mut player, 200);
    assert!(
        down[0] < 0.5 && down[0] > 0.45,
        "a ramp, not a jump: {}",
        down[0]
    );
    assert!(down.windows(2).all(|w| w[1] <= w[0]));
    assert!((down[199] - 0.1).abs() < 1e-6, "{}", down[199]);
    assert_eq!((player.volume(), player.fade()), (0.5, 0.2));

    player.set_fade(1.0);
    let up = render(&mut player, 200);
    assert!(up.windows(2).all(|w| w[1] >= w[0]));
    assert!((up[199] - 0.5).abs() < 1e-6, "{}", up[199]);

    // Paused, a fade takes effect at once.
    player.pause();
    player.set_fade(0.2);
    player.play();
    assert!((render(&mut player, 1)[0] - 0.1).abs() < 1e-6);
    player.set_fade(f32::NAN);
    assert_eq!(player.fade(), 1.0);
}
