//! What went wrong with the speech engine lately, for Settings › Speech engine. The player says
//! what happened in a few words ("The speech engine is unavailable"); the whole error (an
//! address, a transport error, an HTTP status) is kept here, and in the log.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::Serialize;

use super::lock;

/// Problems kept at most; older ones are dropped.
const KEPT: usize = 50;

/// One problem, or a run of the same one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SpeechProblem {
    /// What went wrong, said for people.
    pub message: String,
    /// The whole error.
    pub detail: String,
    /// When it first happened in this run, in ms since the epoch.
    pub first_ms: i64,
    /// When it last happened.
    pub last_ms: i64,
    /// How many times in a row it happened.
    pub times: u32,
}

#[derive(Default)]
pub(super) struct SpeechProblems {
    /// Oldest first.
    list: Mutex<VecDeque<SpeechProblem>>,
}

impl SpeechProblems {
    /// Keeps a problem. The same problem again, with nothing else in between, counts as a run
    /// (a server that is down fails every request alike).
    pub fn record(&self, message: &str, detail: &str, now_ms: i64) {
        let mut list = lock(&self.list);
        if let Some(last) = list
            .back_mut()
            .filter(|p| p.message == message && p.detail == detail)
        {
            last.times += 1;
            last.last_ms = now_ms;
            return;
        }
        if list.len() == KEPT {
            list.pop_front();
        }
        list.push_back(SpeechProblem {
            message: message.to_owned(),
            detail: detail.to_owned(),
            first_ms: now_ms,
            last_ms: now_ms,
            times: 1,
        });
    }

    /// The problems kept, newest first.
    pub fn list(&self) -> Vec<SpeechProblem> {
        lock(&self.list).iter().rev().cloned().collect()
    }

    pub fn clear(&self) {
        lock(&self.list).clear();
    }
}

impl super::Core {
    /// What went wrong with the speech engine lately, newest first.
    pub fn speech_problems(&self) -> Vec<SpeechProblem> {
        self.speech_problems.list()
    }

    pub fn clear_speech_problems(&self) {
        self.speech_problems.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOWN: &str = "The speech engine is unavailable";

    #[test]
    fn the_same_problem_again_is_one_run_and_the_newest_comes_first() {
        let problems = SpeechProblems::default();
        problems.record(DOWN, "engine is unavailable: connection refused", 1);
        problems.record(DOWN, "engine is unavailable: connection refused", 5);
        problems.record(
            "The speech engine took too long to answer",
            "engine request timed out",
            9,
        );
        let list = problems.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].detail, "engine request timed out");
        assert_eq!(
            (list[1].times, list[1].first_ms, list[1].last_ms),
            (2, 1, 5),
            "{list:?}"
        );
    }

    #[test]
    fn only_the_latest_are_kept_and_they_can_be_cleared() {
        let problems = SpeechProblems::default();
        for i in 0..KEPT as i64 + 5 {
            problems.record(DOWN, &format!("attempt {i}"), i);
        }
        let list = problems.list();
        assert_eq!(list.len(), KEPT);
        assert_eq!(list[0].detail, format!("attempt {}", KEPT + 4));
        problems.clear();
        assert!(problems.list().is_empty());
    }
}
