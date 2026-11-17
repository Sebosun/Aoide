import { expect, it } from "vitest";
import type { ChapterView } from "@/api/types";
import { endsAt, formatDuration, speechMs, wordsLeft, segmentAt, chapterWords } from "@/estimate";

const chapter: ChapterView = {
  index: 0,
  title: "T",
  blocks: [
    {
      kind: "heading",
      level: 2,
      segments: [
        { id: "0:0:0", words: ["Title"], starts: [0], speakable: true, canSayAgain: true, note: false, notes: [] },
      ],
    },
    {
      kind: "paragraph",
      level: null,
      segments: [
        {
          id: "0:1:0",
          words: "one two three four".split(" "),
          starts: [7, 11, 15, 21],
          speakable: true,
          canSayAgain: true,
          note: false,
          notes: [],
        },
        { id: "0:1:1", words: ["* * *"], starts: [26], speakable: false, canSayAgain: false, note: false, notes: [] },
        {
          id: "0:1:2",
          words: "five six seven eight nine".split(" "),
          starts: [32, 37, 41, 47, 53],
          speakable: true,
          canSayAgain: true,
          note: false,
          notes: [],
        },
      ],
    },
  ],
  notNarrated: null,
};

it("estimates speech time from words and speed", () => {
  expect(speechMs(160, 1)).toBe(60_000);
  expect(speechMs(160, 2)).toBe(30_000);
});

it("says when a time left runs out by the clock, with the weekday when that is not today", () => {
  const now = new Date(2026, 10, 16, 21, 0);
  const at = (h: number, m: number) =>
    new Date(2026, 10, 16, h, m).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  expect(endsAt(2 * 3_600_000 + 40 * 60_000, now)).toBe(at(23, 40));
  expect(endsAt(5 * 3_600_000, now)).toMatch(/^\S+ \S/);
  expect(endsAt(5 * 3_600_000, now)).not.toBe(at(2, 0));
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
