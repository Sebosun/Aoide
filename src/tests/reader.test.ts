import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi, initialPlayer } from "../api/mock";
import ReaderView from "../views/ReaderView.vue";
import { useReaderStore } from "../stores/reader";
import { usePlayerStore } from "../stores/player";
import { useLibraryStore } from "../stores/library";
import { usePreferencesStore } from "../stores/preferences";

import { choose, pickMenuItem } from "./helpers";
import { useUiStore } from "../stores/ui";
const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("../api", () => ({ get api() { return backend.api; } }));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => { setActivePinia(createPinia()); mock = createMockApi(); backend.api = mock; });
afterEach(() => { mock.dispose(); });

async function openReader(options: { attach?: boolean } = {}) {
  await useLibraryStore().start();
  await useReaderStore().open("book-0");
  await usePlayerStore().start();
  const wrapper = mount(ReaderView, options.attach ? { attachTo: document.body } : {});
  await flushPromises();
  return wrapper;
}
const playing = (extra: Partial<ReturnType<typeof initialPlayer>> = {}) => ({ ...initialPlayer(), bookId: "book-0", status: "playing" as const, segmentId: "0:1:0", wordIndex: 0, ...extra });

it("sets the chapter as a page with the book in the margin", async () => {
  const wrapper = await openReader();
  const text = wrapper.get('[aria-label="Chapter text"]');
  expect(text.get("h2").text()).toBe("The small light");
  const margin = wrapper.get('[aria-label="About this book"]');
  expect(margin.text()).toContain("The Lantern Keeper");
  expect(margin.text()).toContain("Mira Vale");
  expect(margin.text()).toContain("Chapter 1 of 2");
  expect(margin.text()).toMatch(/min left in chapter/);
  wrapper.unmount();
});

it("plays the clicked sentence", async () => {
  const play = vi.spyOn(mock, "play");
  const wrapper = await openReader();
  await wrapper.get('[data-segment-id="0:1:0"]').trigger("click");
  expect(play).toHaveBeenCalledWith("book-0", 0, "0:1:0");
  wrapper.unmount();
});

it("washes the spoken sentence and underlines the word, only for the open book", async () => {
  const wrapper = await openReader();
  mock.emitPlayer(playing({ wordIndex: 2 }));
  await flushPromises();
  expect(wrapper.get(".chapter-text__segment--current").attributes("data-segment-id")).toBe("0:1:0");
  expect(wrapper.get(".chapter-text__word--current").text()).toBe("Ada");
  mock.emitPlayer(playing({ bookId: "book-1", wordIndex: 2 }));
  await flushPromises();
  expect(wrapper.find(".chapter-text__segment--current").exists()).toBe(false);
  wrapper.unmount();
});

it("follows the playhead, stops when the reader scrolls away, and comes back on request", async () => {
  const scroll = vi.fn();
  HTMLElement.prototype.scrollTo = scroll as unknown as HTMLElement["scrollTo"];
  const wrapper = await openReader({ attach: true });
  mock.emitPlayer(playing());
  await flushPromises();
  expect(scroll).toHaveBeenCalled();
  scroll.mockClear();

  await wrapper.get('[aria-label="Chapter text"]').trigger("wheel", { deltaY: 100 });
  await flushPromises();
  expect(wrapper.find('[aria-label="Back to playhead"]').exists()).toBe(true);
  mock.emitPlayer(playing({ segmentId: "0:2:0" }));
  await flushPromises();
  expect(scroll).not.toHaveBeenCalled();

  await wrapper.get('[aria-label="Back to playhead"]').trigger("click");
  expect(scroll).toHaveBeenCalled();
  expect(wrapper.find('[aria-label="Back to playhead"]').exists()).toBe(false);
  wrapper.unmount();
});

it("shows the playhead chapter and jumps to a chapter from the margin", async () => {
  const wrapper = await openReader();
  const play = vi.spyOn(mock, "play");
  mock.emitPlayer(playing({ chapter: 1, segmentId: "1:1:0", wordIndex: 1 }));
  await flushPromises();
  expect(wrapper.get('[aria-label="Chapter text"] h2').text()).toBe("Across the water");
  expect(wrapper.get(".chapter-text__word--current").text()).toBe("dawn");

  await wrapper.get('[aria-label="About this book"] [aria-haspopup="menu"]').trigger("click");
  await pickMenuItem("Chapter 1: The small light");
  await flushPromises();
  expect(play).toHaveBeenCalledWith("book-0", 0, "0:0:0");
  wrapper.unmount();
});

it("shows a footnote from its coral marker without starting playback", async () => {
  const play = vi.spyOn(mock, "play");
  const wrapper = await openReader();
  const marker = wrapper.get(".chapter-text__note");
  expect(marker.text()).toBe("1");
  expect(marker.element.previousElementSibling?.textContent).toBe("Ada");
  await marker.trigger("click");
  expect(wrapper.get('[role="note"]').text()).toContain("original note");
  expect(play).not.toHaveBeenCalled();
  wrapper.unmount();
});

it("lists footnotes in the margin beside their paragraph on wide windows", async () => {
  const matchMedia = window.matchMedia;
  window.matchMedia = ((query: string) => ({ matches: query.includes("min-width"), media: query, addEventListener() {}, removeEventListener() {} })) as unknown as typeof window.matchMedia;
  const wrapper = await openReader();
  const note = wrapper.get(".chapter-text__margin-note");
  expect(note.text()).toContain("Note 1");
  expect(note.text()).toContain("original note");
  expect(note.element.closest(".chapter-text__row")?.contains(wrapper.get('[data-segment-id="0:1:0"]').element)).toBe(true);
  window.matchMedia = matchMedia;
  wrapper.unmount();
});

it("docks the transport: progress, time left, buffer status and errors", async () => {
  const wrapper = await openReader();
  mock.emitPlayer(playing({ status: "buffering", chapterFraction: 0.4 }));
  mock.emitGeneration({ bookId: "book-0", generatingSegmentId: "0:2:0", bufferedAheadMs: 12500, pending: 3, error: "Generation paused", preparing: null });
  await flushPromises();
  const dock = wrapper.get('[aria-label="Player"]');
  expect(dock.get('[role="slider"]').attributes("aria-valuenow")).toBe("40");
  expect(dock.text()).toContain("40%");
  expect(dock.text()).toMatch(/min left/);
  expect(dock.text()).toContain("Buffering…");
  expect(dock.text()).toContain("12.5 s buffered · 3 pending");
  expect(dock.get('[role="alert"]').text()).toContain("Generation paused");
  wrapper.unmount();
});

it("plays from the saved position, pauses, resumes, changes speed, skips chapters and stops", async () => {
  const wrapper = await openReader();
  const play = vi.spyOn(mock, "play"), skip = vi.spyOn(mock, "skipChapter");
  const dock = () => wrapper.get('[aria-label="Player"]');
  await dock().get('[aria-label="Play"]').trigger("click");
  await flushPromises();
  expect(play).toHaveBeenCalledWith("book-0", null, null);
  await dock().get('[aria-label="Pause"]').trigger("click");
  await flushPromises();
  expect(usePlayerStore().state.status).toBe("paused");
  await dock().get('[aria-label="Play"]').trigger("click");
  await flushPromises();
  expect(usePlayerStore().state.status).toBe("playing");
  await choose(dock(), "Playback speed", "1.25×");
  await flushPromises();
  expect(usePlayerStore().state.speed).toBe(1.25);
  expect(dock().text()).toContain("1.25×");
  await dock().get('[aria-label="Next chapter"]').trigger("click");
  await dock().get('[aria-label="Previous chapter"]').trigger("click");
  expect(skip.mock.calls).toEqual([[1], [-1]]);
  await dock().get('[aria-label="Stop"]').trigger("click");
  await flushPromises();
  expect(usePlayerStore().state.status).toBe("stopped");
  wrapper.unmount();
});

it("sets a sleep timer and bookmarks the playhead from the dock", async () => {
  const sleep = vi.spyOn(mock, "setSleepTimer"), mark = vi.spyOn(mock, "addBookmark");
  const wrapper = await openReader();
  await mock.play("book-0", 0, "0:1:0");
  await flushPromises();
  const dock = wrapper.get('[aria-label="Player"]');
  await choose(dock, "Sleep timer", "15 min");
  await flushPromises();
  expect(sleep).toHaveBeenLastCalledWith(15, false);
  expect(dock.text()).toMatch(/1[45]:\d\d/);
  await choose(dock, "Sleep timer", "End of chapter");
  expect(sleep).toHaveBeenLastCalledWith(null, true);
  await dock.get('[aria-label="Add bookmark"]').trigger("click");
  await flushPromises();
  expect(mark).toHaveBeenCalledWith("book-0", "");
  expect(useUiStore().toasts.map(t => t.text)).toContain("Bookmarked");
  wrapper.unmount();
});

it("jumps to a sentence from the scrubber", async () => {
  const wrapper = await openReader();
  await mock.play("book-0", 0, "0:1:0");
  await flushPromises();
  const play = vi.spyOn(mock, "play");
  const slider = wrapper.get('[aria-label="Player"] [role="slider"]');
  await slider.trigger("keydown", { key: "ArrowRight" });
  expect(play).toHaveBeenLastCalledWith("book-0", 0, "0:2:0");
  await slider.trigger("keydown", { key: "Home" });
  expect(play).toHaveBeenLastCalledWith("book-0", 0, "0:0:0");
  wrapper.unmount();
});

it("changes the text size and the reader mode, and remembers both", async () => {
  const save = vi.spyOn(mock, "saveAppearance");
  const wrapper = await openReader();
  await usePreferencesStore().start();
  await wrapper.get('[aria-label="Text size"]').trigger("click");
  await wrapper.get('[aria-label="Larger text"]').trigger("click");
  await flushPromises();
  expect(save).toHaveBeenLastCalledWith(expect.objectContaining({ textSize: 3 }));
  await wrapper.get('[aria-label="Lines"]').trigger("click");
  await flushPromises();
  expect(save).toHaveBeenLastCalledWith(expect.objectContaining({ readerMode: "lines" }));
  wrapper.unmount();
});
