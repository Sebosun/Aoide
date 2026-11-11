import { expect, it } from "vitest";
import type { ChapterView } from "../api/types";
import { formatDuration, speechMs, wordsLeft, segmentAt, chapterWords } from "../estimate";

const chapter: ChapterView = { index: 0, title: "T", blocks: [
  { kind: "heading", level: 2, segments: [{ id: "0:0:0", words: ["Title"], speakable: true, note: false, notes: [] }] },
  { kind: "paragraph", level: null, segments: [
    { id: "0:1:0", words: "one two three four".split(" "), speakable: true, note: false, notes: [] },
    { id: "0:1:1", words: ["* * *"], speakable: false, note: false, notes: [] },
    { id: "0:1:2", words: "five six seven eight nine".split(" "), speakable: true, note: false, notes: [] },
  ] },
] };

it("estimates speech time from words and speed", () => {
  expect(speechMs(160, 1)).toBe(60_000);
  expect(speechMs(160, 2)).toBe(30_000);
});

it("formats durations for labels", () => {
  expect(formatDuration(20_000)).toBe("<1 min");
  expect(formatDuration(12 * 60_000 + 20_000)).toBe("12 min");
  expect(formatDuration(65 * 60_000)).toBe("1 h 5 min");
  expect(formatDuration(6 * 3_600_000 + 20 * 60_000)).toBe("6 h 20 min");
});

it("counts the spoken words left from a segment, including it", () => {
  expect(chapterWords(chapter)).toBe(10);
  expect(wordsLeft(chapter, "0:1:2", null)).toBe(5);
  expect(wordsLeft(chapter, "0:1:2", 3)).toBe(2);
  expect(wordsLeft(chapter, null, null)).toBe(10);
});

it("finds the speakable segment at a share of the chapter", () => {
  expect(segmentAt(chapter, 0)).toBe("0:0:0");
  expect(segmentAt(chapter, 0.3)).toBe("0:1:0");
  expect(segmentAt(chapter, 0.55)).toBe("0:1:2");
  expect(segmentAt(chapter, 1)).toBe("0:1:2");
});
