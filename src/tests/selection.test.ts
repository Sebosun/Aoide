/** What a selection on the page covers, in the chapter's display text: the core's chapters
 *  (`fixtures/chapter-texts.json`, written with their text by src-tauri/src/core/fixtures.rs)
 *  drawn by ChapterText, selected the way a person's drag leaves the DOM. */
import { afterEach, expect, it } from "vitest";
import { mount, type VueWrapper } from "@vue/test-utils";
import type { ChapterView, SegmentView } from "@/api/types";
import ChapterText from "@/components/reader/ChapterText.vue";
import { rangeOfSelection } from "@/components/reader/selection";
import fixture from "@/tests/fixtures/chapter-texts.json";

const chapters = fixture as unknown as { text: string; chapter: ChapterView }[];
const shapes = chapters[chapters.length - 1];
const preview = chapters[0];
const length = (word: string) => Array.from(word).length;
/** The chapter's display text from `start` to `end`, counted in Unicode scalar values. */
const slice = (text: string, start: number, end: number) => Array.from(text).slice(start, end).join("");

let wrapper: VueWrapper | null = null;
afterEach(() => {
  wrapper?.unmount();
  wrapper = null;
  document.getSelection()?.removeAllRanges();
});
function draw(chapter: ChapterView, options: { marginNotes?: boolean; current?: string } = {}) {
  wrapper = mount(ChapterText, {
    attachTo: document.body,
    props: {
      chapter,
      currentSegment: options.current ?? null,
      currentWord: options.current ? 1 : null,
      marginNotes: options.marginNotes ?? false,
    },
  });
  return wrapper.element as HTMLElement;
}
const segment = (chapter: ChapterView, id: string) =>
  chapter.blocks.flatMap((b) => b.segments).find((s) => s.id === id) as SegmentView;

/** The DOM point `at` characters into a sentence's drawn words, not counting note markers. */
function point(root: HTMLElement, id: string, at: number): [Node, number] {
  const element = root.querySelector(`[data-segment-id="${id}"]`)!;
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
  let left = at;
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (node.parentElement?.closest("[data-not-text]")) continue;
    const chars = Array.from((node as Text).data);
    if (left <= chars.length) return [node, chars.slice(0, left).join("").length];
    left -= chars.length;
  }
  throw new Error(`no point ${at} in ${id}`);
}
/** The point at the start (or the end) of word `index` of a sentence. */
function wordPoint(root: HTMLElement, chapter: ChapterView, id: string, index: number, end = false) {
  const words = segment(chapter, id).words;
  const before = words.slice(0, index).reduce((sum, word) => sum + length(word) + 1, 0);
  return point(root, id, before + (end ? length(words[index]) : 0));
}
function select(from: [Node, number], to: [Node, number]) {
  const range = document.createRange();
  range.setStart(...from);
  range.setEnd(...to);
  const selection = document.getSelection()!;
  selection.removeAllRanges();
  selection.addRange(range);
  return selection;
}

it("counts every word of every fixture chapter where the segmenter does", () => {
  let words = 0;
  for (const { text, chapter } of chapters) {
    const root = draw(chapter);
    for (const sentence of chapter.blocks.flatMap((b) => b.segments).filter((s) => !s.note)) {
      sentence.words.forEach((word, i) => {
        const selection = select(
          wordPoint(root, chapter, sentence.id, i),
          wordPoint(root, chapter, sentence.id, i, true),
        );
        const range = rangeOfSelection(selection, root, chapter);
        expect(range).toEqual({
          chapter: chapter.index,
          start: sentence.starts[i],
          end: sentence.starts[i] + length(word),
          text: word,
          firstSegmentId: sentence.id,
        });
        expect(slice(text, range!.start, range!.end)).toBe(word);
        words++;
      });
    }
    wrapper?.unmount();
  }
  expect(words).toBeGreaterThan(200);
});

it("covers words within one sentence, without the footnote marker inside them", () => {
  const { text, chapter } = shapes;
  const root = draw(chapter);
  const range = rangeOfSelection(
    select(wordPoint(root, chapter, "0:1:0", 2), wordPoint(root, chapter, "0:1:0", 4, true)),
    root,
    chapter,
  )!;
  expect(range.text).toBe("harbour was quiet");
  expect(slice(text, range.start, range.end)).toBe("harbour was quiet");
  expect(range.firstSegmentId).toBe("0:1:0");
});

it("covers words across sentences and across paragraphs", () => {
  const { text, chapter } = preview;
  let root = draw(chapter);
  let range = rangeOfSelection(
    select(wordPoint(root, chapter, "0:1:0", 3), wordPoint(root, chapter, "0:1:1", 2, true)),
    root,
    chapter,
  )!;
  expect(range.text).toBe(slice(text, range.start, range.end));
  expect(range.text).toMatch(/^carried a lantern .* pier\. The sea collected$/);
  expect(range.firstSegmentId).toBe("0:1:0");

  wrapper?.unmount();
  root = draw(chapter);
  range = rangeOfSelection(
    select(wordPoint(root, chapter, "0:1:1", 0), wordPoint(root, chapter, "0:2:0", 0, true)),
    root,
    chapter,
  )!;
  expect(range.text).toMatch(/^The sea .* little stars\.\n\nTonight$/);
  expect(range.text).toBe(slice(text, range.start, range.end));
});

it("leaves out a footnote read aloud between paragraphs", () => {
  const { text, chapter } = shapes;
  const root = draw(chapter);
  const range = rangeOfSelection(
    select(wordPoint(root, chapter, "0:1:0", 14), wordPoint(root, chapter, "0:2:0", 1, true)),
    root,
    chapter,
  )!;
  expect(range).toMatchObject({ start: 80, end: 97, text: "night.\n\nfn main()" });
  expect(slice(text, 80, 97)).toBe(range.text);
});

it("counts astral characters and combining accents as the core does", () => {
  const { text, chapter } = shapes;
  const root = draw(chapter, { current: "0:1:0" });
  const word = (index: number) =>
    rangeOfSelection(
      select(wordPoint(root, chapter, "0:1:0", index), wordPoint(root, chapter, "0:1:0", index, true)),
      root,
      chapter,
    )!;
  expect(word(1)).toMatchObject({ start: 12, end: 13, text: "🌊" });
  expect(word(10)).toMatchObject({ start: 56, end: 61, text: "Café" });
  expect(word(13)).toMatchObject({ start: 76, end: 79, text: "𝒶ll" });
  // Part of a word, after an emoji: its characters, not its UTF-16 units.
  const part = rangeOfSelection(select(point(root, "0:1:0", 6), point(root, "0:1:0", 9)), root, chapter)!;
  expect(part).toMatchObject({ start: 14, end: 17, text: "har" });
  expect(slice(text, 14, 17)).toBe("har");
});

it("takes ends between sentences and outside them to the nearest word in the selection", () => {
  const { chapter } = shapes;
  const root = draw(chapter, { marginNotes: true });
  const sentence = root.querySelector('[data-segment-id="0:1:0"]')!;
  const code = root.querySelector('[data-segment-id="0:2:0"]')!;
  // A triple click: from the paragraph's start to the start of the next block.
  const paragraph = rangeOfSelection(select([sentence, 0], [code.parentElement!, 0]), root, chapter)!;
  expect(paragraph).toMatchObject({ start: 8, end: 86, firstSegmentId: "0:1:0" });
  // From the space after a paragraph's last word: the next block's first word.
  const after = Array.from(segment(chapter, "0:1:0").words.join(" ")).length + 1;
  const next = rangeOfSelection(
    select(point(root, "0:1:0", after), wordPoint(root, chapter, "0:2:0", 0, true)),
    root,
    chapter,
  )!;
  expect(next).toMatchObject({ start: 88, end: 90, text: "fn", firstSegmentId: "0:2:0" });
  // Into the margin note beside the paragraph: up to its last word.
  const margin = root.querySelector(".chapter-text__margin-note")!;
  const into = rangeOfSelection(select(wordPoint(root, chapter, "0:1:0", 7), [margin.lastChild!, 3]), root, chapter)!;
  expect(into).toMatchObject({ start: 41, end: 86 });
  expect(into.text).not.toContain("Built");
});

it("starts and ends on words when a drag starts just after a word or ends just before one", () => {
  const { text, chapter } = shapes;
  let root = draw(chapter);
  // From right after "🌊" to right before "when": the words between, no spaces around them.
  const within = rangeOfSelection(
    select(wordPoint(root, chapter, "0:1:0", 1, true), wordPoint(root, chapter, "0:1:0", 5)),
    root,
    chapter,
  )!;
  expect(within).toMatchObject({ start: 14, end: 31, text: "harbour was quiet" });
  expect(slice(text, within.start, within.end)).toBe(within.text);

  wrapper?.unmount();
  root = draw(preview.chapter);
  // To the start of the next sentence's first word: up to the end of "pier.".
  const across = rangeOfSelection(
    select(wordPoint(root, preview.chapter, "0:1:0", 3), wordPoint(root, preview.chapter, "0:1:1", 0)),
    root,
    preview.chapter,
  )!;
  expect(across.text).toMatch(/^carried .* pier\.$/);
  expect(slice(preview.text, across.start, across.end)).toBe(across.text);
  // Nothing but the space between two words covers no word.
  expect(
    rangeOfSelection(
      select(wordPoint(root, preview.chapter, "0:1:0", 3, true), wordPoint(root, preview.chapter, "0:1:0", 4)),
      root,
      preview.chapter,
    ),
  ).toBeNull();
});

it("covers nothing for a footnote marker alone, an empty selection or one outside the chapter", () => {
  const { chapter } = shapes;
  const root = draw(chapter);
  const marker = root.querySelector("[data-not-text]")!;
  expect(rangeOfSelection(select([marker, 0], [marker, marker.childNodes.length]), root, chapter)).toBeNull();
  const at = wordPoint(root, chapter, "0:1:0", 3);
  expect(rangeOfSelection(select(at, at), root, chapter)).toBeNull();
  const outside = document.createElement("p");
  outside.textContent = "Elsewhere";
  document.body.append(outside);
  expect(rangeOfSelection(select([outside.firstChild!, 0], [outside.firstChild!, 4]), root, chapter)).toBeNull();
  outside.remove();
});

it("does not play a sentence when a drag selected words in it; a plain click still plays", async () => {
  const { chapter } = shapes;
  const root = draw(chapter);
  const sentence = wrapper!.get('[data-segment-id="0:1:0"]');
  select(wordPoint(root, chapter, "0:1:0", 2), wordPoint(root, chapter, "0:1:0", 4, true));
  await sentence.trigger("click");
  expect(wrapper!.emitted("play")).toBeUndefined();
  document.getSelection()!.collapseToEnd();
  await sentence.trigger("click");
  expect(wrapper!.emitted("play")).toEqual([["0:1:0"]]);
});

it("does not play a sentence on a click of another button than the main one (WebKit clicks on a right click)", async () => {
  const { chapter } = shapes;
  draw(chapter);
  const sentence = wrapper!.get('[data-segment-id="0:1:0"]');
  await sentence.trigger("click", { button: 2 });
  expect(wrapper!.emitted("play")).toBeUndefined();
  await sentence.trigger("click", { button: 0 });
  expect(wrapper!.emitted("play")).toEqual([["0:1:0"]]);
});

it("draws code with its lines and indentation, and a selection across them is its words", () => {
  const { text, chapter } = shapes;
  const root = draw(chapter);
  const code = root.querySelector("pre")!;
  // As written on the page (the counted spaces end each line).
  expect(code.textContent).toContain('fn main() { \n    println!("tide"); \n\n        let  x = 1; \n}');
  const id = chapter.blocks.find((b) => b.kind === "preformatted")!.segments[0].id;
  const words = segment(chapter, id).words;
  const from = words.indexOf("main()"),
    to = words.indexOf("let");
  const range = rangeOfSelection(
    select(wordPoint(root, chapter, id, from), wordPoint(root, chapter, id, to, true)),
    root,
    chapter,
  );
  expect(range?.text).toBe('main() { println!("tide"); let');
  expect(slice(text, range!.start, range!.end)).toBe('main() {\n    println!("tide");\n\n        let');
});
