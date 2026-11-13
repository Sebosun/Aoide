use aoide_transcribe::{quietest_cut, words_from_tokens, TokenTimes};

fn token(text: &str, t0: i64, t1: i64, t_dtw: i64) -> TokenTimes {
    TokenTimes {
        bytes: text.as_bytes().to_vec(),
        t0,
        t1,
        t_dtw,
        probability: 0.9,
        special: false,
    }
}

#[test]
fn tokens_join_into_words_at_leading_spaces() {
    let words = words_from_tokens(&[
        token(" Od", 0, 10, 2),
        token("ile", 10, 30, 12),
        token(" kept", 30, 50, 33),
        token(" the", 50, 60, 52),
        token(" lamp", 60, 90, 63),
        token(".", 90, 95, 91),
    ]);
    let texts: Vec<_> = words.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(texts, ["Odile", "kept", "the", "lamp."]);
}

#[test]
fn a_word_starts_at_its_dtw_time_and_ends_before_the_next_word() {
    let words = words_from_tokens(&[
        token(" kept", 30, 70, 33),
        token(" the", 50, 60, 52),
        token(" lamp", 60, 90, -1),
    ]);
    let spans: Vec<_> = words.iter().map(|w| (w.start_ms, w.end_ms)).collect();
    // Centiseconds to milliseconds; "lamp" has no DTW time and falls back to t0.
    assert_eq!(spans, [(330, 520), (520, 600), (600, 900)]);
}

#[test]
fn special_tokens_are_skipped_and_split_characters_rejoined() {
    let mut special = token("[_BEG_]", 0, 0, 0);
    special.special = true;
    let e_acute = "é".as_bytes();
    let mut first = token(" caf", 0, 10, 1);
    first.bytes.push(e_acute[0]);
    let second = TokenTimes {
        bytes: vec![e_acute[1]],
        ..token("", 10, 20, 11)
    };
    let words = words_from_tokens(&[special, first, second, token(" noir", 20, 40, 21)]);
    let texts: Vec<_> = words.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(texts, ["café", "noir"]);
}

#[test]
fn times_never_go_backwards() {
    let words = words_from_tokens(&[
        token(" one", 0, 50, 40),
        token(" two", 10, 20, 30),
        token(" three", 20, 30, 35),
    ]);
    for pair in words.windows(2) {
        assert!(pair[0].start_ms <= pair[1].start_ms);
        assert!(pair[0].end_ms <= pair[1].start_ms);
    }
    assert!(words.iter().all(|w| w.start_ms <= w.end_ms));
}

#[test]
fn a_block_is_cut_in_the_quietest_moment_of_the_window() {
    let rate = 1_000;
    let mut samples: Vec<f32> = (0..20_000)
        .map(|i| if i % 2 == 0 { 0.3 } else { -0.3 })
        .collect();
    // A pause from 13.2 s to 13.6 s, and a shorter, less quiet one at 11 s.
    samples[13_200..13_600].iter_mut().for_each(|s| *s = 0.0);
    samples[11_000..11_080].iter_mut().for_each(|s| *s *= 0.1);
    let cut = quietest_cut(&samples, rate, 10_000..16_000);
    assert!((13_200..13_600).contains(&cut), "cut at {cut}");
    // Outside the window, the pause is not chosen.
    let cut = quietest_cut(&samples, rate, 14_000..16_000);
    assert!((14_000..16_000).contains(&cut));
}
