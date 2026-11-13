mod support;

use aoide_sync::{decode_chapters, encode_chapters, found_in_text, match_book, Loudness, Report};
use support::{chapters, read, read_all, strs};

#[test]
fn a_sync_map_and_its_report_survive_storage() {
    let chapters = chapters();
    let text = strs(&chapters);
    let heard = read_all(&[text[0], text[1], text[3]], 0);
    let map = match_book(&text, &heard);

    let bytes = encode_chapters(&map.chapters);
    assert_eq!(decode_chapters(&bytes).unwrap(), map.chapters);
    assert!(decode_chapters(&bytes[..bytes.len() - 1]).is_none());

    let json = serde_json::to_string(&map.report).unwrap();
    assert_eq!(serde_json::from_str::<Report>(&json).unwrap(), map.report);
}

#[test]
fn loudness_is_low_in_silence_and_high_in_speech_frame_by_frame() {
    let rate = 16_000;
    let mut samples = vec![0.0f32; 1_600]; // 100 ms of silence
    samples.extend((0..1_600).map(|n| 0.5 * (n as f32 * 0.2).sin())); // 100 ms of tone
    samples.extend(vec![0.0f32; 800]); // 50 ms of silence
    let loudness = Loudness::measure(&samples, rate, 10);

    assert_eq!(loudness.frame_ms, 10);
    assert_eq!(loudness.levels.len(), 25);
    assert!(loudness.levels[..10].iter().all(|l| *l == 0));
    assert!(loudness.levels[10..20].iter().all(|l| *l > 200));
    assert!(loudness.levels[20..].iter().all(|l| *l == 0));
}

#[test]
fn a_stretch_read_from_the_book_is_found_in_it_and_other_narration_is_not() {
    let chapters = chapters();
    let text = strs(&chapters);
    let from_the_book = read(&text[2][..400], 60_000);
    assert!(found_in_text(&text, &from_the_book) > 0.8);

    let elsewhere = read(
        "The orchard behind the mill had not been pruned in years, and the pears that fell \
         there were left for the wasps and the badgers, who quarrelled over them at dusk.",
        0,
    );
    assert!(found_in_text(&text, &elsewhere) < 0.1);
    assert_eq!(found_in_text(&text, &[]), 0.0);
}
