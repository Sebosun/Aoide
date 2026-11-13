//! Starting inside an entry, and entries whose audio arrives in pieces (long recording clips).

use aoide_audio::{Entry, Pcm, PlayerCore, Status};

const RATE: u32 = 1000; // 1 sample per millisecond.

fn entry(id: &str) -> Entry {
    Entry {
        id: id.into(),
        speakable: true,
        pause_after_ms: 0,
    }
}

fn ramp(from: usize, to: usize) -> Pcm {
    Pcm {
        sample_rate: RATE,
        samples: (from..to).map(|i| i as f32 / 100.0).collect(),
    }
}

fn render(player: &mut PlayerCore, ms: usize) -> Vec<f32> {
    let mut out = vec![9.0; ms];
    player.render(&mut out);
    out
}

fn values(from: usize, to: usize) -> Vec<f32> {
    (from..to).map(|i| i as f32 / 100.0).collect()
}

#[test]
fn seek_to_starts_inside_an_entry() {
    let mut player = PlayerCore::new(RATE);
    player.load(vec![entry("a"), entry("b")], 0);
    player.provide(0, &ramp(0, 10));
    player.provide(1, &ramp(10, 14));
    player.seek_to(0, 6);
    assert_eq!(player.position().offset_ms, 6);
    player.play();
    assert_eq!(render(&mut player, 6), values(6, 12));
    assert_eq!(player.position().index, 1);
}

#[test]
fn streamed_audio_plays_as_it_arrives_and_moves_on_once_finished() {
    let mut player = PlayerCore::new(RATE);
    player.load(vec![entry("long"), entry("next")], 0);
    player.provide(1, &ramp(100, 103));
    player.append(0, &ramp(0, 4));
    player.play();
    assert_eq!(player.position().status, Status::Playing);

    let mut out = render(&mut player, 6);
    assert_eq!(out[..4], values(0, 4)[..]);
    assert_eq!(
        out[4..],
        [0.0, 0.0],
        "waits for more instead of skipping ahead"
    );
    assert_eq!(player.position().index, 0);
    assert_eq!(player.position().status, Status::Buffering);

    player.append(0, &ramp(4, 6));
    player.finish(0);
    out = render(&mut player, 5);
    assert_eq!(out, [values(4, 6), values(100, 103)].concat());
}

#[test]
fn seeking_past_what_has_arrived_waits_for_it() {
    let mut player = PlayerCore::new(RATE);
    player.load(vec![entry("long")], 0);
    player.append(0, &ramp(0, 5));
    player.seek_to(0, 8);
    player.play();
    assert_eq!(player.position().status, Status::Buffering);
    assert!(render(&mut player, 3).iter().all(|s| *s == 0.0));

    player.append(0, &ramp(5, 12));
    player.finish(0);
    assert_eq!(render(&mut player, 4), values(8, 12));
    assert_eq!(player.position().status, Status::Finished);
}

#[test]
fn a_whole_entry_provided_at_once_is_finished() {
    let mut player = PlayerCore::new(RATE);
    player.load(vec![entry("a"), entry("b")], 0);
    player.provide(0, &ramp(0, 2));
    player.append(1, &ramp(2, 3));
    assert!(player.has_audio(0) && player.has_audio(1));
    player.play();
    assert_eq!(render(&mut player, 4), [values(0, 3), vec![0.0]].concat());
    assert_eq!(player.position().status, Status::Buffering);
}

#[test]
fn a_released_entry_streams_again_from_its_start() {
    let mut player = PlayerCore::new(RATE);
    player.load(vec![entry("a"), entry("b")], 0);
    player.append(0, &ramp(0, 3));
    player.release(0);
    assert!(!player.has_audio(0));
    player.append(0, &ramp(0, 2));
    player.play();
    assert_eq!(render(&mut player, 3), [0.0, 0.01, 0.0]);
    assert_eq!(
        player.position().status,
        Status::Buffering,
        "not finished: it waits"
    );

    player.provide(1, &ramp(10, 12));
    player.release_before(2);
    player.seek(0);
    player.append(0, &ramp(0, 2));
    assert_eq!(player.position().status, Status::Playing);
    render(&mut player, 2);
    assert_eq!(
        player.position().status,
        Status::Buffering,
        "audio released earlier is not complete when it streams again"
    );
}
