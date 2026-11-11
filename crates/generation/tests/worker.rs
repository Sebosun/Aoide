use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aoide_generation::{Job, Reason, SegmentStore, WorkItem, Worker, WorkerEvent, WorkerOptions};
use aoide_tts::{
    BoxFuture, EngineDescriptor, EngineError, EngineStatus, Params, SpeechEngine, Synthesis,
    SynthesisRequest, Voice,
};
use tokio::sync::mpsc;

struct FakeEngine {
    descriptor: EngineDescriptor,
    calls: Mutex<Vec<String>>,
    running: AtomicUsize,
    max_running: AtomicUsize,
    latency: Duration,
    fail: Mutex<Vec<String>>,
}

impl FakeEngine {
    fn new(latency_ms: u64) -> Arc<Self> {
        Arc::new(Self {
            descriptor: EngineDescriptor {
                kind: "fake".into(),
                name: "Fake".into(),
                adapter_version: 1,
                languages: None,
                word_timings: false,
                max_chars: 100,
                params: vec![],
            },
            calls: Mutex::new(vec![]),
            running: AtomicUsize::new(0),
            max_running: AtomicUsize::new(0),
            latency: Duration::from_millis(latency_ms),
            fail: Mutex::new(vec![]),
        })
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl SpeechEngine for FakeEngine {
    fn descriptor(&self) -> &EngineDescriptor {
        &self.descriptor
    }
    fn status(&self) -> BoxFuture<'_, EngineStatus> {
        Box::pin(async { EngineStatus::Ready(None) })
    }
    fn voices(&self) -> BoxFuture<'_, Result<Vec<Voice>, EngineError>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn synthesize<'a>(
        &'a self,
        request: &'a SynthesisRequest,
    ) -> BoxFuture<'a, Result<Synthesis, EngineError>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(request.text.clone());
            let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_running.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(self.latency).await;
            self.running.fetch_sub(1, Ordering::SeqCst);
            if self.fail.lock().unwrap().contains(&request.text) {
                return Err(EngineError::Unavailable("down".into()));
            }
            Ok(Synthesis {
                sample_rate: 1000,
                samples: vec![0.0; 1500],
                word_timings: None,
            })
        })
    }
}

#[derive(Default)]
struct Store(Mutex<Vec<String>>);

impl SegmentStore for Store {
    fn store<'a>(
        &'a self,
        job: &'a Job,
        synthesis: Synthesis,
    ) -> BoxFuture<'a, Result<u32, String>> {
        Box::pin(async move {
            self.0.lock().unwrap().push(job.key.clone());
            Ok(synthesis.duration_ms() as u32)
        })
    }
}

fn work(keys: &[&str]) -> Vec<WorkItem> {
    keys.iter()
        .enumerate()
        .map(|(index, key)| WorkItem {
            job: Job {
                book: "bk".into(),
                index,
                key: key.to_string(),
                reason: Reason::Playhead,
            },
            request: SynthesisRequest {
                text: key.to_string(),
                voice_id: "v".into(),
                language: None,
                params: Params::new(),
            },
        })
        .collect()
}

fn options() -> WorkerOptions {
    WorkerOptions {
        debounce: Duration::from_millis(400),
        failure_backoff: Duration::from_secs(10),
    }
}

async fn next(events: &mut mpsc::UnboundedReceiver<WorkerEvent>) -> WorkerEvent {
    tokio::time::timeout(Duration::from_secs(60), events.recv())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test(start_paused = true)]
async fn rapid_plan_changes_only_start_the_settled_plan() {
    let engine = FakeEngine::new(100);
    let store = Arc::new(Store::default());
    let (tx, mut events) = mpsc::unbounded_channel();
    let worker = Worker::spawn(engine.clone(), store.clone(), options(), tx);
    for keys in [["c1"], ["c2"], ["c3"], ["c4"]] {
        worker.set_plan(work(&keys));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        next(&mut events).await,
        WorkerEvent::Started(work(&["c4"])[0].job.clone())
    );
    assert!(matches!(
        next(&mut events).await,
        WorkerEvent::Finished {
            duration_ms: 1500,
            ..
        }
    ));
    assert_eq!(engine.calls(), vec!["c4"]);
}

#[tokio::test(start_paused = true)]
async fn works_through_the_plan_serially_and_stores_each_result() {
    let engine = FakeEngine::new(50);
    let store = Arc::new(Store::default());
    let (tx, mut events) = mpsc::unbounded_channel();
    let worker = Worker::spawn(engine.clone(), store.clone(), options(), tx);
    worker.set_plan(work(&["a", "b", "c"]));
    let mut finished = vec![];
    while finished.len() < 3 {
        if let WorkerEvent::Finished { job, .. } = next(&mut events).await {
            finished.push(job.key);
        }
    }
    assert_eq!(finished, vec!["a", "b", "c"]);
    assert_eq!(*store.0.lock().unwrap(), vec!["a", "b", "c"]);
    assert_eq!(engine.max_running.load(Ordering::SeqCst), 1);
    // Nothing is regenerated even if the caller re-sends a stale plan.
    worker.set_plan(work(&["a", "b", "c"]));
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(engine.calls().len(), 3);
}

#[tokio::test(start_paused = true)]
async fn an_in_flight_request_finishes_and_is_stored_when_the_plan_changes() {
    let engine = FakeEngine::new(2_000);
    let store = Arc::new(Store::default());
    let (tx, mut events) = mpsc::unbounded_channel();
    let worker = Worker::spawn(engine.clone(), store.clone(), options(), tx);
    worker.set_plan(work(&["old1", "old2"]));
    assert!(matches!(next(&mut events).await, WorkerEvent::Started(_)));
    worker.set_plan(work(&["new1"]));
    let mut order = vec![];
    while order.len() < 2 {
        if let WorkerEvent::Finished { job, .. } = next(&mut events).await {
            order.push(job.key);
        }
    }
    assert_eq!(order, vec!["old1", "new1"]);
    assert_eq!(engine.calls(), vec!["old1", "new1"]);
    assert_eq!(engine.max_running.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn failures_are_reported_and_backed_off_without_hot_looping() {
    let engine = FakeEngine::new(10);
    engine.fail.lock().unwrap().push("bad".into());
    let store = Arc::new(Store::default());
    let (tx, mut events) = mpsc::unbounded_channel();
    let worker = Worker::spawn(engine.clone(), store.clone(), options(), tx);
    worker.set_plan(work(&["bad", "good"]));
    let mut seen = vec![];
    while seen.len() < 2 {
        match next(&mut events).await {
            WorkerEvent::Failed { job, error } => {
                assert!(error.contains("down"));
                seen.push(format!("failed:{}", job.key));
            }
            WorkerEvent::Finished { job, .. } => seen.push(format!("ok:{}", job.key)),
            WorkerEvent::Started(_) => {}
        }
    }
    assert_eq!(seen, vec!["failed:bad", "ok:good"]);
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(
        engine.calls(),
        vec!["bad", "good"],
        "no retry inside the backoff window"
    );
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert_eq!(engine.calls().len(), 3, "retried once after the backoff");
}

#[tokio::test(start_paused = true)]
async fn routine_updates_do_not_reset_the_debounce() {
    let engine = FakeEngine::new(50);
    let store = Arc::new(Store::default());
    let (tx, mut events) = mpsc::unbounded_channel();
    let worker = Worker::spawn(engine.clone(), store.clone(), options(), tx);
    worker.set_plan(work(&["a"]));
    loop {
        if let WorkerEvent::Finished { .. } = next(&mut events).await {
            break;
        }
    }
    let finished_at = tokio::time::Instant::now();
    worker.update_plan(work(&["a", "b"]));
    assert!(matches!(next(&mut events).await, WorkerEvent::Started(job) if job.key == "b"));
    assert!(
        finished_at.elapsed() < Duration::from_millis(50),
        "{:?}",
        finished_at.elapsed()
    );
}

#[tokio::test(start_paused = true)]
async fn audio_deleted_after_it_was_made_is_made_again_once_forgotten() {
    let engine = FakeEngine::new(10);
    let store = Arc::new(Store::default());
    let (tx, mut events) = mpsc::unbounded_channel();
    let worker = Worker::spawn(engine.clone(), store.clone(), options(), tx);
    worker.set_plan(work(&["a"]));
    while !matches!(next(&mut events).await, WorkerEvent::Finished { .. }) {}

    // The cache limit deleted "a"; the next plan asks for it again.
    worker.update_plan(work(&["a"]));
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(
        events.try_recv().is_err(),
        "a key made in this run is skipped until forgotten"
    );

    worker.forget(["a".to_string()]);
    assert_eq!(
        next(&mut events).await,
        WorkerEvent::Started(work(&["a"])[0].job.clone())
    );
    assert_eq!(engine.calls(), vec!["a", "a"]);
}
