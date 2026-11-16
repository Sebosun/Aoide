//! A server program Aoide starts and stops for a profile's launch command: the speech engine's
//! and the aligner's. Started when it is wanted, noticed when it ends, started again a few
//! times after a crash, stopped when idle, and never left running after Aoide.
//!
//! Its state is one [`Server`], changed only by [`next`] over named [`ServerEvent`]s.

#![cfg_attr(not(test), allow(dead_code))]

use std::time::Duration;

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
        // Never launched over a server that answers: that one is the user's.
        (Server::Stopped { .. }, E::Answered(Answer::Ready | Answer::Loading)) => {
            (Server::External, A::None)
        }
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
        (Server::Waiting { .. }, E::Quit) => (Server::Stopped { why: StopWhy::Quit }, A::None),

        (state, _) => (state, A::None),
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
            ("quit while stopped", stopped(StopWhy::Idle), E::Quit, quiet(), stopped(StopWhy::Idle), A::None),
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
