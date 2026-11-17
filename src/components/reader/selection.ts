import type { ChapterView, SegmentView } from "@/api/types";

/** What a selection on the page covers, in the chapter's display text: `start..end` are
 *  character offsets (Unicode scalar values, as the core counts them, like `Segment::range`),
 *  `text` the words as the page shows them (single spaces, blocks apart by a blank line), and
 *  `firstSegmentId` the sentence it starts in. */
export interface TextRange {
  chapter: number;
  start: number;
  end: number;
  text: string;
  firstSegmentId: string;
}

/** Characters, as the core counts them (not UTF-16 units). */
const length = (text: string) => Array.from(text).length;

interface Sentence {
  element: Element;
  segment: SegmentView;
  /** Index of its block in the chapter. */
  block: number;
}
/** A place in a sentence: before character `char` of word `word` (`char` may be the word's
 *  length: just after it). */
interface Place {
  sentence: number;
  word: number;
  char: number;
}

/** The sentences drawn in `root` that are the chapter's text, in reading order: not a footnote
 *  read aloud (`note`), which is not in the text. */
function sentences(root: HTMLElement, chapter: ChapterView): Sentence[] {
  const byId = new Map<string, { segment: SegmentView; block: number }>();
  chapter.blocks.forEach((block, index) => {
    for (const segment of block.segments)
      if (!segment.note && segment.starts.length === segment.words.length && segment.words.length)
        byId.set(segment.id, { segment, block: index });
  });
  const drawn: Sentence[] = [];
  for (const element of root.querySelectorAll("[data-segment-id]")) {
    const found = byId.get(element.getAttribute("data-segment-id") ?? "");
    if (found) drawn.push({ element, ...found });
  }
  return drawn;
}

/** How many of a sentence's drawn characters come before the boundary point: its text nodes,
 *  leaving out what is not the book's text (`data-not-text`: footnote markers). */
function charactersBefore(element: Element, boundary: Range): number {
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
  let count = 0;
  for (let node = walker.nextNode() as Text | null; node; node = walker.nextNode() as Text | null) {
    if (node.parentElement?.closest("[data-not-text]")) continue;
    if (boundary.startContainer === node) return count + length(node.data.slice(0, boundary.startOffset));
    // A text node the boundary is not in lies wholly before it or wholly after it.
    if (boundary.comparePoint(node, node.length) > 0) break;
    count += length(node.data);
  }
  return count;
}

/** The word place `at` characters into a sentence drawn as its words joined by single spaces.
 *  A start takes the word after it when it is at a word's end or in the space after it; an end
 *  takes the word before it when it is at a word's start or in the space before it (`{ word: 0,
 *  char: 0 }`: before the first word). Past the last word, a start is the word count. */
function place(words: string[], at: number, side: "start" | "end"): { word: number; char: number } {
  let from = 0;
  for (let word = 0; word < words.length; word++) {
    const size = length(words[word]);
    if (side === "start" && at < from + size) return { word, char: Math.max(0, at - from) };
    if (side === "end" && at <= from && word > 0) return { word: word - 1, char: length(words[word - 1]) };
    if (side === "end" && at <= from + size) return { word, char: Math.max(0, at - from) };
    from += size + 1;
  }
  return side === "end"
    ? { word: words.length - 1, char: length(words[words.length - 1]) }
    : { word: words.length, char: 0 };
}

/** The end of sentence `index`'s last word. */
function endOf(all: Sentence[], index: number): Place {
  const words = all[index].segment.words;
  return { sentence: index, word: words.length - 1, char: length(words[words.length - 1]) };
}

/** Where a boundary point of the selection falls among the sentences: inside one, or between
 *  them (a start takes the first word after it, an end the last word before it). `null` when
 *  no sentence's word lies on that side. */
function locate(all: Sentence[], boundary: Range, side: "start" | "end"): Place | null {
  const inside = all.findIndex((s) => s.element.contains(boundary.startContainer));
  if (inside >= 0) {
    const { words } = all[inside].segment;
    const at = place(words, charactersBefore(all[inside].element, boundary), side);
    if (side === "start") {
      if (at.word < words.length) return { sentence: inside, ...at };
      return inside + 1 < all.length ? { sentence: inside + 1, word: 0, char: 0 } : null;
    }
    if (at.word > 0 || at.char > 0) return { sentence: inside, ...at };
    return inside > 0 ? endOf(all, inside - 1) : null;
  }
  if (side === "start") {
    const next = all.findIndex((s) => boundary.comparePoint(s.element, 0) > 0);
    return next >= 0 ? { sentence: next, word: 0, char: 0 } : null;
  }
  const after = all.findIndex((s) => boundary.comparePoint(s.element, s.element.childNodes.length) >= 0);
  const last = (after < 0 ? all.length : after) - 1;
  return last < 0 ? null : endOf(all, last);
}

const offset = (all: Sentence[], at: Place) => all[at.sentence].segment.starts[at.word] + at.char;

/** The words from `from` to `to` as the page shows them: single spaces, and a blank line
 *  between blocks. */
function textBetween(all: Sentence[], from: Place, to: Place): string {
  let text = "";
  for (let s = from.sentence; s <= to.sentence; s++) {
    const { words } = all[s].segment;
    if (s > from.sentence) text += all[s].block === all[s - 1].block ? " " : "\n\n";
    const first = s === from.sentence ? from.word : 0,
      last = s === to.sentence ? to.word : words.length - 1;
    for (let w = first; w <= last; w++) {
      const chars = Array.from(words[w]);
      const begin = s === from.sentence && w === from.word ? from.char : 0,
        end = s === to.sentence && w === to.word ? to.char : chars.length;
      text += (w > first ? " " : "") + chars.slice(begin, end).join("");
    }
  }
  return text;
}

const collapsedAt = (node: Node, at: number) => {
  const range = document.createRange();
  range.setStart(node, at);
  return range;
};

/** What `selection` covers of `chapter`, drawn in `chapterRoot` (the page, or the Lines view):
 *  sentences are the elements with `data-segment-id`; footnote markers and margin notes
 *  (`data-not-text`) and footnotes read aloud are not counted. `null` when it covers none of
 *  the chapter's words. */
export function rangeOfSelection(
  selection: Selection,
  chapterRoot: HTMLElement,
  chapter: ChapterView,
): TextRange | null {
  if (!selection.rangeCount || selection.isCollapsed) return null;
  const first = selection.getRangeAt(0),
    last = selection.getRangeAt(selection.rangeCount - 1);
  const all = sentences(chapterRoot, chapter);
  if (!all.length) return null;
  const from = locate(all, collapsedAt(first.startContainer, first.startOffset), "start"),
    to = locate(all, collapsedAt(last.endContainer, last.endOffset), "end");
  if (!from || !to) return null;
  const start = offset(all, from),
    end = offset(all, to);
  if (end <= start) return null;
  return {
    chapter: chapter.index,
    start,
    end,
    text: textBetween(all, from, to),
    firstSegmentId: all[from.sentence].segment.id,
  };
}

/** Whether a click on a sentence is one to play it: with the main button (WebKit also clicks
 *  on a right click, which opens the text menu), and not the click that ends a drag selecting
 *  words. */
export function clickToPlay(event: MouseEvent): boolean {
  const selection = document.getSelection();
  return event.button === 0 && !(selection && !selection.isCollapsed);
}

/** Most words a selection may have to be said differently: a name or a term. */
const FEW_WORDS = 4;
/** The words of `text` without the punctuation around them, when they are few enough to be a
 *  name or a term (to say differently); else "". */
export function fewWords(text: string): string {
  const words = text
    .replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}.]+$/gu, "")
    .split(/\s+/)
    .filter(Boolean);
  return words.length && words.length <= FEW_WORDS ? words.join(" ") : "";
}

/** The DOM point `at` characters into a sentence drawn as its words joined by single spaces
 *  (footnote markers, `data-not-text`, not counted). */
function pointIn(element: Element, at: number): [Node, number] {
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
  let left = at,
    last: [Node, number] = [element, 0];
  for (let node = walker.nextNode() as Text | null; node; node = walker.nextNode() as Text | null) {
    if (node.parentElement?.closest("[data-not-text]")) continue;
    const chars = Array.from(node.data);
    if (left <= chars.length) return [node, chars.slice(0, left).join("").length];
    left -= chars.length;
    last = [node, node.length];
  }
  return last;
}

/** Selects in a sentence drawn in `element`, as Shift and an arrow key start a selection on it:
 *  its first word (`"first"`), its last word, selected backwards so that the keys extend it
 *  further back (`"last"`), or all of it (`"all"`, backwards when `backwards`). */
export function selectInSentence(element: Element, words: string[], part: "first" | "last" | "all", backwards = false) {
  const selection = document.getSelection();
  if (!selection || !words.length) return;
  const start = (index: number) => words.slice(0, index).reduce((sum, word) => sum + length(word) + 1, 0);
  const end = start(words.length - 1) + length(words[words.length - 1]);
  const [from, to] =
    part === "first" ? [0, length(words[0])] : part === "last" ? [end, start(words.length - 1)] : [0, end];
  const [anchor, focus] = (part === "all" && backwards ? [to, from] : [from, to]).map((at) => pointIn(element, at));
  selection.setBaseAndExtent(anchor[0], anchor[1], focus[0], focus[1]);
}
