import type { ChapterView, SegmentView } from "./api/types";

/** Typical narration pace at 1×, used where real audio durations are not known yet. */
export const WORDS_PER_MINUTE = 160;

export const speechMs = (words: number, speed = 1) => (words / WORDS_PER_MINUTE) * 60_000 / Math.max(0.1, speed);

export function formatDuration(ms: number): string {
  const minutes = Math.round(ms / 60_000);
  if (ms < 60_000 || minutes < 1) return "<1 min";
  const h = Math.floor(minutes / 60), m = minutes % 60;
  return h ? `${h} h ${m} min` : `${m} min`;
}

const spoken = (chapter: ChapterView): SegmentView[] => chapter.blocks.flatMap(b => b.segments).filter(s => s.speakable);

export const chapterWords = (chapter: ChapterView) => spoken(chapter).reduce((n, s) => n + s.words.length, 0);

/** Spoken words from `segmentId` (from `wordIndex` within it) to the end of the chapter. */
export function wordsLeft(chapter: ChapterView, segmentId: string | null, wordIndex: number | null): number {
  const segments = spoken(chapter);
  const at = segments.findIndex(s => s.id === segmentId);
  if (at < 0) return chapterWords(chapter);
  return segments.slice(at).reduce((n, s) => n + s.words.length, 0) - (wordIndex ?? 0);
}

/** The speakable segment holding the word at `fraction` (0..1) of the chapter's spoken words. */
export function segmentAt(chapter: ChapterView, fraction: number): string | null {
  const segments = spoken(chapter);
  const target = Math.min(1, Math.max(0, fraction)) * chapterWords(chapter);
  let seen = 0;
  for (const segment of segments) {
    seen += segment.words.length;
    if (seen > target) return segment.id;
  }
  return segments.at(-1)?.id ?? null;
}
