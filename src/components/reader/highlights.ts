import type { ChapterView, HighlightView, SegmentView } from "@/api/types";

/** A highlight's part in one sentence: `start..end` in the sentence as the page draws it (its
 *  words joined by single spaces, in UTF-16 units as strings are sliced), and the highlight. */
export interface Mark {
  start: number;
  end: number;
  id: number;
}

/** What a sentence without highlights gets: always the same array, so a new highlight leaves
 *  its props as they were and it is not drawn again. */
export const NO_MARKS: readonly Mark[] = Object.freeze([]);

const units = (chars: string[], count: number) => chars.slice(0, count).join("").length;

/** Where `highlights` fall in `segment`. */
function marksIn(segment: SegmentView, highlights: HighlightView[]): Mark[] {
  const { words, starts } = segment;
  if (segment.note || !words.length || starts.length !== words.length) return [];
  const marks: Mark[] = [];
  for (const highlight of highlights) {
    let drawn = 0,
      open: Mark | null = null;
    words.forEach((word, i) => {
      const chars = Array.from(word),
        from = starts[i],
        to = from + chars.length;
      const a = Math.max(highlight.start, from),
        b = Math.min(highlight.end, to);
      if (a < b) {
        const start = drawn + units(chars, a - from),
          end = drawn + units(chars, b - from);
        // Joined with the word before when the highlight runs through the space between them.
        if (open && open.end === drawn - 1 && highlight.start < from) open.end = end;
        else marks.push((open = { start, end, id: highlight.id }));
      }
      drawn += word.length + 1;
    });
  }
  return marks.sort((x, y) => x.start - y.start);
}

const same = (a: readonly Mark[], b: readonly Mark[]) =>
  a.length === b.length && a.every((m, i) => m.start === b[i].start && m.end === b[i].end && m.id === b[i].id);

/** Each sentence's marks for the chapter's highlights that are drawn (not `detached`). A
 *  sentence whose marks are as they were in `previous` keeps that array, so only the sentences
 *  a change touches are drawn again. */
export function sentenceMarks(
  chapter: ChapterView,
  highlights: HighlightView[],
  previous?: Map<string, readonly Mark[]>,
): Map<string, readonly Mark[]> {
  const here = highlights.filter((h) => h.chapter === chapter.index && !h.detached);
  const result = new Map<string, readonly Mark[]>();
  for (const block of chapter.blocks)
    for (const segment of block.segments) {
      if (!here.length) {
        result.set(segment.id, NO_MARKS);
        continue;
      }
      const marks = marksIn(segment, here);
      const before = previous?.get(segment.id);
      result.set(segment.id, !marks.length ? NO_MARKS : before && same(before, marks) ? before : marks);
    }
  return result;
}

/** `text`, which starts `offset` units into its sentence as drawn, cut where `marks` begin and
 *  end: each piece with its highlight's id, or `null`. */
export function pieces(text: string, offset: number, marks: readonly Mark[]): { text: string; id: number | null }[] {
  if (!marks.length) return [{ text, id: null }];
  const out: { text: string; id: number | null }[] = [];
  let at = 0;
  for (const mark of marks) {
    const a = Math.max(mark.start - offset, at),
      b = Math.min(mark.end - offset, text.length);
    if (b <= a) continue;
    if (a > at) out.push({ text: text.slice(at, a), id: null });
    out.push({ text: text.slice(a, b), id: mark.id });
    at = b;
  }
  if (at < text.length) out.push({ text: text.slice(at), id: null });
  return out;
}
