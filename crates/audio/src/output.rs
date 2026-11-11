//! Real-time output backends that pull from a shared [`PlayerCore`](crate::PlayerCore).
//!
//! [`PipeOutput`] streams raw mono `f32` samples to a system player process (PipeWire's
//! `pw-cat`, PulseAudio's `paplay` or ALSA's `aplay`). It needs no native development libraries
//! at build time. A writer thread renders small chunks and paces itself against the wall clock
//! so the rendered playhead stays close to what is audible.

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::{AudioError, PlayerCore};

/// A command that plays raw mono little-endian `f32` PCM from stdin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputCommand {
    pub program: String,
    pub args: Vec<String>,
    /// Extra delay the player process adds before audio is heard.
    pub device_latency_ms: u32,
}

impl OutputCommand {
    /// Known system players in preference order, for `rate` Hz mono f32.
    pub fn candidates(rate: u32) -> Vec<OutputCommand> {
        let cmd = |program: &str, args: &[String], latency| OutputCommand {
            program: program.into(),
            args: args.to_vec(),
            device_latency_ms: latency,
        };
        let r = rate.to_string();
        vec![
            cmd(
                "pw-cat",
                &[
                    "--playback".into(),
                    "--raw".into(),
                    "--format".into(),
                    "f32".into(),
                    "--rate".into(),
                    r.clone(),
                    "--channels".into(),
                    "1".into(),
                    "--latency".into(),
                    "40ms".into(),
                    "-".into(),
                ],
                40,
            ),
            cmd(
                "paplay",
                &[
                    "--raw".into(),
                    "--format=float32le".into(),
                    format!("--rate={r}"),
                    "--channels=1".into(),
                    "--latency-msec=60".into(),
                ],
                60,
            ),
            cmd(
                "aplay",
                &[
                    "-q".into(),
                    "-t".into(),
                    "raw".into(),
                    "-f".into(),
                    "FLOAT_LE".into(),
                    "-r".into(),
                    r,
                    "-c".into(),
                    "1".into(),
                    "-B".into(),
                    "60000".into(),
                ],
                60,
            ),
        ]
    }
}

/// How far ahead of the wall clock the writer may render.
const MAX_LEAD: Duration = Duration::from_millis(60);
const CHUNK: Duration = Duration::from_millis(10);

pub struct PipeOutput {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    child: Child,
    latency_ms: u32,
}

impl PipeOutput {
    /// Starts the first available command from `commands`.
    pub fn start(
        player: Arc<Mutex<PlayerCore>>,
        commands: &[OutputCommand],
    ) -> Result<Self, AudioError> {
        let rate = player
            .lock()
            .map_err(|_| AudioError::Output("player lock poisoned".into()))?
            .output_rate();
        let mut last_error = String::from("no output command configured");
        for command in commands {
            match Command::new(&command.program)
                .args(&command.args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(mut child) => {
                    // A player that rejects its arguments exits at once; try the next one.
                    std::thread::sleep(Duration::from_millis(150));
                    if let Ok(Some(status)) = child.try_wait() {
                        last_error = format!("{} exited: {status}", command.program);
                        continue;
                    }
                    let stdin = child.stdin.take().expect("stdin is piped");
                    let stop = Arc::new(AtomicBool::new(false));
                    let thread = spawn_writer(player, stdin, rate, Arc::clone(&stop));
                    return Ok(Self {
                        stop,
                        thread: Some(thread),
                        child,
                        latency_ms: command.device_latency_ms + MAX_LEAD.as_millis() as u32,
                    });
                }
                Err(error) => last_error = format!("{}: {error}", command.program),
            }
        }
        Err(AudioError::Output(last_error))
    }

    /// Approximate delay between the rendered playhead and what is audible.
    pub fn latency_ms(&self) -> u32 {
        self.latency_ms
    }

    /// Whether the writer is still running (the player process has not exited).
    pub fn is_running(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }
}

impl Drop for PipeOutput {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_writer(
    player: Arc<Mutex<PlayerCore>>,
    mut stdin: std::process::ChildStdin,
    rate: u32,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("aoide-audio-out".into())
        .spawn(move || {
            let chunk = (u64::from(rate) * CHUNK.as_millis() as u64 / 1000) as usize;
            let mut samples = vec![0f32; chunk];
            let mut bytes = Vec::with_capacity(chunk * 4);
            let started = Instant::now();
            let mut written = 0u64;
            while !stop.load(Ordering::SeqCst) {
                let ahead = Duration::from_micros(written * 1_000_000 / u64::from(rate))
                    .saturating_sub(started.elapsed());
                if ahead > MAX_LEAD {
                    std::thread::sleep(ahead - MAX_LEAD);
                    continue;
                }
                match player.lock() {
                    Ok(mut player) => player.render(&mut samples),
                    Err(_) => break,
                }
                bytes.clear();
                for sample in &samples {
                    bytes.extend_from_slice(&sample.to_le_bytes());
                }
                if stdin.write_all(&bytes).is_err() {
                    break;
                }
                written += chunk as u64;
            }
        })
        .expect("spawn audio writer thread")
}
