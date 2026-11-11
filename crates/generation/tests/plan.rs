use aoide_generation::{
    buffered_ahead_ms, estimate_ms, plan, Job, Listening, PlanItem, Preparing, Reason,
};

fn item(key: &str, cached: bool, ms: u32) -> PlanItem {
    PlanItem {
        key: key.into(),
        speakable: true,
        cached,
        duration_ms: ms,
    }
}

fn silent(key: &str) -> PlanItem {
    PlanItem {
        key: key.into(),
        speakable: false,
        cached: false,
        duration_ms: 0,
    }
}

fn keys(jobs: &[Job]) -> Vec<&str> {
    jobs.iter().map(|j| j.key.as_str()).collect()
}

#[test]
fn estimates_about_fifteen_chars_per_second() {
    assert_eq!(estimate_ms(150), 10_000);
    assert_eq!(estimate_ms(0), 0);
}

#[test]
fn lookahead_takes_uncached_speakable_items_in_window_from_playhead() {
    let items = vec![
        item("a", false, 1000),
        item("b", true, 1000),
        silent("gap"),
        item("c", false, 1000),
        item("d", false, 1000),
        item("e", false, 1000),
    ];
    let listening = Listening {
        book: "bk",
        items: &items,
        playhead: 1,
        lookahead_ms: 2500,
    };
    let jobs = plan(Some(&listening), &[]);
    // b (cached, 1000) + c (1000) + d (1000) reaches 2500; e is outside the window; a is behind.
    assert_eq!(keys(&jobs), vec!["c", "d"]);
    assert!(jobs
        .iter()
        .all(|j| j.reason == Reason::Playhead && j.book == "bk"));
    assert_eq!(jobs[0].index, 3);
}

#[test]
fn prepare_follows_lookahead_and_wraps_around_the_listened_book() {
    let items = vec![
        item("a", false, 1000),
        item("b", false, 1000),
        item("c", false, 1000),
    ];
    let other = vec![item("x", false, 1000), item("y", true, 1000)];
    let listening = Listening {
        book: "bk",
        items: &items,
        playhead: 1,
        lookahead_ms: 500,
    };
    let jobs = plan(
        Some(&listening),
        &[
            Preparing {
                book: "other",
                items: &other,
            },
            Preparing {
                book: "bk",
                items: &items,
            },
        ],
    );
    assert_eq!(keys(&jobs), vec!["b", "x", "c", "a"]);
    assert_eq!(jobs[0].reason, Reason::Playhead);
    assert_eq!(jobs[1].reason, Reason::Prepare);
    assert_eq!(jobs[1].book, "other");
}

#[test]
fn duplicate_keys_are_planned_once() {
    let items = vec![
        item("yes", false, 500),
        item("no", false, 500),
        item("yes", false, 500),
    ];
    let listening = Listening {
        book: "bk",
        items: &items,
        playhead: 0,
        lookahead_ms: 10_000,
    };
    assert_eq!(keys(&plan(Some(&listening), &[])), vec!["yes", "no"]);
}

#[test]
fn nothing_to_do_without_listening_or_preparing() {
    assert!(plan(None, &[]).is_empty());
    let items = vec![item("a", true, 1)];
    let listening = Listening {
        book: "bk",
        items: &items,
        playhead: 5,
        lookahead_ms: 1000,
    };
    assert!(
        plan(Some(&listening), &[]).is_empty(),
        "playhead past the end"
    );
}

#[test]
fn buffered_ahead_counts_contiguous_cached_audio() {
    let items = vec![
        item("a", true, 400),
        silent("s"),
        item("b", true, 600),
        item("c", false, 900),
        item("d", true, 1000),
    ];
    assert_eq!(buffered_ahead_ms(&items, 0), 1000);
    assert_eq!(buffered_ahead_ms(&items, 3), 0);
    assert_eq!(buffered_ahead_ms(&items, 9), 0);
}
