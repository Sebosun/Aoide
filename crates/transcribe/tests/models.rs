use aoide_transcribe::models::{model, model_for, ModelSize, MODELS};

#[test]
fn english_books_get_the_english_model_and_others_the_multilingual_one() {
    assert_eq!(model_for(ModelSize::Base, Some("en-GB")).id, "base.en");
    assert_eq!(model_for(ModelSize::Base, None).id, "base.en");
    assert_eq!(model_for(ModelSize::Small, Some("de")).id, "small");
    assert_eq!(model_for(ModelSize::Tiny, Some("EN")).id, "tiny.en");
}

#[test]
fn every_model_has_a_pinned_checksum_and_a_known_name() {
    for m in MODELS {
        assert_eq!(m.sha256.len(), 64, "{}", m.id);
        assert!(m.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(model(m.id), Some(m));
        assert!(m.url().ends_with(&format!("ggml-{}.bin", m.id)));
    }
}

#[test]
fn word_times_from_a_known_model_are_moved_earlier_by_its_measured_lateness() {
    use aoide_transcribe::models::lateness_ms;
    assert_eq!(lateness_ms("whisper.cpp:ggml-base.en.bin"), 60);
    assert_eq!(lateness_ms("whisper.cpp:ggml-small.en.bin"), 110);
    assert_eq!(lateness_ms("whisper.cpp:ggml-tiny.bin"), 60);
    assert_eq!(lateness_ms("whisper.cpp:my-own-model.bin"), 0);
    assert_eq!(lateness_ms("scripted"), 0);
}
