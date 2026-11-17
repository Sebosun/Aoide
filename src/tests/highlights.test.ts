import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import { initialPlayer } from "@/api/initial";
import ReaderView from "@/views/ReaderView.vue";
import ChapterSegment from "@/components/reader/ChapterSegment.vue";
import { useReaderStore } from "@/stores/reader";
import { usePlayerStore } from "@/stores/player";
import { useLibraryStore } from "@/stores/library";
import { usePreferencesStore } from "@/stores/preferences";
import { sentenceMarks, pieces } from "@/components/reader/highlights";
import { useTextActions } from "@/components/reader/textActions";
import { usePlayerControls } from "@/composables/usePlayerControls";
import { useUiStore } from "@/stores/ui";
import ToastStack from "@/components/ui/ToastStack.vue";
import type { ChapterView, HighlightView, SegmentView } from "@/api/types";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("@/api", () => ({
  get api() {
    return backend.api;
  },
}));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => {
  setActivePinia(createPinia());
  mock = createMockApi();
  backend.api = mock;
});
afterEach(() => {
  mock.dispose();
});

async function openReader(book = "book-1") {
  await useLibraryStore().start();
  await useReaderStore().open(book);
  await usePlayerStore().start();
  const wrapper = mount(ReaderView);
  await flushPromises();
  return wrapper;
}
const sentence = (id: string) =>
  useReaderStore()
    .chapter!.blocks.flatMap((b) => b.segments)
    .find((s) => s.id === id)!;
/** Where word `from` starts and word `to` ends in the chapter, as the core counts. */
const span = (segment: SegmentView, from: number, to: number) => ({
  start: segment.starts[from],
  end: segment.starts[to] + Array.from(segment.words[to]).length,
});

it("marks a highlight in the sentences it covers, words and the spaces between them", () => {
  const segment: SegmentView = {
    id: "0:0:0",
    words: ["A", "café", "😀", "by", "night."],
    starts: [10, 12, 17, 19, 22],
    speakable: true,
    canSayAgain: true,
    note: false,
    notes: [],
  };
  const chapter = {
    index: 0,
    title: "",
    notNarrated: null,
    blocks: [{ kind: "paragraph", level: null, segments: [segment] }],
  } as ChapterView;
  const highlight = (id: number, start: number, end: number): HighlightView => ({
    id,
    chapter: 0,
    chapterTitle: "",
    start,
    end,
    text: "",
    note: "",
    detached: false,
  });
  // "café 😀 by" (the emoji is two UTF-16 units on the page, one character to the core).
  const marks = sentenceMarks(chapter, [highlight(7, 12, 21)]).get("0:0:0")!;
  const drawn = segment.words.join(" ");
  expect(marks.map((m) => drawn.slice(m.start, m.end))).toEqual(["café 😀 by"]);
  expect(pieces(drawn, 0, marks)).toEqual([
    { text: "A ", id: null },
    { text: "café 😀 by", id: 7 },
    { text: " night.", id: null },
  ]);
  // Part of a word; a detached one is not drawn; nothing for a sentence with none.
  const part = sentenceMarks(chapter, [highlight(8, 23, 26), { ...highlight(9, 10, 11), detached: true }]);
  expect(part.get("0:0:0")!.map((m) => drawn.slice(m.start, m.end))).toEqual(["igh"]);
  expect(sentenceMarks(chapter, []).get("0:0:0")).toBe(sentenceMarks(chapter, [highlight(1, 0, 5)]).get("0:0:0"));
});

it("draws a new highlight in the sentence it covers and no other sentence again", async () => {
  const wrapper = await openReader();
  const reader = useReaderStore();
  const segments = wrapper.findAllComponents(ChapterSegment);
  const draws = segments.map((s) => vi.spyOn(s.vm.$ as unknown as { render: () => unknown }, "render"));
  const target = reader.chapter!.blocks.flatMap((b) => b.segments).find((s) => s.speakable && s.words.length > 3)!;
  const { start, end } = span(target, 1, 2);
  await reader.addHighlight(reader.currentChapter, start, end);
  await flushPromises();
  const mark = wrapper.get(`[data-segment-id="${target.id}"] mark`);
  expect(mark.text()).toBe(target.words.slice(1, 3).join(" "));
  const drawn = segments.flatMap((s, i) => (draws[i].mock.calls.length ? [s.props("segment").id] : []));
  expect(drawn).toEqual([target.id]);
  wrapper.unmount();
});

it("shows a highlight made on the page in Lines, in the spoken sentence and those around it", async () => {
  const page = await openReader("book-0");
  const reader = useReaderStore();
  const spoken = sentence("0:1:0"),
    next = sentence("0:2:0");
  await reader.addHighlight(0, span(spoken, 0, 1).start, span(spoken, 0, 1).end);
  await reader.addHighlight(0, span(next, 0, 1).start, span(next, 0, 1).end);
  page.unmount();
  mock.emitConfig({ appearance: { readerMode: "lines" } });
  await usePreferencesStore().start();
  const wrapper = mount(ReaderView);
  mock.emitPlayer({ ...initialPlayer(), bookId: "book-0", status: "playing", segmentId: "0:1:0", wordIndex: 2 });
  await flushPromises();
  const now = wrapper.get(".reader-lines__line--now");
  expect(
    now
      .findAll("mark")
      .map((m) => m.element.textContent)
      .join(""),
  ).toBe("Every evening,");
  const after = wrapper.get('.reader-lines__line[data-segment-id="0:2:0"]');
  expect(after.get("mark").text()).toBe("Tonight a");
  wrapper.unmount();
});

it("highlights selected words from the text menu, joined with a highlight they touch", async () => {
  const wrapper = await openReader();
  const reader = useReaderStore();
  const target = reader.chapter!.blocks.flatMap((b) => b.segments).find((s) => s.speakable && s.words.length > 3)!;
  const highlight = useTextActions().find((a) => a.id === "highlight")!;
  expect(highlight.applies).toEqual(["selection"]);
  const { start, end } = span(target, 0, 1);
  await highlight.run({
    kind: "selection",
    range: { chapter: 0, start, end, text: target.words.slice(0, 2).join(" "), firstSegmentId: target.id },
  });
  expect(reader.highlights.map((h) => h.text)).toEqual([target.words.slice(0, 2).join(" ")]);
  expect(useUiStore().toasts.map((t) => t.text)).toContain("Highlighted");
  // The rest of the sentence from the second word, joined with the words it overlaps.
  const rest = span(target, 1, target.words.length - 1);
  await highlight.run({
    kind: "selection",
    range: { chapter: 0, ...rest, text: target.words.slice(1).join(" "), firstSegmentId: target.id },
  });
  expect(reader.highlights.map((h) => h.text)).toEqual([target.words.join(" ")]);
  wrapper.unmount();
});

it("highlights the sentence just spoken, and Undo in its toast takes it away", async () => {
  const wrapper = await openReader("book-0");
  const toasts = mount(ToastStack);
  const reader = useReaderStore();
  await mock.play({ bookId: "book-0", chapter: 0, segmentId: "0:1:0" });
  await mock.pause();
  await flushPromises();
  await usePlayerControls().highlightSpoken();
  await flushPromises();
  expect(reader.highlights.map((h) => h.text)).toEqual([sentence("0:1:0").words.join(" ")]);
  const toast = toasts.get(".toasts__item");
  expect(toast.text()).toContain("Highlighted");
  await toast.get("button.toasts__action").trigger("click");
  await flushPromises();
  expect(reader.highlights).toEqual([]);
  expect(toasts.find(".toasts__item").exists()).toBe(false);
  toasts.unmount();
  wrapper.unmount();
});

it("opens a card on a highlight that is clicked, to write its note, copy it or remove it, without playing", async () => {
  const play = vi.spyOn(mock, "play");
  const writeText = vi.fn(async () => {});
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
  const wrapper = mount(ReaderView, { attachTo: document.body });
  await useLibraryStore().start();
  await useReaderStore().open("book-0");
  await usePlayerStore().start();
  await flushPromises();
  const reader = useReaderStore();
  const target = sentence("0:1:0");
  await reader.addHighlight(0, span(target, 0, 1).start, span(target, 0, 1).end);
  await flushPromises();
  (document.querySelector('[data-segment-id="0:1:0"] mark') as HTMLElement).click();
  await flushPromises();
  expect(play).not.toHaveBeenCalled();
  const card = () => document.querySelector<HTMLElement>('[role="dialog"][aria-label="Highlight"]');
  expect(card()?.textContent).toContain("Every evening,");
  const note = card()!.querySelector("textarea")!;
  note.value = "Where it starts";
  note.dispatchEvent(new Event("change"));
  await flushPromises();
  expect(reader.highlights[0].note).toBe("Where it starts");
  [...card()!.querySelectorAll("button")].find((b) => b.textContent?.includes("Copy"))!.click();
  await flushPromises();
  expect(writeText).toHaveBeenCalledWith("Every evening,");
  [...card()!.querySelectorAll("button")].find((b) => b.textContent?.includes("Remove"))!.click();
  await flushPromises();
  expect(reader.highlights).toEqual([]);
  expect(card()).toBeNull();
  wrapper.unmount();
});
