use aoide_browser::ready::{next, ReadyEvent, ReadyRules, Stage, Watch};

fn rules(selector: bool) -> ReadyRules {
    ReadyRules {
        timeout_ms: 45_000,
        quiet_ms: 500,
        sample_ms: 300,
        selector,
    }
}

fn run(rules: &ReadyRules, events: &[ReadyEvent]) -> Watch {
    events
        .iter()
        .fold(Watch::start(0), |watch, event| next(watch, *event, rules))
}

use ReadyEvent::*;

#[test]
fn a_page_is_ready_once_loaded_quiet_and_its_text_stops_changing() {
    let rules = rules(false);
    let loading = run(&rules, &[RequestStarted { at: 10 }]);
    assert_eq!(loading.stage, Stage::Loading);

    let settling = run(&rules, &[RequestStarted { at: 10 }, Loaded { at: 200 }]);
    assert_eq!(settling.stage, Stage::Settling);
    assert!(!settling.wants_sample(), "the network is still busy");

    let events = [
        RequestStarted { at: 10 },
        Loaded { at: 200 },
        RequestEnded { at: 400 },
        Tick { at: 700 },
    ];
    assert_eq!(
        run(&rules, &events).stage,
        Stage::Settling,
        "quiet for only 300 ms"
    );
    let mut events = events.to_vec();
    events.push(Tick { at: 900 });
    let sampling = run(&rules, &events);
    assert_eq!(sampling.stage, Stage::Sampling);
    assert!(sampling.wants_sample());

    events.push(Sampled {
        at: 900,
        found: false,
        text_len: 1_200,
    });
    events.push(Sampled {
        at: 1_000,
        found: false,
        text_len: 1_200,
    });
    assert_eq!(
        run(&rules, &events).stage,
        Stage::Sampling,
        "samples too close together"
    );
    events.push(Sampled {
        at: 1_200,
        found: false,
        text_len: 1_500,
    });
    assert_eq!(run(&rules, &events).stage, Stage::Sampling, "still growing");
    events.push(Sampled {
        at: 1_500,
        found: false,
        text_len: 1_500,
    });
    assert_eq!(run(&rules, &events).stage, Stage::Ready);
}

#[test]
fn a_request_during_the_quiet_wait_starts_it_again() {
    let rules = rules(false);
    let watch = run(
        &rules,
        &[
            Loaded { at: 100 },
            Tick { at: 400 },
            RequestStarted { at: 450 },
            Tick { at: 700 },
            RequestEnded { at: 800 },
            Tick { at: 1_200 },
        ],
    );
    assert_eq!(watch.stage, Stage::Settling);
    assert_eq!(
        next(watch, Tick { at: 1_300 }, &rules).stage,
        Stage::Sampling
    );
}

#[test]
fn with_a_selector_the_page_is_ready_when_it_appears() {
    let rules = rules(true);
    let quiet = run(&rules, &[Loaded { at: 100 }, Tick { at: 600 }]);
    assert_eq!(quiet.stage, Stage::Sampling);
    let waiting = next(
        quiet,
        Sampled {
            at: 600,
            found: false,
            text_len: 50,
        },
        &rules,
    );
    let waiting = next(
        waiting,
        Sampled {
            at: 900,
            found: false,
            text_len: 50,
        },
        &rules,
    );
    assert_eq!(waiting.stage, Stage::Sampling, "stable text is not enough");
    let ready = next(
        waiting,
        Sampled {
            at: 1_200,
            found: true,
            text_len: 900,
        },
        &rules,
    );
    assert_eq!(ready.stage, Stage::Ready);
}

#[test]
fn a_page_that_never_settles_gives_up_at_the_timeout() {
    let plain = rules(false);
    let busy = run(&plain, &[RequestStarted { at: 10 }, Loaded { at: 100 }]);
    assert_eq!(
        next(busy, Tick { at: 44_999 }, &plain).stage,
        Stage::Settling
    );
    assert_eq!(
        next(busy, Tick { at: 45_000 }, &plain).stage,
        Stage::TimedOut
    );
    // Never loaded at all.
    let never = run(&plain, &[Tick { at: 50_000 }]);
    assert_eq!(never.stage, Stage::TimedOut);
    // Over: further events change nothing.
    assert_eq!(
        next(never, Loaded { at: 60_000 }, &plain).stage,
        Stage::TimedOut
    );
    let ready = run(
        &rules(true),
        &[
            Loaded { at: 1 },
            Tick { at: 600 },
            Sampled {
                at: 600,
                found: true,
                text_len: 9,
            },
        ],
    );
    assert_eq!(ready.stage, Stage::Ready);
    assert_eq!(
        next(ready, RequestStarted { at: 700 }, &plain).stage,
        Stage::Ready
    );
    assert!(Stage::Ready.is_over() && Stage::TimedOut.is_over() && !Stage::Sampling.is_over());
}
