//! The audio output: the system player process playback renders into.

use std::sync::{Arc, Mutex};

use aoide_audio::output::{OutputCommand, PipeOutput};

use super::lock;
use super::playback::Playback;

/// Pipes playback's audio to the first of the system's players that starts.
pub(super) struct Output {
    commands: Vec<OutputCommand>,
    running: Mutex<Option<PipeOutput>>,
}

impl Output {
    pub fn new(commands: Vec<OutputCommand>) -> Self {
        Self {
            commands,
            running: Mutex::new(None),
        }
    }

    /// Starts the output for `playback` unless it runs already.
    pub fn ensure(&self, playback: &Playback) -> Result<(), String> {
        let mut running = lock(&self.running);
        if running.as_ref().is_some_and(PipeOutput::is_running) {
            return Ok(());
        }
        let started = PipeOutput::start(Arc::clone(playback.player()), &self.commands)
            .map_err(|e| {
                tracing::error!(event = "output.start", outcome = "failed", error = %e, "Cannot start the audio output");
                e.to_string()
            })?;
        tracing::info!(
            event = "output.start",
            outcome = "ok",
            latency_ms = started.latency_ms(),
            "Started the audio output"
        );
        playback.set_latency(started.latency_ms());
        *running = Some(started);
        Ok(())
    }

    /// Restarts the output if its player process died while `playback` plays.
    pub fn keep_alive(&self, playback: &Playback) {
        let dead = lock(&self.running)
            .as_ref()
            .is_some_and(|output| !output.is_running());
        if playback.is_playing() && dead {
            tracing::warn!(
                event = "output.restart",
                "The audio output stopped during playback; restarting it"
            );
            if let Err(error) = self.ensure(playback) {
                playback.fail(format!("Audio output failed: {error}"));
            }
        }
    }

    /// Stops the player process.
    pub fn close(&self) {
        *lock(&self.running) = None;
    }
}

impl super::Core {
    pub(super) fn ensure_output(&self) -> Result<(), String> {
        self.output.ensure(&self.playback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(script: &str) -> Output {
        Output::new(vec![OutputCommand {
            program: "sh".into(),
            args: vec!["-c".into(), script.into()],
            device_latency_ms: 0,
        }])
    }

    fn running(output: &Output) -> bool {
        lock(&output.running)
            .as_ref()
            .is_some_and(PipeOutput::is_running)
    }

    #[test]
    fn a_player_that_died_is_started_again_only_when_it_is_needed() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("started.txt");
        let starts = || std::fs::read_to_string(&log).unwrap().lines().count();
        let playback = Playback::new(aoide_audio::PlayerCore::new(48_000));
        let output = output(&format!("echo started >> '{}'; sleep 0.4", log.display()));
        output.ensure(&playback).unwrap();
        for _ in 0..100 {
            if !running(&output) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        assert!(!running(&output), "the player exits");

        output.keep_alive(&playback);
        assert_eq!(starts(), 1, "nothing plays, so it stays down");

        output.ensure(&playback).unwrap();
        assert_eq!(starts(), 2);
        assert!(running(&output));
        output.close();
        assert!(!running(&output));
    }
}
