#![allow(dead_code)]

use aoide_sync::HeardWord;

/// Four short chapters of original prose, as chapter display text (blocks joined by "\n\n").
pub fn chapters() -> Vec<String> {
    [
        &[
            "Chapter One: The Keeper",
            "Odile Marsh kept the lamp at Gull Point for eleven winters before anyone thought to ask her why. She climbed the iron stair each dusk with a can of oil in one hand and a pocket loaf in the other, and she counted the steps aloud so the gulls would know she was coming.",
            "The lamp was first lit in 1865, when the harbour still smelled of tar and herring. Her grandmother had polished the same brass, and her mother had cursed the same draught that whistled under the gallery door on every north-easterly night.",
        ][..],
        &[
            "Chapter Two: The Letter",
            "A letter came in March, carried up the cliff path by a boy who would not stop talking about his new boots. It was addressed to the keeper in a careful, slanting hand, and it smelled faintly of cloves.",
            "Odile read it twice at the kitchen table, then folded it into quarters and tucked it behind the clock. The writer wanted to buy the light. Not the tower, not the cottage, only the lens, which he described as a marvel of the age wasted on fishermen.",
        ][..],
        &[
            "Chapter Three: The Storm",
            "The storm arrived three days early and stayed a week. Waves broke over the lower rocks and threw spray against the lantern glass so hard that Odile heard it from her bed, a sound like handfuls of gravel flung by an angry child.",
            "On the fourth night a schooner came round the headland with one mast gone and her sails in rags. Odile trimmed the wick, wound the clockwork that turned the lens, and stood at the rail with a lantern of her own until the crew found the channel.",
        ][..],
        &[
            "Chapter Four: The Answer",
            "When the weather cleared she took the letter from behind the clock and wrote her reply on the back of it. She did not explain the schooner or the gravel or the eleven winters. She wrote only that the lens was not for sale, and that marvels belonged where they were needed.",
            "The boy with the new boots carried it down the cliff path the next morning, and Odile climbed the iron stair that evening counting aloud, as she always had.",
        ][..],
    ]
    .iter()
    .map(|blocks| blocks.join("\n\n"))
    .collect()
}

/// A word as a transcriber reports it: text with its punctuation, and a span.
pub fn word(text: &str, start_ms: u64) -> HeardWord {
    let letters = text.chars().filter(|c| c.is_alphanumeric()).count() as u64;
    HeardWord {
        text: text.to_owned(),
        start_ms,
        end_ms: start_ms + 120 + 45 * letters,
    }
}

/// Reads `text` aloud from `from_ms`: one heard word per whitespace-separated word, with short
/// pauses between them and longer ones after sentences.
pub fn read(text: &str, from_ms: u64) -> Vec<HeardWord> {
    let mut at = from_ms;
    text.split_whitespace()
        .map(|w| {
            let heard = word(w, at);
            at = heard.end_ms
                + if w.ends_with(['.', '!', '?']) {
                    450
                } else {
                    60
                };
            heard
        })
        .collect()
}

/// Reads several texts one after another, with a pause between them.
pub fn read_all(texts: &[&str], from_ms: u64) -> Vec<HeardWord> {
    let mut words: Vec<HeardWord> = Vec::new();
    for text in texts {
        let start = words.last().map_or(from_ms, |w| w.end_ms + 1_500);
        words.extend(read(text, start));
    }
    words
}

pub fn strs(chapters: &[String]) -> Vec<&str> {
    chapters.iter().map(String::as_str).collect()
}
