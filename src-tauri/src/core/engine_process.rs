//! A server program Aoide starts and stops for a profile's launch command: the speech engine's
//! and the aligner's. Started when it is wanted, noticed when it ends, started again a few
//! times after a crash, stopped when idle, and never left running after Aoide.
//!
//! Its state is one [`Server`], changed only by [`next`] over named [`ServerEvent`]s.

use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use aoide_tts::LaunchCommand;
use serde::Serialize;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use super::lock;

/// Why no server of ours runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopWhy {
    /// Not started in this run (or the profile changed).
    Never,
    /// Stopped after the idle time without use.
    Idle,
    /// Ended after it had been Ready, and was not wanted when it could start again.
    Exited,
    /// Another profile is in use.
    Switched,
    /// The user chose Try again after a failure.
    Retry,
    /// The user chose Stop now.
    Asked,
    Quit,
}

/// How a server process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exit {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    /// How long it ran.
    pub ran_ms: u64,
}

/// Why Aoide gave up on the server until the user tries again or saves the profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The program could not be started at all.
    Spawn { error: String },
    /// It ended before it was ever Ready.
    Exited(Exit),
    /// Its address is taken: something else accepted a connection there before the launch
    /// (`None`), or the server ended saying the address is in use.
    PortTaken(Option<Exit>),
    /// No answer and no output for the start timeout.
    StartTimedOut,
    /// It crashed more often than the restart cap allows.
    TooManyRestarts(Exit),
}

/// What follows a stop once the process has ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum After {
    Stopped(StopWhy),
    Failed(Failure),
}

/// Crashes after Ready counted against the restart cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Restarts {
    pub count: u32,
    /// When the first counted crash happened.
    pub since_ms: i64,
}

/// The server Aoide starts, as one explicit state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Server {
    /// None of ours runs. One is launched when it is wanted.
    Stopped { why: StopWhy },
    /// Launched (`pid` once spawned); waiting for an answer of Ready. `since_ms` is the last
    /// sign of life: the launch, or an answer of Loading.
    Starting {
        pid: Option<u32>,
        restarts: Restarts,
        since_ms: i64,
    },
    /// Ours, answering.
    Running {
        pid: u32,
        restarts: Restarts,
        last_used_ms: i64,
        ready_since_ms: i64,
    },
    /// SIGTERM sent to its process group; SIGKILL at `kill_at_ms`. Then `then`.
    Stopping {
        pid: u32,
        kill_at_ms: i64,
        then: After,
    },
    /// Ours ended after it had been Ready. Launched again at `retry_at_ms` if still wanted.
    Waiting {
        restarts: Restarts,
        retry_at_ms: i64,
        exit: Exit,
    },
    /// Given up until the user says Try again or saves the profile.
    Failed { why: Failure },
    /// Another program answers at the address. Aoide uses it and never stops it.
    External,
}

impl Server {
    /// The process of ours that runs, if one does.
    pub fn pid(&self) -> Option<u32> {
        match self {
            Server::Starting { pid, .. } => *pid,
            Server::Running { pid, .. } | Server::Stopping { pid, .. } => Some(*pid),
            _ => None,
        }
    }

    /// Its name in logs and views.
    pub fn name(&self) -> &'static str {
        match self {
            Server::Stopped { .. } => "stopped",
            Server::Starting { .. } => "starting",
            Server::Running { .. } => "running",
            Server::Stopping { .. } => "stopping",
            Server::Waiting { .. } => "waiting",
            Server::Failed { .. } => "failed",
            Server::External => "external",
        }
    }
}

/// What the server's address answered when last asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// Nothing listens.
    Offline,
    /// It answers, loading its model.
    Loading,
    Ready,
    /// Something answers, but not as the engine should.
    Error,
}

/// Something that happens to the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    /// Something needs the server now.
    Wanted,
    /// One look at its address.
    Answered(Answer),
    Spawned {
        pid: u32,
    },
    SpawnFailed {
        error: String,
    },
    /// Before the launch, something accepted a connection at the address.
    AddressTaken,
    /// Process `pid`, the group leader, was reaped.
    Exited {
        pid: u32,
        exit: Exit,
    },
    /// A request was made to it.
    Used,
    /// Timers: the start timeout, idle, retry, the kill grace.
    Tick,
    /// Another profile is in use; `same_launch` when its launch command and address are the
    /// same (only a voice or a parameter changed).
    Switched {
        same_launch: bool,
    },
    /// Try again, in Settings.
    RetryChosen,
    /// Stop now, in Settings or on the engine card.
    StopChosen,
    Quit,
}

impl ServerEvent {
    /// Its name in logs.
    pub fn name(&self) -> &'static str {
        match self {
            ServerEvent::Wanted => "wanted",
            ServerEvent::Answered(_) => "answered",
            ServerEvent::Spawned { .. } => "spawned",
            ServerEvent::SpawnFailed { .. } => "spawn_failed",
            ServerEvent::AddressTaken => "address_taken",
            ServerEvent::Exited { .. } => "exited",
            ServerEvent::Used => "used",
            ServerEvent::Tick => "tick",
            ServerEvent::Switched { .. } => "switched",
            ServerEvent::RetryChosen => "retry_chosen",
            ServerEvent::StopChosen => "stop_chosen",
            ServerEvent::Quit => "quit",
        }
    }
}

/// Why a server is launched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchWhy {
    Wanted,
    /// It crashed after Ready.
    Restart,
    /// It was stopped when idle.
    IdleWake,
    /// The user chose Try again.
    Retry,
}

impl LaunchWhy {
    pub fn name(self) -> &'static str {
        match self {
            LaunchWhy::Wanted => "wanted",
            LaunchWhy::Restart => "restart",
            LaunchWhy::IdleWake => "idle_wake",
            LaunchWhy::Retry => "retry",
        }
    }
}

/// What to do after a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerAction {
    None,
    Launch {
        why: LaunchWhy,
    },
    /// SIGTERM to the process group.
    Terminate,
    /// SIGKILL to the process group.
    Kill,
}

/// The server's timings: constants in the app, shrunk in tests; the start timeout and the
/// idle time come from the launch command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timing {
    /// Quiet time (no answer, no output) after which a start is given up.
    pub start_timeout_ms: i64,
    /// From SIGTERM to SIGKILL.
    pub grace_ms: i64,
    /// From SIGTERM to SIGKILL when Aoide quits.
    pub quit_grace_ms: i64,
    /// Time without use after which the server is stopped; `None` keeps it running.
    pub idle_ms: Option<i64>,
    /// The wait before each restart after a crash.
    pub backoff_ms: [i64; 3],
    /// Crashes after Ready restarted within `restart_window_ms`; one more gives up.
    pub restart_cap: u32,
    pub restart_window_ms: i64,
    /// Ready this long, and earlier crashes no longer count.
    pub ready_reset_ms: i64,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            start_timeout_ms: 180_000,
            grace_ms: 10_000,
            quit_grace_ms: 5_000,
            idle_ms: Some(30 * 60_000),
            backoff_ms: [2_000, 10_000, 30_000],
            restart_cap: 3,
            restart_window_ms: 10 * 60_000,
            ready_reset_ms: 5 * 60_000,
        }
    }
}

impl Timing {
    pub fn grace(&self) -> Duration {
        Duration::from_millis(self.grace_ms.max(0) as u64)
    }
}

/// What [`next`] reads besides the state and the event.
#[derive(Debug, Clone, Copy)]
pub struct Ctx<'a> {
    pub now_ms: i64,
    /// Something needs the server now (decided by the host).
    pub wanted: bool,
    /// What the address answered last; `None` before the first look.
    pub last_answer: Option<Answer>,
    /// When the server last printed a line.
    pub last_output_ms: Option<i64>,
    /// Its last lines.
    pub tail: &'a [String],
    pub timing: &'a Timing,
}

/// Whether the server's last lines say its address is taken.
pub fn port_taken(tail: &[String]) -> bool {
    tail.iter().any(|line| {
        let line = line.to_ascii_lowercase();
        line.contains("address already in use")
            || line.contains("eaddrinuse")
            || line.contains("errno 98")
    })
}

/// The answer an event carries, if it is one.
fn answered(event: &ServerEvent) -> Option<Answer> {
    match event {
        ServerEvent::Answered(answer) => Some(*answer),
        _ => None,
    }
}

/// The whole state machine. Events that do not apply leave the state as it is.
pub fn next(state: Server, event: ServerEvent, ctx: &Ctx) -> (Server, ServerAction) {
    use ServerAction as A;
    use ServerEvent as E;
    let now = ctx.now_ms;
    let t = ctx.timing;
    let stop = |pid: u32, then: After, grace_ms: i64| {
        (
            Server::Stopping {
                pid,
                kill_at_ms: now + grace_ms,
                then,
            },
            A::Terminate,
        )
    };
    match (state, event) {
        // Never launched over a server that answers: that one is the user's. One answering
        // after ours failed (the user started it by hand) is used too.
        (
            Server::Stopped { .. } | Server::Failed { .. },
            E::Answered(Answer::Ready | Answer::Loading),
        ) => (Server::External, A::None),
        (Server::Stopped { why }, event @ (E::Wanted | E::Answered(_) | E::Tick))
            if (event == E::Wanted || ctx.wanted)
                && matches!(
                    answered(&event).or(ctx.last_answer),
                    Some(Answer::Offline | Answer::Error)
                )
                && why != StopWhy::Quit =>
        {
            let why = match why {
                StopWhy::Idle => LaunchWhy::IdleWake,
                StopWhy::Retry => LaunchWhy::Retry,
                StopWhy::Exited => LaunchWhy::Restart,
                _ => LaunchWhy::Wanted,
            };
            (
                Server::Starting {
                    pid: None,
                    restarts: Restarts::default(),
                    since_ms: now,
                },
                A::Launch { why },
            )
        }

        (
            Server::Starting {
                restarts, since_ms, ..
            },
            E::Spawned { pid },
        ) => (
            Server::Starting {
                pid: Some(pid),
                restarts,
                since_ms,
            },
            A::None,
        ),
        (Server::Starting { .. }, E::SpawnFailed { error }) => (
            Server::Failed {
                why: Failure::Spawn { error },
            },
            A::None,
        ),
        (Server::Starting { .. }, E::AddressTaken) => (
            Server::Failed {
                why: Failure::PortTaken(None),
            },
            A::None,
        ),
        (
            Server::Starting {
                pid: Some(pid),
                restarts,
                ..
            },
            E::Answered(Answer::Ready | Answer::Error),
        ) => (
            Server::Running {
                pid,
                restarts,
                last_used_ms: now,
                ready_since_ms: now,
            },
            A::None,
        ),
        (Server::Starting { pid, restarts, .. }, E::Answered(Answer::Loading)) => (
            Server::Starting {
                pid,
                restarts,
                since_ms: now,
            },
            A::None,
        ),
        (
            state @ Server::Starting {
                pid: Some(pid),
                since_ms,
                ..
            },
            E::Tick,
        ) => {
            let alive = since_ms.max(ctx.last_output_ms.unwrap_or(i64::MIN));
            if now - alive >= t.start_timeout_ms {
                stop(pid, After::Failed(Failure::StartTimedOut), t.grace_ms)
            } else {
                (state, A::None)
            }
        }
        // An exit before Ready is not retried: a broken command fails the same way each time.
        (
            Server::Starting {
                pid: Some(ours), ..
            },
            E::Exited { pid, exit },
        ) if pid == ours => {
            let why = if port_taken(ctx.tail) {
                Failure::PortTaken(Some(exit))
            } else {
                Failure::Exited(exit)
            };
            (Server::Failed { why }, A::None)
        }

        (
            Server::Running {
                pid,
                restarts,
                ready_since_ms,
                ..
            },
            E::Used,
        ) => (
            Server::Running {
                pid,
                restarts,
                last_used_ms: now,
                ready_since_ms,
            },
            A::None,
        ),
        (
            state @ Server::Running {
                pid,
                restarts,
                last_used_ms,
                ready_since_ms,
            },
            E::Tick,
        ) => {
            if ctx.wanted {
                let running = Server::Running {
                    pid,
                    restarts,
                    last_used_ms: now,
                    ready_since_ms,
                };
                (running, A::None)
            } else if t.idle_ms.is_some_and(|idle| now - last_used_ms >= idle) {
                stop(pid, After::Stopped(StopWhy::Idle), t.grace_ms)
            } else {
                (state, A::None)
            }
        }
        (
            Server::Running {
                pid: ours,
                restarts,
                ready_since_ms,
                ..
            },
            E::Exited { pid, exit },
        ) if pid == ours => {
            let forgiven = now - ready_since_ms >= t.ready_reset_ms
                || now - restarts.since_ms > t.restart_window_ms;
            let before = if forgiven {
                Restarts::default()
            } else {
                restarts
            };
            let restarts = Restarts {
                count: before.count + 1,
                since_ms: if before.count == 0 {
                    now
                } else {
                    before.since_ms
                },
            };
            if restarts.count > t.restart_cap {
                return (
                    Server::Failed {
                        why: Failure::TooManyRestarts(exit),
                    },
                    A::None,
                );
            }
            let step = (restarts.count as usize - 1).min(t.backoff_ms.len() - 1);
            (
                Server::Waiting {
                    restarts,
                    retry_at_ms: now + t.backoff_ms[step],
                    exit,
                },
                A::None,
            )
        }

        (
            state @ Server::Waiting {
                restarts,
                retry_at_ms,
                ..
            },
            E::Tick,
        ) => {
            if now < retry_at_ms {
                (state, A::None)
            } else if ctx.wanted {
                (
                    Server::Starting {
                        pid: None,
                        restarts,
                        since_ms: now,
                    },
                    A::Launch {
                        why: LaunchWhy::Restart,
                    },
                )
            } else {
                (
                    Server::Stopped {
                        why: StopWhy::Exited,
                    },
                    A::None,
                )
            }
        }

        (
            Server::Stopping {
                pid: ours, then, ..
            },
            E::Exited { pid, .. },
        ) if pid == ours => {
            let state = match then {
                After::Stopped(why) => Server::Stopped { why },
                After::Failed(why) => Server::Failed { why },
            };
            (state, A::None)
        }
        (state @ Server::Stopping { kill_at_ms, .. }, E::Tick) if now >= kill_at_ms => {
            (state, A::Kill)
        }

        (Server::Failed { .. }, E::RetryChosen) => (
            Server::Stopped {
                why: StopWhy::Retry,
            },
            A::None,
        ),

        (Server::External, E::Answered(Answer::Offline)) => (
            Server::Stopped {
                why: StopWhy::Never,
            },
            A::None,
        ),

        (state, E::Switched { same_launch: true }) if !matches!(state, Server::Failed { .. }) => {
            (state, A::None)
        }
        (
            Server::Stopping {
                pid, kill_at_ms, ..
            },
            E::Switched { .. },
        ) => (
            Server::Stopping {
                pid,
                kill_at_ms,
                then: After::Stopped(StopWhy::Switched),
            },
            A::None,
        ),
        (state, E::Switched { .. }) => match state.pid() {
            Some(pid) => stop(pid, After::Stopped(StopWhy::Switched), t.grace_ms),
            None if matches!(state, Server::Waiting { .. }) => (
                Server::Stopped {
                    why: StopWhy::Switched,
                },
                A::None,
            ),
            None => (
                Server::Stopped {
                    why: StopWhy::Never,
                },
                A::None,
            ),
        },

        (state @ (Server::Starting { .. } | Server::Running { .. }), E::StopChosen) => {
            match state.pid() {
                Some(pid) => stop(pid, After::Stopped(StopWhy::Asked), t.grace_ms),
                None => (state, A::None),
            }
        }
        (Server::Waiting { .. }, E::StopChosen) => (
            Server::Stopped {
                why: StopWhy::Asked,
            },
            A::None,
        ),

        (state @ (Server::Starting { .. } | Server::Running { .. }), E::Quit) => {
            match state.pid() {
                Some(pid) => stop(pid, After::Stopped(StopWhy::Quit), t.quit_grace_ms),
                None => (Server::Stopped { why: StopWhy::Quit }, A::None),
            }
        }
        (
            Server::Stopping {
                pid, kill_at_ms, ..
            },
            E::Quit,
        ) => (
            Server::Stopping {
                pid,
                kill_at_ms: kill_at_ms.min(now + t.quit_grace_ms),
                then: After::Stopped(StopWhy::Quit),
            },
            A::None,
        ),
        // Nothing is launched after a quit, however much is still wanted.
        (Server::Waiting { .. } | Server::Stopped { .. }, E::Quit) => {
            (Server::Stopped { why: StopWhy::Quit }, A::None)
        }

        (state, _) => (state, A::None),
    }
}

// ------------------------------------------------------------------ the process

/// Lines of the server's output kept for the view and for [`port_taken`].
const TAIL_LINES: usize = 40;
/// The server log stops growing at this size.
const LOG_LIMIT: u64 = 10 * 1024 * 1024;
/// How long the port check waits for a connection.
const PORT_CHECK: Duration = Duration::from_millis(300);
/// How long the reaper waits for the last of the output once the leader has ended.
const OUTPUT_DRAIN: Duration = Duration::from_millis(500);
/// How often timers are looked at.
const TICK: Duration = Duration::from_secs(1);

/// Which server: its name in logs, and the files it keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Names {
    /// `program` in `proc.*` and `engine.server.*` events.
    pub program: &'static str,
    /// The stem of its log file, pidfile and thread names.
    pub file: &'static str,
}

impl Names {
    pub const SPEECH: Names = Names {
        program: "engine",
        file: "speech-server",
    };
    pub const ALIGNER: Names = Names {
        program: "aligner",
        file: "aligner",
    };
}

/// What is launched, and the address it answers at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub launch: LaunchCommand,
    pub base_url: String,
}

impl Spec {
    /// What a profile with `launch` at `base_url` launches, if anything.
    pub fn of(launch: &Option<LaunchCommand>, base_url: &str) -> Option<Spec> {
        launch.clone().map(|launch| Spec {
            launch,
            base_url: base_url.to_owned(),
        })
    }

    /// The address the port check connects to.
    fn address(&self) -> Option<SocketAddr> {
        let url = url::Url::parse(&self.base_url).ok()?;
        let host = url.host_str()?.trim_matches(['[', ']']).to_owned();
        let port = url.port_or_known_default()?;
        (host.as_str(), port).to_socket_addrs().ok()?.next()
    }
}

/// The server's state, as the window shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum ServerState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Waiting,
    Failed,
    External,
}

/// Why the server failed, as the window shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum FailureKind {
    Spawn,
    Exited,
    PortTaken,
    StartTimedOut,
    TooManyRestarts,
}

/// Why no server runs, as the window shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum StoppedKind {
    Never,
    Idle,
    Exited,
    Switched,
    Retry,
    Asked,
    Quit,
}

impl From<StopWhy> for StoppedKind {
    fn from(why: StopWhy) -> Self {
        match why {
            StopWhy::Never => StoppedKind::Never,
            StopWhy::Idle => StoppedKind::Idle,
            StopWhy::Exited => StoppedKind::Exited,
            StopWhy::Switched => StoppedKind::Switched,
            StopWhy::Retry => StoppedKind::Retry,
            StopWhy::Asked => StoppedKind::Asked,
            StopWhy::Quit => StoppedKind::Quit,
        }
    }
}

/// The server Aoide starts for a launch command, as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ServerView {
    pub state: ServerState,
    /// Why it is stopped, while it is.
    pub stopped: Option<StoppedKind>,
    /// Why it failed, while it has.
    pub failure: Option<FailureKind>,
    /// How the failed server ended, when it ended.
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    /// Why it could not be started at all.
    pub error: Option<String>,
    /// Minutes without use before it is stopped; `None` keeps it running.
    pub idle_minutes: Option<u32>,
    /// Seconds without an answer or output before a start is given up.
    pub start_timeout_seconds: u32,
    /// Whether Try again applies.
    pub can_retry: bool,
    /// Whether Stop now applies.
    pub can_stop: bool,
    /// The name of the server's log file (in Aoide's logs folder).
    pub log_file: String,
    /// Its last lines, while it has failed.
    pub last_lines: Vec<String>,
}

/// What the server needs from the rest of the app.
pub(super) trait ServerHost: Send + Sync {
    fn now_ms(&self) -> i64;
    /// Something needs the server now.
    fn wanted(&self) -> bool;
    /// Its view changed (`None`: there is no launch command).
    fn server_changed(&self, view: Option<ServerView>);
}

/// The server's output: its last lines, how many it printed, and its log file.
#[derive(Default)]
struct Output {
    tail: VecDeque<String>,
    lines: u64,
    log: Option<LogFile>,
}

struct LogFile {
    file: std::fs::File,
    written: u64,
    /// Nothing more is written: the limit was reached or writing failed.
    closed: bool,
}

impl Output {
    fn line(&mut self, line: String, names: Names) {
        if let Some(log) = &mut self.log {
            log.write(&line, names);
        }
        self.tail.push_back(line);
        while self.tail.len() > TAIL_LINES {
            self.tail.pop_front();
        }
        self.lines += 1;
    }
}

impl LogFile {
    fn write(&mut self, line: &str, names: Names) {
        if self.closed {
            return;
        }
        let bytes = line.len() as u64 + 1;
        let written = if self.written + bytes > LOG_LIMIT {
            self.closed = true;
            writeln!(
                self.file,
                "[Aoide: this log reached 10 MB; later lines are not kept]"
            )
        } else {
            self.written += bytes;
            writeln!(self.file, "{line}")
        };
        if let Err(error) = written {
            self.closed = true;
            tracing::warn!(
                event = "engine.server.log",
                program = names.program,
                file = %format!("{}.log", names.file),
                %error,
                "Cannot write the server's log"
            );
        }
    }
}

/// A spawn for the spawner thread, and where its result goes.
struct SpawnRequest {
    command: Command,
    reply: mpsc::Sender<Result<u32, String>>,
}

/// A stop under way, for its log line.
struct Stop {
    started: Instant,
    killed: bool,
}

struct Inner {
    state: Server,
    spec: Option<Spec>,
    last_answer: Option<Answer>,
    last_output_ms: Option<i64>,
    /// Output lines seen at the last step.
    seen_lines: u64,
    stopping: Option<Stop>,
}

/// The server Aoide starts for a launch command; see the module documentation.
pub(super) struct ServerProcess {
    names: Names,
    pidfile: PathBuf,
    logs_dir: PathBuf,
    base: Timing,
    tick: Duration,
    inner: Mutex<Inner>,
    /// One step at a time, with the actions it leads to.
    steps: Mutex<()>,
    events: UnboundedSender<ServerEvent>,
    receiver: Mutex<Option<UnboundedReceiver<ServerEvent>>>,
    spawner: Mutex<mpsc::Sender<SpawnRequest>>,
    output: Arc<Mutex<Output>>,
    /// The view last handed to the host.
    sent: Mutex<Option<Option<ServerView>>>,
}

impl ServerProcess {
    /// The server for `spec`, keeping its pidfile in `data_dir` and its log in `logs_dir`.
    pub fn new(
        names: Names,
        data_dir: &Path,
        logs_dir: &Path,
        spec: Option<Spec>,
        base: Timing,
    ) -> Self {
        let pidfile = data_dir.join(format!("{}.pid", names.file));
        reclaim(&pidfile, names, base.grace());
        let (events, receiver) = tokio::sync::mpsc::unbounded_channel();
        let output = Arc::new(Mutex::new(Output::default()));
        let (spawner, requests) = mpsc::channel();
        {
            let (events, output) = (events.clone(), Arc::clone(&output));
            let grace = base.grace();
            let spawned = std::thread::Builder::new()
                .name(format!("aoide-{}", names.file))
                .spawn(move || spawn_loop(names, requests, events, output, grace));
            if let Err(error) = spawned {
                tracing::error!(event = "proc.spawn", program = names.program, outcome = "failed", %error, "Cannot start the thread that starts the server");
            }
        }
        Self {
            names,
            pidfile,
            logs_dir: logs_dir.to_owned(),
            base,
            tick: TICK,
            inner: Mutex::new(Inner {
                state: Server::Stopped {
                    why: StopWhy::Never,
                },
                spec,
                last_answer: None,
                last_output_ms: None,
                seen_lines: 0,
                stopping: None,
            }),
            steps: Mutex::new(()),
            events,
            receiver: Mutex::new(Some(receiver)),
            spawner: Mutex::new(spawner),
            output,
            sent: Mutex::new(None),
        }
    }

    /// Looks at the timers every `tick` instead of every second (tests).
    #[cfg(test)]
    pub fn with_tick(mut self, tick: Duration) -> Self {
        self.tick = tick;
        self
    }

    /// The server's log file.
    pub fn log_path(&self) -> PathBuf {
        self.logs_dir.join(format!("{}.log", self.names.file))
    }

    /// Its last lines.
    pub fn log_tail(&self) -> Vec<String> {
        lock(&self.output).tail.iter().cloned().collect()
    }

    /// Something needs the server now.
    pub fn want(&self) {
        self.send(ServerEvent::Wanted);
    }

    /// What its address answered.
    pub fn answered(&self, answer: Answer) {
        self.send(ServerEvent::Answered(answer));
    }

    /// A request was made to it.
    pub fn used(&self) {
        self.send(ServerEvent::Used);
    }

    /// Try again, after a failure.
    pub fn retry(&self) {
        self.send(ServerEvent::RetryChosen);
    }

    /// Stop now.
    pub fn stop_now(&self) {
        self.send(ServerEvent::StopChosen);
    }

    /// Launches `spec` from now on, if that is a change (as [`ServerProcess::switch`]).
    pub fn follow(&self, spec: Option<Spec>) {
        if lock(&self.inner).spec != spec {
            self.switch(spec);
        }
    }

    /// Another profile is in use, launching `spec`.
    pub fn switch(&self, spec: Option<Spec>) {
        let same_launch = {
            let mut inner = lock(&self.inner);
            let same = inner.spec == spec;
            inner.spec = spec;
            same
        };
        self.send(ServerEvent::Switched { same_launch });
    }

    fn send(&self, event: ServerEvent) {
        let _ = self.events.send(event);
    }

    /// The server's view; `None` without a launch command.
    pub fn view(&self) -> Option<ServerView> {
        let inner = lock(&self.inner);
        let spec = inner.spec.as_ref()?;
        let timing = self.timing(Some(spec));
        let (stopped, failure, exit, error) = match &inner.state {
            Server::Stopped { why } => (Some(StoppedKind::from(*why)), None, None, None),
            Server::Failed { why } => {
                let (kind, exit, error) = match why {
                    Failure::Spawn { error } => (FailureKind::Spawn, None, Some(error.clone())),
                    Failure::Exited(exit) => (FailureKind::Exited, Some(*exit), None),
                    Failure::PortTaken(exit) => (FailureKind::PortTaken, *exit, None),
                    Failure::StartTimedOut => (FailureKind::StartTimedOut, None, None),
                    Failure::TooManyRestarts(exit) => {
                        (FailureKind::TooManyRestarts, Some(*exit), None)
                    }
                };
                (None, Some(kind), exit, error)
            }
            _ => (None, None, None, None),
        };
        let state = match inner.state {
            Server::Stopped { .. } => ServerState::Stopped,
            Server::Starting { .. } => ServerState::Starting,
            Server::Running { .. } => ServerState::Running,
            Server::Stopping { .. } => ServerState::Stopping,
            Server::Waiting { .. } => ServerState::Waiting,
            Server::Failed { .. } => ServerState::Failed,
            Server::External => ServerState::External,
        };
        Some(ServerView {
            state,
            stopped,
            failure,
            exit_code: exit.and_then(|e| e.code),
            signal: exit.and_then(|e| e.signal),
            error,
            idle_minutes: timing.idle_ms.map(|ms| (ms / 60_000) as u32),
            start_timeout_seconds: (timing.start_timeout_ms / 1000) as u32,
            can_retry: state == ServerState::Failed,
            can_stop: matches!(
                state,
                ServerState::Starting | ServerState::Running | ServerState::Waiting
            ),
            log_file: format!("{}.log", self.names.file),
            last_lines: if state == ServerState::Failed {
                self.log_tail()
            } else {
                vec![]
            },
        })
    }

    /// The timings for `spec`: the start timeout and the idle time are the launch command's.
    fn timing(&self, spec: Option<&Spec>) -> Timing {
        let mut timing = self.base.clone();
        if let Some(spec) = spec {
            timing.start_timeout_ms = i64::from(spec.launch.start_timeout_seconds) * 1000;
            timing.idle_ms = (spec.launch.idle_minutes > 0)
                .then(|| i64::from(spec.launch.idle_minutes) * 60_000);
        }
        timing
    }

    /// Applies `event` with [`next`], logs the transition, tells the host, and carries out
    /// what follows (a launch leads to more events, applied in turn).
    pub fn step(&self, event: ServerEvent, host: &dyn ServerHost) {
        let _one = lock(&self.steps);
        let mut pending = Some(event);
        while let Some(event) = pending.take() {
            let now = host.now_ms();
            let wanted = host.wanted();
            let cause = event.name();
            let (before, after, action, stopped) = {
                let mut inner = lock(&self.inner);
                if let ServerEvent::Answered(answer) = event {
                    inner.last_answer = Some(answer);
                }
                if inner.spec.is_none()
                    && matches!(event, ServerEvent::Wanted | ServerEvent::Answered(_))
                {
                    continue;
                }
                let (lines, tail) = {
                    let output = lock(&self.output);
                    (
                        output.lines,
                        output.tail.iter().cloned().collect::<Vec<_>>(),
                    )
                };
                if lines != inner.seen_lines {
                    inner.seen_lines = lines;
                    inner.last_output_ms = Some(now);
                }
                let timing = self.timing(inner.spec.as_ref());
                let ctx = Ctx {
                    now_ms: now,
                    wanted,
                    last_answer: inner.last_answer,
                    last_output_ms: inner.last_output_ms,
                    tail: &tail,
                    timing: &timing,
                };
                let before = inner.state.clone();
                let (after, action) = next(before.clone(), event, &ctx);
                inner.state = after.clone();
                let stopped = match (&before, &after) {
                    (Server::Stopping { then, .. }, after) if after.name() != "stopping" => {
                        inner.stopping.take().map(|stop| (then.clone(), stop))
                    }
                    _ => None,
                };
                (before, after, action, stopped)
            };
            if before.name() != after.name() {
                self.log_stage(&before, &after, cause);
            }
            if before.pid().is_some() && after.pid().is_none() {
                let _ = std::fs::remove_file(&self.pidfile);
            }
            if let Some((then, stop)) = stopped {
                self.log_stop(&then, &stop);
            }
            self.send_view(host);
            pending = self.carry_out(action, &after);
        }
    }

    fn log_stage(&self, before: &Server, after: &Server, cause: &str) {
        let attempt = match after {
            Server::Starting { restarts, .. }
            | Server::Running { restarts, .. }
            | Server::Waiting { restarts, .. } => restarts.count,
            _ => 0,
        };
        let failure = match after {
            Server::Failed { why } => Some(crate::logging::name(failure_kind(why))),
            _ => None,
        };
        if matches!(after, Server::Failed { .. } | Server::Waiting { .. }) {
            tracing::warn!(
                event = "engine.server.stage",
                program = self.names.program,
                from = before.name(),
                to = after.name(),
                cause,
                attempt,
                failure,
                "The server stopped"
            );
        } else {
            tracing::info!(
                event = "engine.server.stage",
                program = self.names.program,
                from = before.name(),
                to = after.name(),
                cause,
                attempt,
                "The server's state changed"
            );
        }
    }

    fn log_stop(&self, then: &After, stop: &Stop) {
        let why = match then {
            After::Stopped(StopWhy::Idle) => "idle",
            After::Stopped(StopWhy::Switched) => "switched",
            After::Stopped(StopWhy::Quit) => "quit",
            After::Stopped(StopWhy::Asked) => "asked",
            After::Stopped(_) => "stopped",
            After::Failed(_) => "timeout",
        };
        tracing::info!(
            event = "engine.server.stop",
            program = self.names.program,
            why,
            outcome = if stop.killed { "killed" } else { "terminated" },
            took_ms = crate::logging::took(stop.started),
            "Stopped the server"
        );
    }

    fn send_view(&self, host: &dyn ServerHost) {
        let view = self.view();
        let mut sent = lock(&self.sent);
        if sent.as_ref() != Some(&view) {
            *sent = Some(view.clone());
            drop(sent);
            host.server_changed(view);
        }
    }

    /// Carries out `action` in `state`; a launch returns how it went.
    fn carry_out(&self, action: ServerAction, state: &Server) -> Option<ServerEvent> {
        match action {
            ServerAction::None => None,
            ServerAction::Launch { why } => Some(self.launch(why)),
            ServerAction::Terminate => {
                if let Some(pid) = state.pid() {
                    lock(&self.inner).stopping = Some(Stop {
                        started: Instant::now(),
                        killed: false,
                    });
                    process::signal(pid, process::Sig::Term);
                }
                None
            }
            ServerAction::Kill => {
                if let Some(pid) = state.pid() {
                    if let Some(stop) = &mut lock(&self.inner).stopping {
                        stop.killed = true;
                    }
                    process::signal(pid, process::Sig::Kill);
                }
                None
            }
        }
    }

    fn launch(&self, why: LaunchWhy) -> ServerEvent {
        let Some(spec) = lock(&self.inner).spec.clone() else {
            return ServerEvent::SpawnFailed {
                error: "there is no launch command".into(),
            };
        };
        if let Some(address) = spec.address() {
            if TcpStream::connect_timeout(&address, PORT_CHECK).is_ok() {
                tracing::warn!(
                    event = "proc.spawn",
                    program = self.names.program,
                    why = why.name(),
                    outcome = "address_taken",
                    "Something else listens at the server's address; not starting it"
                );
                return ServerEvent::AddressTaken;
            }
        }
        self.new_log();
        let launch = &spec.launch;
        let mut command = Command::new(&launch.program);
        command
            .args(&launch.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = launch
            .working_dir
            .as_deref()
            .filter(|d| !d.trim().is_empty())
        {
            command.current_dir(dir);
        }
        // Its own process group, so a stop reaches what a wrapper (`sh`, `uv run`) started.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        end_with_the_spawner(&mut command);
        let (reply, answer) = mpsc::channel();
        let sent = lock(&self.spawner).send(SpawnRequest { command, reply });
        let spawned = match sent {
            Ok(()) => answer
                .recv()
                .unwrap_or_else(|_| Err("the spawner thread is gone".into())),
            Err(_) => Err("the spawner thread is gone".into()),
        };
        match spawned {
            Ok(pid) => {
                tracing::info!(
                    event = "proc.spawn",
                    program = self.names.program,
                    pid,
                    why = why.name(),
                    outcome = "ok",
                    "Launched the server"
                );
                if let Err(error) = leftover::Record::of(pid, &launch.program).write(&self.pidfile)
                {
                    tracing::warn!(event = "engine.server.reclaim", program = self.names.program, outcome = "unwritten", %error, "Cannot write the server's pidfile");
                }
                ServerEvent::Spawned { pid }
            }
            Err(error) => {
                tracing::error!(
                    event = "proc.spawn",
                    program = self.names.program,
                    why = why.name(),
                    outcome = "failed",
                    %error,
                    "Could not launch the server"
                );
                ServerEvent::SpawnFailed { error }
            }
        }
    }

    /// Starts a new log for a launch; the last one becomes `<name>.previous.log`.
    fn new_log(&self) {
        let path = self.log_path();
        let previous = self
            .logs_dir
            .join(format!("{}.previous.log", self.names.file));
        let _ = std::fs::create_dir_all(&self.logs_dir);
        let _ = std::fs::rename(&path, previous);
        let file = std::fs::File::create(&path);
        let mut output = lock(&self.output);
        output.tail.clear();
        output.log = match file {
            Ok(file) => Some(LogFile {
                file,
                written: 0,
                closed: false,
            }),
            Err(error) => {
                tracing::warn!(
                    event = "engine.server.log",
                    program = self.names.program,
                    file = %crate::logging::file(&path),
                    %error,
                    "Cannot write the server's log"
                );
                None
            }
        };
    }

    /// Applies what happens to the server, and its timers, for as long as Aoide runs.
    pub async fn run(&self, host: &dyn ServerHost) {
        let Some(mut receiver) = lock(&self.receiver).take() else {
            return;
        };
        self.send_view(host);
        let mut ticks = tokio::time::interval(self.tick);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let event = tokio::select! {
                event = receiver.recv() => match event {
                    Some(event) => event,
                    None => return,
                },
                _ = ticks.tick() => ServerEvent::Tick,
            };
            self.step(event, host);
        }
    }

    /// Stops a server of ours before Aoide quits, waiting for it for the quit grace at most,
    /// then killing what is left.
    pub fn stop(&self, host: &dyn ServerHost) {
        self.step(ServerEvent::Quit, host);
        let (pid, grace) = {
            let inner = lock(&self.inner);
            let grace = self.timing(inner.spec.as_ref()).quit_grace_ms;
            (inner.state.pid(), grace)
        };
        let Some(pid) = pid else { return };
        let killed = !process::gone_within(pid, Duration::from_millis(grace.max(0) as u64));
        if killed {
            process::signal(pid, process::Sig::Kill);
            process::gone_within(pid, Duration::from_secs(1));
        }
        let _ = std::fs::remove_file(&self.pidfile);
        if let Some(mut stop) = lock(&self.inner).stopping.take() {
            stop.killed |= killed;
            self.log_stop(&After::Stopped(StopWhy::Quit), &stop);
        }
    }
}

/// Stops a server an earlier run started and lost (a crash, a `kill -9`), named by the pidfile
/// it left, and removes the file.
fn reclaim(pidfile: &Path, names: Names, grace: Duration) {
    let Some(record) = leftover::Record::read(pidfile) else {
        let _ = std::fs::remove_file(pidfile);
        return;
    };
    let ours = record.still_ours();
    tracing::warn!(
        event = "engine.server.reclaim",
        program = names.program,
        pid = record.pid,
        outcome = if ours { "stopped" } else { "stale" },
        "A server from an earlier run was left behind"
    );
    if ours {
        let started = Instant::now();
        process::signal(record.pgid, process::Sig::Term);
        let killed = !process::gone_within(record.pgid, grace);
        if killed {
            process::signal(record.pgid, process::Sig::Kill);
            process::gone_within(record.pgid, Duration::from_secs(1));
        }
        tracing::info!(
            event = "engine.server.stop",
            program = names.program,
            why = "leftover",
            outcome = if killed { "killed" } else { "terminated" },
            took_ms = crate::logging::took(started),
            "Stopped the server"
        );
    }
    let _ = std::fs::remove_file(pidfile);
}

/// On Linux, the server gets SIGTERM when the thread that started it ends: the spawner thread
/// lives as long as Aoide, so a crash or a `kill -9` of Aoide ends the server too. If Aoide was
/// gone before the signal was set up, the server does not start.
#[cfg(target_os = "linux")]
fn end_with_the_spawner(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    let parent = std::process::id();
    // SAFETY: between fork and exec this makes only async-signal-safe system calls (prctl,
    // getppid) and allocates nothing.
    unsafe {
        command.pre_exec(move || {
            rustix::process::set_parent_process_death_signal(Some(rustix::process::Signal::TERM))?;
            match rustix::process::getppid() {
                Some(pid) if pid.as_raw_nonzero().get() as u32 == parent => Ok(()),
                _ => Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe)),
            }
        });
    }
}

#[cfg(not(target_os = "linux"))]
fn end_with_the_spawner(_command: &mut Command) {}

fn failure_kind(failure: &Failure) -> FailureKind {
    match failure {
        Failure::Spawn { .. } => FailureKind::Spawn,
        Failure::Exited(_) => FailureKind::Exited,
        Failure::PortTaken(_) => FailureKind::PortTaken,
        Failure::StartTimedOut => FailureKind::StartTimedOut,
        Failure::TooManyRestarts(_) => FailureKind::TooManyRestarts,
    }
}

/// The spawner thread: starts each requested server, with readers for its output and a reaper
/// that tells `events` when it ends. Runs until the [`ServerProcess`] is dropped.
fn spawn_loop(
    names: Names,
    requests: mpsc::Receiver<SpawnRequest>,
    events: UnboundedSender<ServerEvent>,
    output: Arc<Mutex<Output>>,
    grace: Duration,
) {
    for SpawnRequest { mut command, reply } in requests {
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let _ = reply.send(Err(error.to_string()));
                continue;
            }
        };
        let pid = child.id();
        let started = Instant::now();
        let (done, drained) = mpsc::channel::<()>();
        let readers: Vec<Box<dyn std::io::Read + Send>> = [
            child
                .stdout
                .take()
                .map(|o| Box::new(o) as Box<dyn std::io::Read + Send>),
            child
                .stderr
                .take()
                .map(|e| Box::new(e) as Box<dyn std::io::Read + Send>),
        ]
        .into_iter()
        .flatten()
        .collect();
        let streams = readers.len();
        for stream in readers {
            let (output, done) = (Arc::clone(&output), done.clone());
            let _ = std::thread::Builder::new()
                .name(format!("aoide-{}-output", names.file))
                .spawn(move || read_output(stream, &output, names, done));
        }
        drop(done);
        let _ = reply.send(Ok(pid));
        let events = events.clone();
        let _ = std::thread::Builder::new()
            .name(format!("aoide-{}-wait", names.file))
            .spawn(move || {
                let status = child.wait();
                let exit = exit_of(status.ok(), started);
                // The last lines may say why it ended.
                let deadline = Instant::now() + OUTPUT_DRAIN;
                for _ in 0..streams {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if drained.recv_timeout(left).is_err() {
                        break;
                    }
                }
                process::end_group(pid, grace);
                tracing::warn!(
                    event = "proc.exit",
                    program = names.program,
                    pid,
                    code = exit.code,
                    signal = exit.signal,
                    ran_ms = exit.ran_ms,
                    file = %format!("{}.log", names.file),
                    "The server ended"
                );
                let _ = events.send(ServerEvent::Exited { pid, exit });
            });
    }
}

/// Reads one of the server's streams line by line into `output`, until it closes.
fn read_output(
    stream: Box<dyn std::io::Read + Send>,
    output: &Mutex<Output>,
    names: Names,
    _done: mpsc::Sender<()>,
) {
    let mut reader = std::io::BufReader::new(stream);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buffer)
                    .trim_end_matches(['\n', '\r'])
                    .to_owned();
                lock(output).line(line, names);
            }
        }
    }
}

fn exit_of(status: Option<std::process::ExitStatus>, started: Instant) -> Exit {
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        status.and_then(|s| s.signal())
    };
    #[cfg(not(unix))]
    let signal = None;
    Exit {
        code: status.and_then(|s| s.code()),
        signal,
        ran_ms: crate::logging::took(started),
    }
}

/// The pidfile, and servers an earlier run of Aoide lost in a crash or a `kill -9`.
mod leftover {
    use std::path::Path;

    /// What the pidfile says about the server Aoide started.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Record {
        pub pid: u32,
        pub pgid: u32,
        /// When the process started, in clock ticks since boot (Linux), to tell it from
        /// another program that got the same pid later.
        pub start: Option<u64>,
        /// The program's file name.
        pub program: String,
    }

    impl Record {
        /// The record of server `pid` (leading its own group) running `program`.
        pub fn of(pid: u32, program: &str) -> Self {
            Self {
                pid,
                pgid: pid,
                start: stat(pid).map(|s| s.start),
                program: Path::new(program)
                    .file_name()
                    .map_or_else(|| program.to_owned(), |n| n.to_string_lossy().into_owned()),
            }
        }

        pub fn write(&self, path: &Path) -> std::io::Result<()> {
            let start = self.start.map_or_else(String::new, |s| s.to_string());
            std::fs::write(
                path,
                format!(
                    "pid={}\npgid={}\nstart={start}\nprogram={}\n",
                    self.pid, self.pgid, self.program
                ),
            )
        }

        pub fn read(path: &Path) -> Option<Self> {
            let text = std::fs::read_to_string(path).ok()?;
            let field = |name: &str| {
                text.lines()
                    .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
                    .map(str::trim)
            };
            Some(Self {
                pid: field("pid")?.parse().ok()?,
                pgid: field("pgid")?.parse().ok()?,
                start: field("start").and_then(|s| s.parse().ok()),
                program: field("program").unwrap_or_default().to_owned(),
            })
        }

        /// Whether the process group it names is still the server Aoide started: its leader
        /// runs with the same start time, or the leader is gone and members it started remain.
        /// Off Linux nothing can be checked, and nothing is taken for Aoide's.
        pub fn still_ours(&self) -> bool {
            let Some(start) = self.start else {
                return false;
            };
            match stat(self.pid) {
                Some(leader) => leader.start == start && leader.pgrp == self.pgid,
                None => members_since(self.pgid, start),
            }
        }
    }

    struct Stat {
        pgrp: u32,
        start: u64,
    }

    /// Fields 5 (process group) and 22 (start time) of `/proc/<pid>/stat`.
    #[cfg(target_os = "linux")]
    fn stat(pid: u32) -> Option<Stat> {
        let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // The command name in field 2 may hold spaces and parentheses: count from the last ')'.
        let fields: Vec<&str> = text
            .get(text.rfind(')')? + 1..)?
            .split_whitespace()
            .collect();
        Some(Stat {
            pgrp: fields.get(2)?.parse().ok()?,
            start: fields.get(19)?.parse().ok()?,
        })
    }

    #[cfg(not(target_os = "linux"))]
    fn stat(_pid: u32) -> Option<Stat> {
        None
    }

    /// Whether a process of group `pgid` started at or after `since` still runs.
    #[cfg(target_os = "linux")]
    fn members_since(pgid: u32, since: u64) -> bool {
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return false;
        };
        entries
            .flatten()
            .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
            .filter_map(stat)
            .any(|s| s.pgrp == pgid && s.start >= since)
    }

    #[cfg(not(target_os = "linux"))]
    fn members_since(_pgid: u32, _since: u64) -> bool {
        false
    }
}

/// Signals to a server's process group (its leader's pid is the group's id).
mod process {
    use std::time::{Duration, Instant};

    #[derive(Debug, Clone, Copy)]
    pub enum Sig {
        Term,
        Kill,
    }

    #[cfg(unix)]
    pub fn signal(pgid: u32, sig: Sig) {
        use rustix::process::{kill_process_group, Pid, Signal};
        let Some(pgid) = Pid::from_raw(pgid as i32) else {
            return;
        };
        let sig = match sig {
            Sig::Term => Signal::TERM,
            Sig::Kill => Signal::KILL,
        };
        let _ = kill_process_group(pgid, sig);
    }

    /// Whether anything of group `pgid` still runs (or is not reaped yet).
    #[cfg(unix)]
    pub fn alive(pgid: u32) -> bool {
        rustix::process::Pid::from_raw(pgid as i32)
            .is_some_and(|pgid| rustix::process::test_kill_process_group(pgid).is_ok())
    }

    #[cfg(not(unix))]
    pub fn signal(_pgid: u32, _sig: Sig) {}

    #[cfg(not(unix))]
    pub fn alive(_pgid: u32) -> bool {
        false
    }

    /// Waits up to `limit` for group `pgid` to be gone; whether it is.
    pub fn gone_within(pgid: u32, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        while alive(pgid) {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        true
    }

    /// Ends what is left of group `pgid` once its leader has ended: SIGTERM, then SIGKILL
    /// after `grace`. A dead `uv run` with a live Python child holding the port is the case.
    pub fn end_group(pgid: u32, grace: Duration) {
        if !alive(pgid) {
            return;
        }
        signal(pgid, Sig::Term);
        if !gone_within(pgid, grace) {
            signal(pgid, Sig::Kill);
            gone_within(pgid, Duration::from_secs(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Answer::*;
    use Server::*;
    use ServerAction as A;
    use ServerEvent as E;

    const NOW: i64 = 1_000_000;

    fn exit(code: i32) -> Exit {
        Exit {
            code: Some(code),
            signal: None,
            ran_ms: 1_500,
        }
    }

    fn running(last_used_ms: i64) -> Server {
        Running {
            pid: 7,
            restarts: Restarts::default(),
            last_used_ms,
            ready_since_ms: NOW - 1_000,
        }
    }

    fn starting(since_ms: i64) -> Server {
        Starting {
            pid: Some(7),
            restarts: Restarts::default(),
            since_ms,
        }
    }

    /// The context of one row: `wanted`, the last answer, the last output and the tail.
    struct Row {
        wanted: bool,
        answer: Option<Answer>,
        output: Option<i64>,
        tail: Vec<String>,
    }

    fn quiet() -> Row {
        Row {
            wanted: false,
            answer: Some(Offline),
            output: None,
            tail: vec![],
        }
    }

    fn wanted() -> Row {
        Row {
            wanted: true,
            ..quiet()
        }
    }

    fn said(lines: &[&str]) -> Row {
        Row {
            tail: lines.iter().map(|l| l.to_string()).collect(),
            ..quiet()
        }
    }

    #[test]
    fn every_rule_of_the_server_state_machine() {
        let timing = Timing::default();
        let t = &timing;
        let crashed = exit(1);
        let stopped = |why| Stopped { why };
        let stopping = |then| Stopping {
            pid: 7,
            kill_at_ms: NOW + t.grace_ms,
            then,
        };
        let waiting = |count, since_ms, retry_in| Waiting {
            restarts: Restarts { count, since_ms },
            retry_at_ms: NOW + retry_in,
            exit: crashed,
        };
        let launched = |restarts| Starting {
            pid: None,
            restarts,
            since_ms: NOW,
        };
        let launch = |why| A::Launch { why };
        let fresh = Restarts::default();

        #[rustfmt::skip]
        let rows: Vec<(&str, Server, ServerEvent, Row, Server, ServerAction)> = vec![
            // Launch on want, never over a server that answers.
            ("want while offline", stopped(StopWhy::Never), E::Wanted, quiet(), launched(fresh), launch(LaunchWhy::Wanted)),
            ("want before any answer waits for one", stopped(StopWhy::Never), E::Wanted, Row { answer: None, ..quiet() }, stopped(StopWhy::Never), A::None),
            ("an offline answer while wanted", stopped(StopWhy::Never), E::Answered(Offline), Row { answer: None, ..wanted() }, launched(fresh), launch(LaunchWhy::Wanted)),
            ("an offline answer while not wanted", stopped(StopWhy::Never), E::Answered(Offline), quiet(), stopped(StopWhy::Never), A::None),
            ("a tick while wanted", stopped(StopWhy::Never), E::Tick, wanted(), launched(fresh), launch(LaunchWhy::Wanted)),
            ("a ready answer is someone else's", stopped(StopWhy::Never), E::Answered(Ready), wanted(), External, A::None),
            ("a loading answer is someone else's", stopped(StopWhy::Idle), E::Answered(Loading), quiet(), External, A::None),
            ("after idle, a want wakes it", stopped(StopWhy::Idle), E::Wanted, quiet(), launched(fresh), launch(LaunchWhy::IdleWake)),
            ("after try again, a want retries", stopped(StopWhy::Retry), E::Wanted, quiet(), launched(fresh), launch(LaunchWhy::Retry)),
            ("after a crash, a want restarts", stopped(StopWhy::Exited), E::Wanted, quiet(), launched(fresh), launch(LaunchWhy::Restart)),
            ("after stop now, a want starts it", stopped(StopWhy::Asked), E::Wanted, quiet(), launched(fresh), launch(LaunchWhy::Wanted)),
            // Spawning.
            ("spawned", launched(fresh), E::Spawned { pid: 7 }, quiet(), Starting { pid: Some(7), restarts: fresh, since_ms: NOW }, A::None),
            ("spawn failed", launched(fresh), E::SpawnFailed { error: "no such file".into() }, quiet(), Failed { why: Failure::Spawn { error: "no such file".into() } }, A::None),
            ("the address is taken", launched(fresh), E::AddressTaken, quiet(), Failed { why: Failure::PortTaken(None) }, A::None),
            // Starting.
            ("ready", starting(NOW - 5_000), E::Answered(Ready), quiet(), Running { pid: 7, restarts: fresh, last_used_ms: NOW, ready_since_ms: NOW }, A::None),
            ("an error answer is an answer", starting(NOW - 5_000), E::Answered(Error), quiet(), Running { pid: 7, restarts: fresh, last_used_ms: NOW, ready_since_ms: NOW }, A::None),
            ("loading is a sign of life", starting(NOW - 5_000), E::Answered(Loading), quiet(), starting(NOW), A::None),
            ("quiet past the start timeout", starting(NOW - t.start_timeout_ms), E::Tick, quiet(), stopping(After::Failed(Failure::StartTimedOut)), A::Terminate),
            ("output keeps it starting", starting(NOW - t.start_timeout_ms), E::Tick, Row { output: Some(NOW - 10), ..quiet() }, starting(NOW - t.start_timeout_ms), A::None),
            ("ended before ready", starting(NOW - 5_000), E::Exited { pid: 7, exit: exit(2) }, quiet(), Failed { why: Failure::Exited(exit(2)) }, A::None),
            ("ended saying the port is taken", starting(NOW - 5_000), E::Exited { pid: 7, exit: exit(1) }, said(&["OSError: [Errno 98] Address already in use"]), Failed { why: Failure::PortTaken(Some(exit(1))) }, A::None),
            ("another process's exit is not ours", starting(NOW - 5_000), E::Exited { pid: 8, exit: exit(2) }, quiet(), starting(NOW - 5_000), A::None),
            // Running.
            ("used", running(NOW - 60_000), E::Used, quiet(), running(NOW), A::None),
            ("wanted counts as use", running(NOW - 60_000), E::Tick, wanted(), running(NOW), A::None),
            ("idle", running(NOW - t.idle_ms.unwrap()), E::Tick, quiet(), stopping(After::Stopped(StopWhy::Idle)), A::Terminate),
            ("not yet idle", running(NOW - 1_000), E::Tick, quiet(), running(NOW - 1_000), A::None),
            ("crashed", running(NOW), E::Exited { pid: 7, exit: crashed }, quiet(), waiting(1, NOW, t.backoff_ms[0]), A::None),
            ("crashed again", Running { pid: 7, restarts: Restarts { count: 1, since_ms: NOW - 60_000 }, last_used_ms: NOW, ready_since_ms: NOW - 1_000 }, E::Exited { pid: 7, exit: crashed }, quiet(), waiting(2, NOW - 60_000, t.backoff_ms[1]), A::None),
            ("crashed once too often", Running { pid: 7, restarts: Restarts { count: 3, since_ms: NOW - 60_000 }, last_used_ms: NOW, ready_since_ms: NOW - 1_000 }, E::Exited { pid: 7, exit: crashed }, quiet(), Failed { why: Failure::TooManyRestarts(crashed) }, A::None),
            ("ready long enough, crashes are forgiven", Running { pid: 7, restarts: Restarts { count: 3, since_ms: NOW - 60_000 }, last_used_ms: NOW, ready_since_ms: NOW - t.ready_reset_ms }, E::Exited { pid: 7, exit: crashed }, quiet(), waiting(1, NOW, t.backoff_ms[0]), A::None),
            ("old crashes fall out of the window", Running { pid: 7, restarts: Restarts { count: 3, since_ms: NOW - t.restart_window_ms - 1 }, last_used_ms: NOW, ready_since_ms: NOW - 1_000 }, E::Exited { pid: 7, exit: crashed }, quiet(), waiting(1, NOW, t.backoff_ms[0]), A::None),
            // Waiting to start again.
            ("retry while wanted", waiting(1, NOW, 0), E::Tick, wanted(), Starting { pid: None, restarts: Restarts { count: 1, since_ms: NOW }, since_ms: NOW }, launch(LaunchWhy::Restart)),
            ("retry while not wanted", waiting(1, NOW, 0), E::Tick, quiet(), stopped(StopWhy::Exited), A::None),
            ("not yet time to retry", waiting(1, NOW, 500), E::Tick, wanted(), waiting(1, NOW, 500), A::None),
            // Stopping.
            ("stopped", stopping(After::Stopped(StopWhy::Idle)), E::Exited { pid: 7, exit: crashed }, quiet(), stopped(StopWhy::Idle), A::None),
            ("stopped into a failure", stopping(After::Failed(Failure::StartTimedOut)), E::Exited { pid: 7, exit: crashed }, quiet(), Failed { why: Failure::StartTimedOut }, A::None),
            ("the grace is over", Stopping { pid: 7, kill_at_ms: NOW, then: After::Stopped(StopWhy::Idle) }, E::Tick, quiet(), Stopping { pid: 7, kill_at_ms: NOW, then: After::Stopped(StopWhy::Idle) }, A::Kill),
            ("still in grace", stopping(After::Stopped(StopWhy::Idle)), E::Tick, quiet(), stopping(After::Stopped(StopWhy::Idle)), A::None),
            ("a want while stopping waits", stopping(After::Stopped(StopWhy::Idle)), E::Wanted, quiet(), stopping(After::Stopped(StopWhy::Idle)), A::None),
            // Failed.
            ("failed stays failed when wanted", Failed { why: Failure::StartTimedOut }, E::Wanted, quiet(), Failed { why: Failure::StartTimedOut }, A::None),
            ("try again", Failed { why: Failure::StartTimedOut }, E::RetryChosen, quiet(), stopped(StopWhy::Retry), A::None),
            ("a server started by hand after a failure is used", Failed { why: Failure::Exited(exit(2)) }, E::Answered(Ready), wanted(), External, A::None),
            ("one loading by hand after a failure is used too", Failed { why: Failure::PortTaken(None) }, E::Answered(Loading), quiet(), External, A::None),
            ("no answer leaves a failure as it is", Failed { why: Failure::Exited(exit(2)) }, E::Answered(Offline), wanted(), Failed { why: Failure::Exited(exit(2)) }, A::None),
            ("saving the profile clears a failure", Failed { why: Failure::StartTimedOut }, E::Switched { same_launch: true }, quiet(), stopped(StopWhy::Never), A::None),
            // External.
            ("the other program went away", External, E::Answered(Offline), quiet(), stopped(StopWhy::Never), A::None),
            ("an external server is used as it is", External, E::Wanted, wanted(), External, A::None),
            ("an external server is never stopped", External, E::Quit, quiet(), External, A::None),
            ("an external server is never stopped now", External, E::StopChosen, quiet(), External, A::None),
            // Switching.
            ("switched away from a running server", running(NOW), E::Switched { same_launch: false }, quiet(), stopping(After::Stopped(StopWhy::Switched)), A::Terminate),
            ("switched away while starting", starting(NOW), E::Switched { same_launch: false }, quiet(), stopping(After::Stopped(StopWhy::Switched)), A::Terminate),
            ("switched away while waiting", waiting(1, NOW, 500), E::Switched { same_launch: false }, quiet(), stopped(StopWhy::Switched), A::None),
            ("only the voice changed", running(NOW), E::Switched { same_launch: true }, quiet(), running(NOW), A::None),
            ("switched away from an external server", External, E::Switched { same_launch: false }, quiet(), stopped(StopWhy::Never), A::None),
            // Stop now.
            ("stop now", running(NOW), E::StopChosen, quiet(), stopping(After::Stopped(StopWhy::Asked)), A::Terminate),
            ("stop now while waiting", waiting(1, NOW, 500), E::StopChosen, quiet(), stopped(StopWhy::Asked), A::None),
            // Quit.
            ("quit while running", running(NOW), E::Quit, quiet(), Stopping { pid: 7, kill_at_ms: NOW + t.quit_grace_ms, then: After::Stopped(StopWhy::Quit) }, A::Terminate),
            ("quit while waiting", waiting(1, NOW, 500), E::Quit, quiet(), stopped(StopWhy::Quit), A::None),
            ("quit while stopped", stopped(StopWhy::Idle), E::Quit, quiet(), stopped(StopWhy::Quit), A::None),
            ("nothing is launched after quitting", stopped(StopWhy::Quit), E::Tick, wanted(), stopped(StopWhy::Quit), A::None),
        ];

        for (name, state, event, row, expected, action) in rows {
            let ctx = Ctx {
                now_ms: NOW,
                wanted: row.wanted,
                last_answer: row.answer,
                last_output_ms: row.output,
                tail: &row.tail,
                timing: t,
            };
            assert_eq!(next(state, event, &ctx), (expected, action), "{name}");
        }
    }

    #[test]
    fn an_idle_time_of_none_keeps_the_server_running() {
        let timing = Timing {
            idle_ms: None,
            ..Timing::default()
        };
        let ctx = Ctx {
            now_ms: NOW,
            wanted: false,
            last_answer: Some(Ready),
            last_output_ms: None,
            tail: &[],
            timing: &timing,
        };
        assert_eq!(
            next(running(0), ServerEvent::Tick, &ctx),
            (running(0), ServerAction::None)
        );
    }

    #[test]
    fn a_taken_port_is_found_in_the_last_lines() {
        let lines = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(port_taken(&lines(&[
            "INFO: loading",
            "ERROR: [Errno 98] error while attempting to bind on address ('127.0.0.1', 8883): address already in use",
        ])));
        assert!(port_taken(&lines(&["Error: listen EADDRINUSE: :::8883"])));
        assert!(port_taken(&lines(&["OSError: Address already in use"])));
        assert!(!port_taken(&lines(&[
            "Traceback",
            "ModuleNotFoundError: torch"
        ])));
        assert!(!port_taken(&[]));
    }
}

/// The server with real `sh` scripts as fake servers, a settable clock and a settable want.
#[cfg(test)]
pub(super) mod process_tests {
    use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

    use super::*;

    pub(in crate::core) struct Host {
        /// Added to the real clock.
        pub ahead_ms: AtomicI64,
        pub wanted: AtomicBool,
        pub views: Mutex<Vec<Option<ServerView>>>,
    }

    impl Host {
        pub fn new() -> Arc<Self> {
            Arc::new(Self {
                ahead_ms: AtomicI64::new(0),
                wanted: AtomicBool::new(false),
                views: Mutex::new(vec![]),
            })
        }

        pub fn advance(&self, ms: i64) {
            self.ahead_ms.fetch_add(ms, Ordering::SeqCst);
        }
    }

    impl ServerHost for Host {
        fn now_ms(&self) -> i64 {
            super::super::now_ms() + self.ahead_ms.load(Ordering::SeqCst)
        }
        fn wanted(&self) -> bool {
            self.wanted.load(Ordering::SeqCst)
        }
        fn server_changed(&self, view: Option<ServerView>) {
            self.views.lock().unwrap().push(view);
        }
    }

    /// A launch command running `script` with `sh`.
    pub fn script(script: &str) -> LaunchCommand {
        LaunchCommand {
            program: "sh".into(),
            args: vec!["-c".into(), script.into()],
            working_dir: None,
            ..LaunchCommand::default()
        }
    }

    pub fn spec(launch: LaunchCommand) -> Spec {
        Spec {
            launch,
            // Nothing listens on the discard port.
            base_url: "http://127.0.0.1:9".into(),
        }
    }

    pub fn timing() -> Timing {
        Timing {
            grace_ms: 200,
            quit_grace_ms: 200,
            backoff_ms: [50, 50, 50],
            ..Timing::default()
        }
    }

    /// A running server over `dir` for `launch`, its host, and the task applying its events.
    pub fn running(
        dir: &Path,
        launch: LaunchCommand,
    ) -> (Arc<ServerProcess>, Arc<Host>, tokio::task::JoinHandle<()>) {
        let server = Arc::new(
            ServerProcess::new(
                Names::SPEECH,
                dir,
                &dir.join("logs"),
                Some(spec(launch)),
                timing(),
            )
            .with_tick(Duration::from_millis(20)),
        );
        let host = Host::new();
        let task = {
            let (server, host) = (Arc::clone(&server), Arc::clone(&host));
            tokio::spawn(async move { server.run(&*host).await })
        };
        (server, host, task)
    }

    pub async fn until(what: &str, check: impl Fn() -> bool) {
        for _ in 0..400 {
            if check() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("timed out waiting for {what}");
    }

    pub fn state(server: &ServerProcess) -> Option<ServerState> {
        server.view().map(|v| v.state)
    }

    fn lines(path: &Path) -> usize {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .count()
    }

    #[tokio::test]
    async fn looking_at_an_offline_engine_with_nothing_wanted_launches_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let marks = dir.path().join("launched.txt");
        let (server, _host, task) = running(
            dir.path(),
            script(&format!(
                "echo launched >> '{}'; exec sleep 300",
                marks.display()
            )),
        );
        for _ in 0..3 {
            server.answered(Answer::Offline);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert_eq!(lines(&marks), 0);
        assert_eq!(state(&server), Some(ServerState::Stopped));
        task.abort();
    }

    #[tokio::test]
    async fn a_want_launches_the_server_once() {
        let dir = tempfile::tempdir().unwrap();
        let marks = dir.path().join("launched.txt");
        let (server, host, task) = running(
            dir.path(),
            script(&format!(
                "echo launched >> '{}'; exec sleep 300",
                marks.display()
            )),
        );
        server.answered(Answer::Offline);
        for _ in 0..3 {
            server.want();
            server.answered(Answer::Offline);
        }
        until("the launch", || lines(&marks) == 1).await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert_eq!(lines(&marks), 1);
        assert_eq!(state(&server), Some(ServerState::Starting));
        server.stop(&*host);
        task.abort();
    }

    #[tokio::test]
    async fn a_server_that_ends_at_once_fails_with_its_exit_code_and_last_lines() {
        let dir = tempfile::tempdir().unwrap();
        let (server, _host, task) = running(dir.path(), script("echo boom >&2; exit 2"));
        server.answered(Answer::Offline);
        server.want();
        until("the failure", || {
            state(&server) == Some(ServerState::Failed)
        })
        .await;

        let view = server.view().unwrap();
        assert_eq!(view.failure, Some(FailureKind::Exited));
        assert_eq!(view.exit_code, Some(2));
        assert!(view.can_retry);
        assert_eq!(view.last_lines, vec!["boom".to_string()]);
        assert_eq!(server.log_tail(), vec!["boom".to_string()]);
        let log = std::fs::read_to_string(dir.path().join("logs/speech-server.log")).unwrap();
        assert_eq!(log, "boom\n");
        task.abort();
    }

    #[tokio::test]
    async fn a_missing_program_fails_to_start() {
        let dir = tempfile::tempdir().unwrap();
        let missing = LaunchCommand {
            program: dir.path().join("no-such-server").display().to_string(),
            args: vec![],
            working_dir: None,
            ..LaunchCommand::default()
        };
        let (server, _host, task) = running(dir.path(), missing);
        server.answered(Answer::Offline);
        server.want();
        until("the failure", || {
            state(&server) == Some(ServerState::Failed)
        })
        .await;

        let view = server.view().unwrap();
        assert_eq!(view.failure, Some(FailureKind::Spawn));
        assert!(view.error.is_some());
        task.abort();
    }

    /// Whether process `pid`, written by a script to `file`, still runs.
    pub fn alive_in(file: &Path) -> Option<bool> {
        let pid: i32 = std::fs::read_to_string(file).ok()?.trim().parse().ok()?;
        let pid = rustix::process::Pid::from_raw(pid)?;
        Some(rustix::process::test_kill_process(pid).is_ok())
    }

    /// A wrapper (as `uv run` or `conda run` are) whose child is the real server: writes its
    /// own pid to `pid` and its child's to `child`.
    pub fn wrapper(pid: &Path, child: &Path, child_script: &str) -> LaunchCommand {
        script(&format!(
            "sh -c '{child_script}' & echo $! > '{}'; echo $$ > '{}'; wait",
            child.display(),
            pid.display()
        ))
    }

    #[tokio::test]
    async fn the_server_is_starting_until_the_engine_answers_ready() {
        let dir = tempfile::tempdir().unwrap();
        let (server, host, task) = running(dir.path(), script("exec sleep 300"));
        server.answered(Answer::Offline);
        server.want();
        until("the launch", || {
            state(&server) == Some(ServerState::Starting)
        })
        .await;

        server.answered(Answer::Loading);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(state(&server), Some(ServerState::Starting));
        server.answered(Answer::Ready);
        until("ready", || state(&server) == Some(ServerState::Running)).await;
        server.stop(&*host);
        task.abort();
    }

    #[tokio::test]
    async fn a_silent_server_is_stopped_after_the_start_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let pid = dir.path().join("pid");
        let (server, host, task) = running(
            dir.path(),
            script(&format!("echo $$ > '{}'; exec sleep 300", pid.display())),
        );
        server.answered(Answer::Offline);
        server.want();
        until("the launch", || alive_in(&pid) == Some(true)).await;
        tokio::time::sleep(Duration::from_millis(100)).await; // Its one line of output is old.

        host.advance(Timing::default().start_timeout_ms);
        until("the failure", || {
            state(&server) == Some(ServerState::Failed)
        })
        .await;

        let view = server.view().unwrap();
        assert_eq!(view.failure, Some(FailureKind::StartTimedOut));
        assert_eq!(alive_in(&pid), Some(false));
        task.abort();
    }

    #[tokio::test]
    async fn output_keeps_a_starting_server_starting() {
        let dir = tempfile::tempdir().unwrap();
        let (server, host, task) = running(
            dir.path(),
            script("while true; do echo 'downloading weights'; sleep 0.02; done"),
        );
        server.answered(Answer::Offline);
        server.want();
        until("the launch", || {
            state(&server) == Some(ServerState::Starting)
        })
        .await;

        host.advance(Timing::default().start_timeout_ms);
        tokio::time::sleep(Duration::from_millis(300)).await;

        assert_eq!(state(&server), Some(ServerState::Starting));
        server.stop(&*host);
        task.abort();
    }

    #[tokio::test]
    async fn the_start_timeout_comes_from_the_launch_command() {
        let dir = tempfile::tempdir().unwrap();
        let mut launch = script("exec sleep 300");
        launch.start_timeout_seconds = 5;
        let (server, host, task) = running(dir.path(), launch);
        server.answered(Answer::Offline);
        server.want();
        until("the launch", || {
            state(&server) == Some(ServerState::Starting)
        })
        .await;

        host.advance(5_000);
        until("the failure", || {
            state(&server) == Some(ServerState::Failed)
        })
        .await;
        assert_eq!(server.view().unwrap().start_timeout_seconds, 5);
        task.abort();
    }

    #[tokio::test]
    async fn a_stop_ends_the_wrappers_child_too() {
        let dir = tempfile::tempdir().unwrap();
        let (pid, child) = (dir.path().join("pid"), dir.path().join("child"));
        let (server, host, task) = running(dir.path(), wrapper(&pid, &child, "exec sleep 300"));
        server.answered(Answer::Offline);
        server.want();
        until("the launch", || alive_in(&child) == Some(true)).await;
        server.answered(Answer::Ready);
        until("ready", || state(&server) == Some(ServerState::Running)).await;

        server.stop_now();
        until("the stop", || state(&server) == Some(ServerState::Stopped)).await;

        assert_eq!(alive_in(&pid), Some(false));
        assert_eq!(
            alive_in(&child),
            Some(false),
            "the real server is not left behind"
        );
        assert_eq!(server.view().unwrap().stopped, Some(StoppedKind::Asked));
        drop(host);
        task.abort();
    }

    #[tokio::test]
    async fn a_server_that_ignores_sigterm_is_killed_after_the_grace_period() {
        let dir = tempfile::tempdir().unwrap();
        let pid = dir.path().join("pid");
        let (server, host, task) = running(
            dir.path(),
            script(&format!(
                "trap '' TERM; echo $$ > '{}'; while true; do sleep 0.05; done",
                pid.display()
            )),
        );
        server.answered(Answer::Offline);
        server.want();
        until("the launch", || alive_in(&pid) == Some(true)).await;
        server.answered(Answer::Ready);
        until("ready", || state(&server) == Some(ServerState::Running)).await;

        server.stop_now();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            state(&server),
            Some(ServerState::Stopping),
            "still in its grace"
        );
        until("the kill", || state(&server) == Some(ServerState::Stopped)).await;
        assert_eq!(alive_in(&pid), Some(false));
        drop(host);
        task.abort();
    }

    #[tokio::test]
    async fn a_leader_that_ends_by_itself_leaves_no_member_behind() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("child");
        let (server, _host, task) = running(
            dir.path(),
            script(&format!(
                "sleep 300 & echo $! > '{}'; exit 1",
                child.display()
            )),
        );
        server.answered(Answer::Offline);
        server.want();
        until("the failure", || {
            state(&server) == Some(ServerState::Failed)
        })
        .await;

        assert_eq!(alive_in(&child), Some(false));
        task.abort();
    }

    #[tokio::test]
    async fn quitting_stops_the_whole_group_within_the_quit_grace() {
        let dir = tempfile::tempdir().unwrap();
        let (pid, child) = (dir.path().join("pid"), dir.path().join("child"));
        let (server, host, task) = running(
            dir.path(),
            wrapper(
                &pid,
                &child,
                "trap \"\" TERM; while true; do sleep 0.05; done",
            ),
        );
        server.answered(Answer::Offline);
        server.want();
        until("the launch", || alive_in(&child) == Some(true)).await;

        let started = Instant::now();
        server.stop(&*host);

        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(alive_in(&pid), Some(false));
        assert_eq!(alive_in(&child), Some(false));
        task.abort();
    }

    /// A group of its own running `script`, as a server Aoide lost in a crash would be.
    #[cfg(target_os = "linux")]
    fn lost(script: &str) -> std::process::Child {
        let mut command = std::process::Command::new("sh");
        command.args(["-c", script]);
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        command.spawn().unwrap()
    }

    #[cfg(target_os = "linux")]
    fn server_over(dir: &Path) -> ServerProcess {
        ServerProcess::new(
            Names::SPEECH,
            dir,
            &dir.join("logs"),
            Some(spec(script("exec sleep 300"))),
            timing(),
        )
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn the_pidfile_names_the_server_while_it_runs() {
        let dir = tempfile::tempdir().unwrap();
        let pidfile = dir.path().join("speech-server.pid");
        let (server, _host, task) = running(dir.path(), script("exec sleep 300"));
        server.answered(Answer::Offline);
        server.want();
        until("the pidfile", || pidfile.exists()).await;
        let written = std::fs::read_to_string(&pidfile).unwrap();
        assert!(written.contains("program=sh"), "{written}");

        server.stop_now();
        until("the stop", || state(&server) == Some(ServerState::Stopped)).await;
        assert!(!pidfile.exists());
        task.abort();
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn a_server_left_by_a_crash_is_stopped_at_the_next_start() {
        let dir = tempfile::tempdir().unwrap();
        let mut leftover = lost("exec sleep 300");
        let pid = leftover.id();
        leftover::Record::of(pid, "sh")
            .write(&dir.path().join("speech-server.pid"))
            .unwrap();

        let _server = server_over(dir.path());

        let ended = leftover.try_wait().unwrap().or_else(|| {
            std::thread::sleep(Duration::from_millis(200));
            leftover.try_wait().unwrap()
        });
        assert!(ended.is_some(), "the leftover server was stopped");
        assert!(!dir.path().join("speech-server.pid").exists());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn what_is_left_of_a_lost_servers_group_is_stopped_too() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("child");
        let mut leader = lost(&format!("sleep 300 & echo $! > '{}'", child.display()));
        let record = leftover::Record::of(leader.id(), "sh");
        leader.wait().unwrap();
        until("the child", || alive_in(&child) == Some(true)).await;
        record.write(&dir.path().join("speech-server.pid")).unwrap();

        let _server = server_over(dir.path());

        until("the child to be stopped", || {
            alive_in(&child) == Some(false)
        })
        .await;
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn a_pidfile_naming_another_program_now_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let mut other = lost("exec sleep 300");
        let mut record = leftover::Record::of(other.id(), "sh");
        record.start = record.start.map(|s| s.saturating_sub(1_000));
        record.write(&dir.path().join("speech-server.pid")).unwrap();

        let _server = server_over(dir.path());

        assert!(
            other.try_wait().unwrap().is_none(),
            "not ours: left running"
        );
        assert!(!dir.path().join("speech-server.pid").exists());
        other.kill().unwrap();
        other.wait().unwrap();
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn the_server_gets_sigterm_when_the_thread_that_started_it_ends() {
        let dir = tempfile::tempdir().unwrap();
        let pid = dir.path().join("pid");
        let (server, _host, task) = running(
            dir.path(),
            script(&format!("echo $$ > '{}'; exec sleep 300", pid.display())),
        );
        server.answered(Answer::Offline);
        server.want();
        until("the launch", || alive_in(&pid) == Some(true)).await;

        task.abort();
        let _ = task.await;
        drop(server); // Ends the spawner thread, without stopping anything.

        until("the parent-death signal", || alive_in(&pid) == Some(false)).await;
    }

    /// Answers Ready for the server every few milliseconds, as an engine whose port is open.
    fn answering(server: &Arc<ServerProcess>) -> tokio::task::JoinHandle<()> {
        let server = Arc::clone(server);
        tokio::spawn(async move {
            loop {
                if server.view().map(|v| v.state) == Some(ServerState::Starting) {
                    server.answered(Answer::Ready);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
    }

    #[tokio::test]
    async fn a_crashed_server_is_started_again_three_times_and_then_given_up() {
        let dir = tempfile::tempdir().unwrap();
        let marks = dir.path().join("launched.txt");
        let (server, host, task) = running(
            dir.path(),
            script(&format!(
                "echo x >> '{}'; sleep 0.3; exit 1",
                marks.display()
            )),
        );
        host.wanted.store(true, std::sync::atomic::Ordering::SeqCst);
        let answers = answering(&server);
        server.answered(Answer::Offline);
        server.want();
        until("the failure", || {
            state(&server) == Some(ServerState::Failed)
        })
        .await;

        let view = server.view().unwrap();
        assert_eq!(view.failure, Some(FailureKind::TooManyRestarts));
        assert_eq!(view.exit_code, Some(1));
        assert_eq!(lines(&marks), 4, "launched, then started again three times");
        let waited = host
            .views
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .any(|v| v.state == ServerState::Waiting);
        assert!(waited);

        // Try again (by now the engine's address answers that nothing listens).
        server.answered(Answer::Offline);
        server.retry();
        server.want();
        until("the relaunch", || lines(&marks) == 5).await;
        answers.abort();
        server.stop(&*host);
        task.abort();
    }

    #[tokio::test]
    async fn a_server_that_answers_before_any_want_is_used_and_never_started_or_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let marks = dir.path().join("launched.txt");
        let (server, host, task) = running(
            dir.path(),
            script(&format!("echo x >> '{}'; exec sleep 300", marks.display())),
        );
        server.answered(Answer::Ready);
        server.want();
        until("external", || state(&server) == Some(ServerState::External)).await;
        server.stop_now();
        server.stop(&*host);
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(lines(&marks), 0);
        assert_eq!(state(&server), Some(ServerState::External));

        // It went away: ours may start on the next want.
        server.answered(Answer::Offline);
        until("stopped", || state(&server) == Some(ServerState::Stopped)).await;
        task.abort();
    }

    #[tokio::test]
    async fn a_server_saying_its_address_is_in_use_fails_with_the_port_taken() {
        let dir = tempfile::tempdir().unwrap();
        let (server, _host, task) = running(
            dir.path(),
            script("echo 'ERROR: [Errno 98] Address already in use' >&2; exit 1"),
        );
        server.answered(Answer::Offline);
        server.want();
        until("the failure", || {
            state(&server) == Some(ServerState::Failed)
        })
        .await;
        assert_eq!(server.view().unwrap().failure, Some(FailureKind::PortTaken));
        task.abort();
    }

    #[tokio::test]
    async fn something_else_listening_at_the_address_keeps_the_server_from_starting() {
        let dir = tempfile::tempdir().unwrap();
        let marks = dir.path().join("launched.txt");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut taken = spec(script(&format!("echo x >> '{}'", marks.display())));
        taken.base_url = format!("http://{}", listener.local_addr().unwrap());
        let server = Arc::new(
            ServerProcess::new(
                Names::SPEECH,
                dir.path(),
                &dir.path().join("logs"),
                Some(taken),
                timing(),
            )
            .with_tick(Duration::from_millis(20)),
        );
        let host = Host::new();
        let task = {
            let (server, host) = (Arc::clone(&server), Arc::clone(&host));
            tokio::spawn(async move { server.run(&*host).await })
        };
        server.answered(Answer::Error);
        server.want();
        until("the failure", || {
            state(&server) == Some(ServerState::Failed)
        })
        .await;

        let view = server.view().unwrap();
        assert_eq!(view.failure, Some(FailureKind::PortTaken));
        assert_eq!(view.exit_code, None);
        assert_eq!(lines(&marks), 0, "not launched");
        task.abort();
    }

    #[tokio::test]
    async fn switching_to_another_launch_command_stops_the_old_server_and_a_voice_change_does_not()
    {
        let dir = tempfile::tempdir().unwrap();
        let (pid, other) = (dir.path().join("pid"), dir.path().join("other"));
        let first = script(&format!("echo $$ > '{}'; exec sleep 300", pid.display()));
        let (server, host, task) = running(dir.path(), first.clone());
        server.answered(Answer::Offline);
        server.want();
        until("the launch", || alive_in(&pid) == Some(true)).await;
        server.answered(Answer::Ready);
        until("ready", || state(&server) == Some(ServerState::Running)).await;

        server.switch(Some(spec(first)));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            state(&server),
            Some(ServerState::Running),
            "the same launch command"
        );
        assert_eq!(alive_in(&pid), Some(true));

        server.switch(Some(spec(script(&format!(
            "echo $$ > '{}'; exec sleep 300",
            other.display()
        )))));
        until("the old server to stop", || alive_in(&pid) == Some(false)).await;
        until("stopped", || state(&server) == Some(ServerState::Stopped)).await;
        assert_eq!(server.view().unwrap().stopped, Some(StoppedKind::Switched));
        assert!(!other.exists(), "the new one waits for a want");

        server.answered(Answer::Offline);
        server.want();
        until("the new launch", || alive_in(&other) == Some(true)).await;
        server.stop(&*host);
        task.abort();
    }

    #[tokio::test]
    async fn an_unused_server_is_stopped_after_its_idle_time_and_started_again_when_wanted() {
        let dir = tempfile::tempdir().unwrap();
        let pid = dir.path().join("pid");
        let mut launch = script(&format!("echo $$ > '{}'; exec sleep 300", pid.display()));
        launch.idle_minutes = 5;
        let (server, host, task) = running(dir.path(), launch);
        server.answered(Answer::Offline);
        server.want();
        until("the launch", || alive_in(&pid) == Some(true)).await;
        server.answered(Answer::Ready);
        until("ready", || state(&server) == Some(ServerState::Running)).await;

        // Work queued or prepared keeps it running however long it takes.
        host.wanted.store(true, std::sync::atomic::Ordering::SeqCst);
        host.advance(6 * 60_000);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(state(&server), Some(ServerState::Running));

        host.wanted
            .store(false, std::sync::atomic::Ordering::SeqCst);
        host.advance(4 * 60_000);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            state(&server),
            Some(ServerState::Running),
            "used until just now"
        );
        host.advance(60_000);
        until("the idle stop", || {
            state(&server) == Some(ServerState::Stopped)
        })
        .await;
        assert_eq!(alive_in(&pid), Some(false));
        let view = server.view().unwrap();
        assert_eq!(view.stopped, Some(StoppedKind::Idle));
        assert_eq!(view.idle_minutes, Some(5));

        std::fs::remove_file(&pid).unwrap();
        server.answered(Answer::Offline);
        server.want();
        until("the wake", || alive_in(&pid) == Some(true)).await;
        assert_eq!(state(&server), Some(ServerState::Starting));
        server.stop(&*host);
        task.abort();
    }

    #[tokio::test]
    async fn an_idle_time_of_zero_keeps_the_server_running() {
        let dir = tempfile::tempdir().unwrap();
        let mut launch = script("exec sleep 300");
        launch.idle_minutes = 0;
        let (server, host, task) = running(dir.path(), launch);
        server.answered(Answer::Offline);
        server.want();
        until("the launch", || {
            state(&server) == Some(ServerState::Starting)
        })
        .await;
        server.answered(Answer::Ready);
        until("ready", || state(&server) == Some(ServerState::Running)).await;

        host.advance(24 * 60 * 60_000);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(state(&server), Some(ServerState::Running));
        assert_eq!(server.view().unwrap().idle_minutes, None);
        server.stop(&*host);
        task.abort();
    }
}
