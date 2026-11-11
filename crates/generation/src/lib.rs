//! Decides which segments to synthesise next and runs one TTS request at a time.
//!
//! The planner is a pure function of the playhead, the cache and the prepare queue: callers
//! recompute the plan whenever any of those change and hand it to the [`Worker`], which
//! replaces (never appends to) its pending work. The worker waits for the plan to settle
//! (debounce) before starting a request, never runs two requests at once, and always finishes
//! and stores an in-flight request, because servers cannot cancel work.
//!
//! Failures usually mean the setup is wrong (server down, unknown voice), not the sentence, so
//! after one the whole worker backs off (see [`Backoff`]) instead of trying the next segment.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use aoide_tts::{BoxFuture, SpeechEngine, Synthesis, SynthesisRequest};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

/// One segment as the planner sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItem {
    pub key: String,
    /// False for segments with nothing to say; they are never generated.
    pub speakable: bool,
    pub cached: bool,
    /// Actual duration when cached, otherwise an estimate (see [`estimate_ms`]).
    pub duration_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Playhead,
    Prepare,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub book: String,
    /// Index into that book's item list.
    pub index: usize,
    pub key: String,
    pub reason: Reason,
}

/// The book being listened to.
pub struct Listening<'a> {
    pub book: &'a str,
    pub items: &'a [PlanItem],
    pub playhead: usize,
    /// How much audio (cached or not) ahead of the playhead to keep generated.
    pub lookahead_ms: u32,
}

/// A book queued for whole-book preparation.
pub struct Preparing<'a> {
    pub book: &'a str,
    pub items: &'a [PlanItem],
}

/// Rough spoken duration for `chars` characters of text (about 15 characters per second).
pub fn estimate_ms(chars: usize) -> u32 {
    u32::try_from(chars as u64 * 1000 / 15).unwrap_or(u32::MAX)
}

/// Contiguous cached audio from the playhead onward (unspeakable items count as zero and do not
/// break the run).
pub fn buffered_ahead_ms(items: &[PlanItem], playhead: usize) -> u32 {
    let mut total = 0u32;
    for item in items.iter().skip(playhead) {
        if !item.speakable {
            continue;
        }
        if !item.cached {
            break;
        }
        total = total.saturating_add(item.duration_ms);
    }
    total
}

/// Jobs in priority order:
/// 1. uncached speakable items within `lookahead_ms` of the playhead, in reading order;
/// 2. prepare targets in queue order, all uncached speakable items; for the book being
///    listened to, starting at the playhead and wrapping to the start.
///
/// Each key appears at most once.
pub fn plan(listening: Option<&Listening<'_>>, preparing: &[Preparing<'_>]) -> Vec<Job> {
    let mut seen = HashSet::new();
    let mut jobs = Vec::new();
    let mut push = |book: &str, index: usize, item: &PlanItem, reason: Reason| {
        if item.speakable && !item.cached && seen.insert(item.key.clone()) {
            jobs.push(Job {
                book: book.to_owned(),
                index,
                key: item.key.clone(),
                reason,
            });
        }
    };
    if let Some(listening) = listening {
        let mut ahead = 0u32;
        for (index, item) in listening.items.iter().enumerate().skip(listening.playhead) {
            if ahead >= listening.lookahead_ms {
                break;
            }
            ahead = ahead.saturating_add(item.duration_ms);
            push(listening.book, index, item, Reason::Playhead);
        }
    }
    for target in preparing {
        let start = listening
            .filter(|l| l.book == target.book)
            .map_or(0, |l| l.playhead.min(target.items.len()));
        let order = (start..target.items.len()).chain(0..start);
        for index in order {
            push(target.book, index, &target.items[index], Reason::Prepare);
        }
    }
    jobs
}

/// Persists a finished synthesis (e.g. encode + write file + index row) and returns its
/// duration in milliseconds.
pub trait SegmentStore: Send + Sync + 'static {
    fn store<'a>(
        &'a self,
        job: &'a Job,
        synthesis: Synthesis,
    ) -> BoxFuture<'a, Result<u32, String>>;
}

#[derive(Debug, Clone)]
pub struct WorkItem {
    pub job: Job,
    pub request: SynthesisRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerEvent {
    Started(Job),
    Finished { job: Job, duration_ms: u32 },
    Failed { job: Job, error: String },
}

#[derive(Debug, Clone, Copy)]
pub struct WorkerOptions {
    /// The plan must be unchanged this long before a new request starts.
    pub debounce: Duration,
    /// After a failure no request starts for this long; it doubles with each failure in a row.
    pub failure_backoff: Duration,
    /// The longest wait after failures. A failed key is also skipped this long, so a single
    /// segment the engine cannot say does not hold up the rest.
    pub max_failure_backoff: Duration,
}

/// Whether the worker may start requests, given recent failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backoff {
    Working,
    /// `streak` requests failed in a row; no request starts before `until`.
    Waiting {
        streak: u32,
        until: Instant,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackoffEvent {
    Succeeded,
    Failed {
        at: Instant,
    },
    /// The caller replaced the plan after a jump or a settings change: start afresh.
    NewPlan,
}

impl Backoff {
    /// The whole state machine.
    pub fn next(self, event: BackoffEvent, options: &WorkerOptions) -> Self {
        match (self, event) {
            (_, BackoffEvent::Succeeded | BackoffEvent::NewPlan) => Backoff::Working,
            (state, BackoffEvent::Failed { at }) => {
                let streak = match state {
                    Backoff::Working => 1,
                    Backoff::Waiting { streak, .. } => streak.saturating_add(1),
                };
                let wait = options
                    .failure_backoff
                    .saturating_mul(1 << (streak - 1).min(16))
                    .min(options.max_failure_backoff);
                Backoff::Waiting {
                    streak,
                    until: at + wait,
                }
            }
        }
    }

    fn until(self) -> Option<Instant> {
        match self {
            Backoff::Working => None,
            Backoff::Waiting { until, .. } => Some(until),
        }
    }
}

impl Default for WorkerOptions {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(450),
            failure_backoff: Duration::from_secs(10),
            max_failure_backoff: Duration::from_secs(60),
        }
    }
}

struct Plan {
    items: Vec<WorkItem>,
    /// No new request starts before this instant.
    settle_at: Instant,
    /// Counts [`Worker::set_plan`] calls, so the worker can tell a new plan from an update.
    resets: u64,
}

/// Handle to the background worker task. Dropping it stops the worker after any in-flight job.
pub struct Worker {
    plan: watch::Sender<Plan>,
    debounce: Duration,
    /// Keys whose audio was deleted since they were made; the worker makes them again if asked.
    forgotten: Arc<std::sync::Mutex<HashSet<String>>>,
}

impl Worker {
    /// Spawns the worker on the current Tokio runtime.
    pub fn spawn(
        engine: Arc<dyn SpeechEngine>,
        store: Arc<dyn SegmentStore>,
        options: WorkerOptions,
        events: mpsc::UnboundedSender<WorkerEvent>,
    ) -> Self {
        let (plan, receiver) = watch::channel(Plan {
            items: Vec::new(),
            settle_at: Instant::now(),
            resets: 0,
        });
        let forgotten = Arc::default();
        tokio::spawn(run(
            receiver,
            Arc::clone(&forgotten),
            engine,
            store,
            options,
            events,
        ));
        Self {
            plan,
            debounce: options.debounce,
            forgotten,
        }
    }

    /// Replaces the pending work after a jump (seek, chapter change, new book). The worker waits
    /// for the plan to stay unchanged for the debounce period before starting a request.
    pub fn set_plan(&self, items: Vec<WorkItem>) {
        let settle_at = Instant::now() + self.debounce;
        self.plan.send_modify(|plan| {
            plan.items = items;
            plan.settle_at = settle_at;
            plan.resets += 1;
        });
    }

    /// Replaces the pending work after a routine change (a segment was cached, the playhead
    /// advanced normally). Does not restart the debounce.
    pub fn update_plan(&self, items: Vec<WorkItem>) {
        self.plan.send_modify(|plan| plan.items = items);
    }

    /// The audio for these keys was deleted: make them again when a plan asks for them. Without
    /// this the worker skips keys it made earlier in this run.
    pub fn forget(&self, keys: impl IntoIterator<Item = String>) {
        self.forgotten.lock().expect("forgotten lock").extend(keys);
        self.plan.send_modify(|_| {}); // Wake the worker.
    }
}

async fn run(
    mut plan: watch::Receiver<Plan>,
    forgotten: Arc<std::sync::Mutex<HashSet<String>>>,
    engine: Arc<dyn SpeechEngine>,
    store: Arc<dyn SegmentStore>,
    options: WorkerOptions,
    events: mpsc::UnboundedSender<WorkerEvent>,
) {
    let mut done = HashSet::new();
    let mut failed: HashMap<String, Instant> = HashMap::new();
    let mut backoff = Backoff::Working;
    let mut resets = 0;
    loop {
        for key in forgotten.lock().expect("forgotten lock").drain() {
            done.remove(&key);
        }
        let now = Instant::now();
        let (settle_at, next) = {
            let current = plan.borrow_and_update();
            if current.resets != resets {
                resets = current.resets;
                backoff = backoff.next(BackoffEvent::NewPlan, &options);
                failed.clear();
            }
            failed.retain(|_, until| *until > now);
            let next = current
                .items
                .iter()
                .find(|w| !done.contains(&w.job.key) && !failed.contains_key(&w.job.key))
                .cloned();
            (current.settle_at, next)
        };
        let start_at = backoff
            .until()
            .map_or(settle_at, |until| until.max(settle_at));
        let wake = match &next {
            Some(_) if start_at > now => Some(start_at),
            Some(_) => None,
            None => failed.values().min().copied(),
        };
        if next.is_none() || wake.is_some() {
            tokio::select! {
                changed = plan.changed() => if changed.is_err() { return },
                _ = sleep_until(wake), if wake.is_some() => {}
            }
            continue;
        }
        let item = next.expect("checked above");
        let _ = events.send(WorkerEvent::Started(item.job.clone()));
        let outcome = match engine.synthesize(&item.request).await {
            Ok(synthesis) => store.store(&item.job, synthesis).await,
            Err(error) => Err(error.to_string()),
        };
        match outcome {
            Ok(duration_ms) => {
                backoff = backoff.next(BackoffEvent::Succeeded, &options);
                done.insert(item.job.key.clone());
                let _ = events.send(WorkerEvent::Finished {
                    job: item.job,
                    duration_ms,
                });
            }
            Err(error) => {
                let at = Instant::now();
                backoff = backoff.next(BackoffEvent::Failed { at }, &options);
                failed.insert(item.job.key.clone(), at + options.max_failure_backoff);
                let _ = events.send(WorkerEvent::Failed {
                    job: item.job,
                    error,
                });
            }
        }
    }
}

async fn sleep_until(instant: Option<Instant>) {
    match instant {
        Some(instant) => tokio::time::sleep_until(instant).await,
        None => std::future::pending().await,
    }
}
