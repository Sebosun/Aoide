//! When a rendered page is ready to be read, from what is observed of it: the load event, the
//! network going quiet, then the article's selector appearing (when a site's rule names one)
//! or the page's text no longer changing. Pure: times are given with each event.

/// Where a page's readiness is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The load event has not fired.
    Loading,
    /// Loaded; waiting for the network to be quiet.
    Settling,
    /// Quiet; sampling the page for its selector or for text that stopped changing.
    Sampling,
    Ready,
    /// The page took too long; what is there is taken.
    TimedOut,
}

impl Stage {
    pub fn is_over(self) -> bool {
        matches!(self, Stage::Ready | Stage::TimedOut)
    }
}

/// What is observed of the page, at a time in milliseconds since the navigation started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadyEvent {
    Loaded {
        at: u64,
    },
    RequestStarted {
        at: u64,
    },
    /// A request finished or failed.
    RequestEnded {
        at: u64,
    },
    /// The page was looked at: whether the selector is there, and its text's length.
    Sampled {
        at: u64,
        found: bool,
        text_len: usize,
    },
    /// Time passed.
    Tick {
        at: u64,
    },
}

impl ReadyEvent {
    fn at(self) -> u64 {
        match self {
            Self::Loaded { at }
            | Self::RequestStarted { at }
            | Self::RequestEnded { at }
            | Self::Sampled { at, .. }
            | Self::Tick { at } => at,
        }
    }
}

/// How long to wait for what.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadyRules {
    pub timeout_ms: u64,
    /// The network must be quiet this long.
    pub quiet_ms: u64,
    /// Text samples this far apart must have the same length.
    pub sample_ms: u64,
    /// A selector marks the article as present.
    pub selector: bool,
}

impl Default for ReadyRules {
    fn default() -> Self {
        Self {
            timeout_ms: 45_000,
            quiet_ms: 500,
            sample_ms: 300,
            selector: false,
        }
    }
}

/// A page being watched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Watch {
    pub stage: Stage,
    started_at: u64,
    in_flight: usize,
    /// Since when no request has been in flight.
    quiet_since: Option<u64>,
    /// The sample text lengths are compared with.
    sample: Option<(u64, usize)>,
}

impl Watch {
    /// A navigation started at `at`.
    pub fn start(at: u64) -> Self {
        Self {
            stage: Stage::Loading,
            started_at: at,
            in_flight: 0,
            quiet_since: Some(at),
            sample: None,
        }
    }

    /// Whether the page should be sampled now.
    pub fn wants_sample(&self) -> bool {
        self.stage == Stage::Sampling
    }
}

/// The one way a watch changes.
pub fn next(mut watch: Watch, event: ReadyEvent, rules: &ReadyRules) -> Watch {
    if watch.stage.is_over() {
        return watch;
    }
    let at = event.at();
    match event {
        ReadyEvent::RequestStarted { .. } => {
            watch.in_flight += 1;
            watch.quiet_since = None;
        }
        ReadyEvent::RequestEnded { .. } => {
            watch.in_flight = watch.in_flight.saturating_sub(1);
            if watch.in_flight == 0 {
                watch.quiet_since = Some(at);
            }
        }
        ReadyEvent::Loaded { .. } if watch.stage == Stage::Loading => {
            watch.stage = Stage::Settling;
        }
        ReadyEvent::Sampled {
            found, text_len, ..
        } if watch.stage == Stage::Sampling => {
            let steady = watch
                .sample
                .is_some_and(|(then, len)| at >= then + rules.sample_ms && len == text_len);
            if (rules.selector && found) || (!rules.selector && steady && text_len > 0) {
                watch.stage = Stage::Ready;
                return watch;
            }
            // A sample taken too soon after the last one is not compared again later.
            if watch
                .sample
                .is_none_or(|(then, _)| at >= then + rules.sample_ms)
            {
                watch.sample = Some((at, text_len));
            }
        }
        _ => {}
    }
    if at >= watch.started_at + rules.timeout_ms {
        watch.stage = Stage::TimedOut;
    } else if watch.stage == Stage::Settling
        && watch
            .quiet_since
            .is_some_and(|since| at >= since + rules.quiet_ms)
    {
        watch.stage = Stage::Sampling;
    }
    watch
}
