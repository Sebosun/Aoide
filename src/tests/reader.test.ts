import { beforeEach, afterEach, expect, it, onTestFinished, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import { initialPlayer } from "@/api/initial";
import ReaderView from "@/views/ReaderView.vue";
import ReaderTools from "@/components/reader/ReaderTools.vue";
import ListenButton from "@/components/reader/ListenButton.vue";
import ChapterMenu from "@/components/reader/ChapterMenu.vue";
import ChapterText from "@/components/reader/ChapterText.vue";
import { useReaderStore } from "@/stores/reader";
import { usePlayerStore } from "@/stores/player";
import { useLibraryStore } from "@/stores/library";
import { usePreferencesStore } from "@/stores/preferences";

import { choose, openMenu, pickMenuItem } from "@/tests/helpers";
import { useUiStore } from "@/stores/ui";
import { useNavigationStore } from "@/stores/navigation";
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

async function openReader(options: { attach?: boolean; book?: string } = {}) {
  await useLibraryStore().start();
  await useReaderStore().open(options.book ?? "book-0");
  await usePlayerStore().start();
  const wrapper = mount(ReaderView, options.attach ? { attachTo: document.body } : {});
  await flushPromises();
  return wrapper;
}
const playing = (extra: Partial<ReturnType<typeof initialPlayer>> = {}) => ({
  ...initialPlayer(),
  bookId: "book-0",
  status: "playing" as const,
  segmentId: "0:1:0",
  wordIndex: 0,
  ...extra,
});

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

it("starts the next or previous chapter at its top, following again", async () => {
  const scroll = vi.fn();
  HTMLElement.prototype.scrollTo = scroll as unknown as HTMLElement["scrollTo"];
  const wrapper = await openReader({ attach: true });
  mock.emitPlayer(playing());
  await flushPromises();
  await wrapper.get('[aria-label="Chapter text"]').trigger("wheel", { deltaY: 100 });
  await flushPromises();
  expect(wrapper.find('[aria-label="Back to playhead"]').exists()).toBe(true);
  scroll.mockClear();

  await wrapper.get('[aria-label="Player"] [aria-label="Next chapter"]').trigger("click");
  await flushPromises();
  expect(wrapper.get('[aria-label="Chapter text"] h2').text()).toBe("Across the water");
  expect(scroll).toHaveBeenCalledWith({ top: 0 });
  expect(wrapper.find('[aria-label="Back to playhead"]').exists()).toBe(false);

  scroll.mockClear();
  await wrapper.get('[aria-label="Player"] [aria-label="Previous chapter"]').trigger("click");
  await flushPromises();
  expect(scroll).toHaveBeenCalledWith({ top: 0 });
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
  window.matchMedia = ((query: string) => ({
    matches: query.includes("min-width"),
    media: query,
    addEventListener() {},
    removeEventListener() {},
  })) as unknown as typeof window.matchMedia;
  const wrapper = await openReader();
  const note = wrapper.get(".chapter-text__margin-note");
  expect(note.text()).toContain("Note 1");
  expect(note.text()).toContain("original note");
  expect(note.element.closest(".chapter-text__row")?.contains(wrapper.get('[data-segment-id="0:1:0"]').element)).toBe(
    true,
  );
  window.matchMedia = matchMedia;
  wrapper.unmount();
});

it("docks the transport: progress, time left, buffer status and errors", async () => {
  const wrapper = await openReader();
  mock.emitPlayer(playing({ status: "buffering", chapterFraction: 0.4 }));
  mock.emitGeneration({
    bookId: "book-0",
    generatingSegmentId: "0:2:0",
    bufferedAheadMs: 12500,
    pending: 3,
    error: "Generation paused",
    preparing: null,
  });
  await flushPromises();
  const dock = wrapper.get('[aria-label="Player"]');
  expect(dock.get('[role="slider"]').attributes("aria-valuenow")).toBe("40");
  expect(dock.text()).toContain("40%");
  expect(dock.text()).toMatch(/min left/);
  expect(dock.text()).toContain("Buffering…");
  expect(dock.text(), "buffer numbers stay off the bar").not.toContain("buffered");
  expect(dock.get('[role="slider"]').attributes("title")).toBe("12.5 s buffered · 3 pending");
  expect(dock.get('[role="alert"]').text()).toContain("Generation paused");
  wrapper.unmount();
});

it("plays from the saved position, pauses, resumes, changes speed, skips chapters and stops", async () => {
  const wrapper = await openReader();
  const play = vi.spyOn(mock, "play"),
    skip = vi.spyOn(mock, "skipChapter");
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
  const sleep = vi.spyOn(mock, "setSleepTimer"),
    mark = vi.spyOn(mock, "addBookmark");
  const wrapper = await openReader();
  await mock.play("book-0", 0, "0:1:0");
  await flushPromises();
  const dock = wrapper.get('[aria-label="Player"]');
  expect(dock.get('[aria-label="Sleep timer"]').text(), "just the moon while off").toBe("");
  await choose(dock, "Sleep timer", "15 min");
  await flushPromises();
  expect(sleep).toHaveBeenLastCalledWith(15, false);
  expect(dock.text()).toMatch(/1[45]:\d\d/);
  await choose(dock, "Sleep timer", "End of chapter");
  expect(sleep).toHaveBeenLastCalledWith(null, true);
  await dock.get('[aria-label="Add bookmark"]').trigger("click");
  await flushPromises();
  expect(mark).toHaveBeenCalledWith("book-0", "");
  expect(useUiStore().toasts.map((t) => t.text)).toContain("Bookmarked");
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

it("sets the volume from the dock, and mutes and unmutes to the same level", async () => {
  const wrapper = await openReader();
  const dock = () => wrapper.get('[aria-label="Player"]');
  const volume = () => document.body.querySelector<HTMLInputElement>('input[aria-label="Volume"]')!;
  const press = async (label: string) => {
    document.body.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!.click();
    await flushPromises();
  };
  expect(volume(), "the slider opens from the speaker button").toBeNull();
  await dock().get('button[aria-label="Volume"]').trigger("click");
  expect(volume().value).toBe("100");
  volume().value = "40";
  volume().dispatchEvent(new Event("input"));
  await flushPromises();
  expect(usePlayerStore().state.volume).toBe(0.4);
  await press("Mute");
  expect(usePlayerStore().state.volume).toBe(0);
  expect(volume().value).toBe("0");
  await press("Unmute");
  expect(usePlayerStore().state.volume).toBe(0.4);
  wrapper.unmount();
});

it("reads a silent book as a page with its progress underneath, and no player", async () => {
  const play = vi.spyOn(mock, "play");
  const wrapper = await openReader({ book: "book-1" });
  expect(wrapper.find('[aria-label="Player"]').exists()).toBe(false);
  const progress = wrapper.get('[role="progressbar"][aria-label="Reading progress"]');
  expect(progress.attributes("aria-valuenow")).toBe("0");
  expect(wrapper.get(".sheet__foot").text()).toMatch(/0%.*left to read/);
  expect(wrapper.get('[aria-label="About this book"]').text()).toMatch(/min to read/);
  expect(wrapper.find('[aria-label="Reader mode"]').exists()).toBe(false);
  const listen = wrapper.get("button.listen-button");
  expect([listen.text(), listen.attributes("aria-pressed")]).toEqual(["Listen", "false"]);
  // Sentences are only read: clicking one plays nothing.
  const sentence = wrapper.get('[data-segment-id="0:1:0"]');
  expect(sentence.attributes("role")).toBeUndefined();
  await sentence.trigger("click");
  expect(play).not.toHaveBeenCalled();
  wrapper.unmount();
});

it("listens from where the reader is and reads on silently when Listening is pressed again", async () => {
  const setMode = vi.spyOn(mock, "setMode"),
    play = vi.spyOn(mock, "play");
  const wrapper = await openReader({ book: "book-1" });
  useReaderStore().place = { chapter: 1, segmentId: "1:1:0" };
  await wrapper.get("button.listen-button").trigger("click");
  await flushPromises();
  expect(setMode).toHaveBeenCalledWith("book-1", "listen");
  expect(play).toHaveBeenCalledWith("book-1", 1, "1:1:0");
  const listening = wrapper.get("button.listen-button");
  expect([listening.text(), listening.attributes("aria-pressed")]).toEqual(["Listening", "true"]);
  expect(wrapper.find('[aria-label="Player"]').exists()).toBe(true);
  expect(wrapper.find('[aria-label="Reader mode"]').exists()).toBe(true);

  await listening.trigger("click");
  await flushPromises();
  expect(setMode).toHaveBeenLastCalledWith("book-1", "read");
  expect(usePlayerStore().state.status).toBe("paused");
  expect(wrapper.find('[aria-label="Player"]').exists()).toBe(false);
  wrapper.unmount();
});

it("shows Lines only while listening, and a silent book in Lines mode as a page", async () => {
  mock.emitConfig({ appearance: { readerMode: "lines" } });
  await usePreferencesStore().start();
  const wrapper = await openReader({ book: "book-1" });
  expect(wrapper.find(".sheet").exists()).toBe(true);
  expect(wrapper.find('[aria-label="Spoken sentence"]').exists()).toBe(false);
  wrapper.unmount();
  const along = await openReader();
  expect(along.find('[aria-label="Reader mode"]').exists()).toBe(true);
  along.unmount();
});

it("offers ways to give the book a voice when nothing could narrate it", async () => {
  const play = vi.spyOn(mock, "play");
  const wrapper = await openReader({ book: "book-1", attach: true });
  mock.emitEngine({ profileName: "Local Chatterbox", state: "offline", detail: null });
  await flushPromises();
  const listen = wrapper.get("button.listen-button");
  expect(listen.attributes("aria-haspopup")).toBe("menu");
  await listen.trigger("click");
  await flushPromises();
  const items = [...document.body.querySelectorAll('[role="menu"] [role^="menuitem"]')].map((i) =>
    i.textContent?.trim(),
  );
  expect(items).toEqual(["Narrate with a voice", "Pair an audiobook…", "Speech engine settings"]);
  expect(play).not.toHaveBeenCalled();
  await pickMenuItem("Narrate with a voice");
  expect(play).toHaveBeenCalledWith("book-1", null, null);
  wrapper.unmount();
});

/** Lays the open chapter's sentences out `step` px apart, the first at `first`, in a reader
 *  800 px tall (jsdom has no layout). */
function layOut(wrapper: ReturnType<typeof mount>, first: number, step = 100) {
  const box = wrapper.get(".sheet__scroll").element as HTMLElement;
  box.getBoundingClientRect = () => ({ top: 0, bottom: 800, height: 800 }) as DOMRect;
  const spoken = [...box.querySelectorAll<HTMLElement>(".chapter-text__segment:not(.chapter-text__segment--quiet)")];
  spoken.forEach((segment, index) => {
    segment.getBoundingClientRect = () => ({ top: first + index * step }) as DOMRect;
  });
  return { box, ids: spoken.map((s) => s.dataset.segmentId!) };
}

it("tells the core where the reader is, at most once a second, and listens from there", async () => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] });
  try {
    const place = vi.spyOn(mock, "setReadingPlace"),
      play = vi.spyOn(mock, "play");
    const wrapper = await openReader({ book: "book-1" });
    let { box, ids } = layOut(wrapper, -50);
    box.dispatchEvent(new Event("scroll"));
    await vi.advanceTimersByTimeAsync(0);
    expect(place).toHaveBeenCalledTimes(1);
    expect(place).toHaveBeenLastCalledWith("book-1", 0, ids[1]);
    // The same sentence at the top is not reported again.
    box.dispatchEvent(new Event("scroll"));
    await vi.advanceTimersByTimeAsync(1500);
    expect(place).toHaveBeenCalledTimes(1);

    ({ box, ids } = layOut(wrapper, 50));
    box.dispatchEvent(new Event("scroll"));
    await vi.advanceTimersByTimeAsync(0);
    expect(place).toHaveBeenCalledTimes(2);
    ({ box, ids } = layOut(wrapper, -150));
    box.dispatchEvent(new Event("scroll"));
    await vi.advanceTimersByTimeAsync(500);
    expect(place).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(600);
    expect(place).toHaveBeenLastCalledWith("book-1", 0, ids[2]);
    await flushPromises();
    // The core's progress shows in the library at once.
    expect(useLibraryStore().books.find((b) => b.id === "book-1")?.progress?.segmentId).toBe(ids[2]);

    await wrapper.get("button.listen-button").trigger("click");
    await flushPromises();
    expect(play).toHaveBeenCalledWith("book-1", 0, ids[2]);
    wrapper.unmount();
  } finally {
    vi.useRealTimers();
  }
});

it("opens a book read silently at its saved place, and picking a chapter only shows it", async () => {
  const play = vi.spyOn(mock, "play");
  await mock.setReadingPlace("book-1", 1, "1:1:0");
  const scrolled: string[] = [];
  const original = Element.prototype.scrollIntoView;
  Element.prototype.scrollIntoView = function (this: HTMLElement) {
    scrolled.push(this.dataset.segmentId ?? "");
  };
  onTestFinished(() => {
    Element.prototype.scrollIntoView = original;
  });
  const wrapper = await openReader({ book: "book-1" });
  expect(useReaderStore().currentChapter).toBe(1);
  expect(scrolled).toEqual(["1:1:0"]);
  // The scroll that shows the saved place is not the reader's: nothing is reported for it.
  const reported = vi.spyOn(mock, "setReadingPlace");
  layOut(wrapper, 0);
  wrapper.get(".sheet__scroll").element.dispatchEvent(new Event("scroll"));
  await new Promise((resolve) => setTimeout(resolve, 50));
  expect(reported).not.toHaveBeenCalled();

  await wrapper.get("button.sheet__next").trigger("click");
  await flushPromises();
  expect(useReaderStore().currentChapter).toBe(2);
  expect(wrapper.find("button.sheet__next").exists()).toBe(false);
  expect(play).not.toHaveBeenCalled();
  wrapper.unmount();
});

it("counts reading time only while a silent book is read in a focused window", async () => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"] });
  const focused = vi.spyOn(document, "hasFocus").mockReturnValue(true);
  try {
    const activity = vi.spyOn(mock, "readingActivity");
    const wrapper = await openReader({ book: "book-1" });
    await vi.advanceTimersByTimeAsync(30_000);
    expect(activity).not.toHaveBeenCalled(); // No input yet: nobody may be there.

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown" }));
    expect(activity).toHaveBeenCalledTimes(1);
    expect(activity).toHaveBeenLastCalledWith("book-1");
    window.dispatchEvent(new Event("scroll"));
    expect(activity).toHaveBeenCalledTimes(1); // At most every 15 s.
    await vi.advanceTimersByTimeAsync(15_000);
    expect(activity).toHaveBeenCalledTimes(2);
    // A minute without input: the reader stepped away.
    await vi.advanceTimersByTimeAsync(60_000);
    const away = activity.mock.calls.length;
    await vi.advanceTimersByTimeAsync(60_000);
    expect(activity).toHaveBeenCalledTimes(away);

    // Another window has focus.
    focused.mockReturnValue(false);
    window.dispatchEvent(new PointerEvent("pointerdown"));
    await vi.advanceTimersByTimeAsync(30_000);
    expect(activity).toHaveBeenCalledTimes(away);

    // Listening counts as listening only.
    focused.mockReturnValue(true);
    await useReaderStore().setMode("listen");
    await flushPromises();
    window.dispatchEvent(new PointerEvent("pointerdown"));
    await vi.advanceTimersByTimeAsync(30_000);
    expect(activity).toHaveBeenCalledTimes(away);
    wrapper.unmount();
  } finally {
    vi.useRealTimers();
  }
});

/** The preview's audiobook on its own ("Winter Radio": Part One 1.02 s, Part Two 0.525 s). */
async function openAudiobook() {
  await mock.addAudiobook(["~/Audiobooks/Winter Radio"]);
  return openReader({ book: "book-5", attach: true });
}

it("shows an audiobook as a player with its parts, and nothing that needs text", async () => {
  const wrapper = await openAudiobook();
  const deck = wrapper.get(".audio-reader");
  expect(deck.get("h1").text()).toBe("Winter Radio");
  expect(deck.text()).toContain("Ines Morrow");
  expect(deck.text()).toContain("Part 1 · Part One");
  expect(deck.text()).toContain("0% of the book");
  const rows = deck.findAll(".audio-reader__part-row");
  expect(rows.map((r) => r.text())).toEqual(["1Part One0:01", "2Part Two0:00"]);
  expect(rows[0].attributes("aria-current")).toBe("true");
  // No Page/Lines switch, text size, Listen button or chapter menu.
  for (const absent of [ReaderTools, ListenButton, ChapterMenu, ChapterText])
    expect(wrapper.findComponent(absent).exists()).toBe(false);
  expect(wrapper.find('[aria-label="Text size"]').exists()).toBe(false);
  wrapper.unmount();
});

it("scrubs and skips an audiobook on its timeline, and jumps to a part", async () => {
  const seek = vi.spyOn(mock, "seek"),
    play = vi.spyOn(mock, "play");
  const wrapper = await openAudiobook();
  const slider = wrapper.get('[role="slider"][aria-label="Position in part"]');
  await slider.trigger("keydown", { key: "End" });
  await flushPromises();
  expect(seek).toHaveBeenLastCalledWith("book-5", 1020);
  expect(wrapper.text(), "the saved place moved into the next part").toContain("Part 2 · Part Two");
  await wrapper.get('[aria-label="Back 30 seconds"]').trigger("click");
  expect(seek).toHaveBeenLastCalledWith("book-5", 0);

  await wrapper.findAll(".audio-reader__part-row")[1].trigger("click");
  await flushPromises();
  expect(play).toHaveBeenLastCalledWith("book-5", 1, null);
  expect(wrapper.get(".audio-reader__part-row--current").text()).toContain("Part Two");
  expect(wrapper.find('.audio-reader__play[aria-label="Pause"]').exists()).toBe(true);
  await wrapper.get('[aria-label="Forward 30 seconds"]').trigger("click");
  expect(seek).toHaveBeenLastCalledWith("book-5", 1545);
  await mock.pause();
  wrapper.unmount();
});

it("bookmarks a moment of an audiobook and plays from it", async () => {
  const wrapper = await openAudiobook();
  await mock.play("book-5", 1, null);
  await mock.pause();
  await flushPromises();
  await wrapper.get('[aria-label="Bookmark this moment"]').trigger("click");
  await flushPromises();
  expect(useUiStore().toasts.map((t) => t.text)).toContain("Bookmarked");
  await wrapper.get('[aria-label="Search and more"]').trigger("click");
  await flushPromises();
  const sheet = wrapper.get('[role="dialog"]');
  expect(sheet.findAll('[role="tab"]').map((t) => t.text())).toEqual(["Bookmarks", "Details"]);
  const mark = sheet.get(".book-bookmarks__play");
  expect(mark.text()).toContain("Part 2 · 0:00");
  expect(mark.text()).toContain("Part Two");

  const seek = vi.spyOn(mock, "seek"),
    play = vi.spyOn(mock, "play");
  await mark.trigger("click");
  await flushPromises();
  expect(seek).toHaveBeenCalledWith("book-5", 1020);
  expect(play).toHaveBeenCalledWith("book-5", null, null);
  await mock.pause();
  wrapper.unmount();
});

/** The reader on the preview's article (book-6), saved in `stage`. */
async function openArticle(stage: string, more = "") {
  mock.dispose();
  setActivePinia(createPinia());
  mock = createMockApi(`?article=${stage}${more}`);
  backend.api = mock;
  return openReader({ book: "book-6", attach: true });
}

it("heads an article with its site, reading time and original, and outlines its headings", async () => {
  Element.prototype.scrollIntoView ??= () => {};
  const wrapper = await openArticle("ready");
  const open = vi.spyOn(mock, "openOriginal");
  const head = wrapper.get('[aria-label="About this article"]');
  expect(head.get("h1").text()).toBe("The Quay Lights");
  expect(head.text()).toContain("Quay Weekly");
  expect(head.text()).toContain("Ada Brightwater");
  expect(head.text()).toMatch(/\d+ min read/);
  await head
    .findAll("button")
    .find((b) => b.text() === "Open original")!
    .trigger("click");
  expect(open).toHaveBeenCalledWith("book-6");
  expect(wrapper.find('[aria-label="This article needs you"]').exists()).toBe(false);

  // An article has one chapter: the header offers its headings instead of chapters.
  expect(wrapper.find(".reader__chapters").exists()).toBe(false);
  const scrolled = vi.spyOn(Element.prototype, "scrollIntoView");
  await wrapper.get('[aria-label="Outline"]').trigger("click");
  await flushPromises();
  expect(openMenu()?.textContent).toContain("Why by hand");
  await pickMenuItem("Why by hand");
  expect(scrolled).toHaveBeenCalled();
  expect((scrolled.mock.contexts[0] as HTMLElement).textContent?.trim()).toBe("Why by hand");
  wrapper.unmount();
});

it("says what an article that needs you is missing, and fetches it again", async () => {
  for (const [stage, heading] of [
    ["needsyou-signin", "The site asked for a sign-in"],
    ["needsyou-cutshort", "The article looks cut short"],
  ]) {
    for (const [action, source] of [
      ["Fetch again", "http"],
      ["Try an archived copy", "archive"],
    ]) {
      const wrapper = await openArticle(stage);
      const refetch = vi.spyOn(mock, "refetchArticle");
      const notice = () => wrapper.get('[aria-label="This article needs you"]');
      expect(notice().get("h2").text(), stage).toBe(heading);
      expect(notice().text()).toContain("Kept so far: 93 words.");
      if (stage === "needsyou-cutshort") expect(notice().text()).toContain("about 1,200 words");
      const button = (text: string) =>
        notice()
          .findAll("button")
          .find((b) => b.text() === text)!;
      // The original is opened from the article's header.
      expect(button("Open original")).toBeUndefined();
      await button(action).trigger("click");
      await flushPromises();
      expect(refetch).toHaveBeenLastCalledWith("book-6", source);
      // The notice stays while the fetch runs, its actions waiting for it.
      expect(notice().text()).toContain("Fetching again…");
      expect(button("Fetch again").attributes("disabled")).toBeDefined();
      wrapper.unmount();
    }
  }
});

it("offers a sign-in window first once the browser is on, else turning it on", async () => {
  let wrapper = await openArticle("needsyou-signin");
  const notice = () => wrapper.get('[aria-label="This article needs you"]');
  const first = () => notice().findAll("button")[0];
  expect(first().text()).toBe("Turn on the browser…");
  await first().trigger("click");
  const navigation = useNavigationStore();
  expect([navigation.settingsOpen, navigation.settingsTab]).toEqual([true, "sites"]);
  wrapper.unmount();

  wrapper = await openArticle("needsyou-signin", "&browser=ready");
  const start = vi.spyOn(mock, "startSignIn");
  expect(first().text()).toBe("Sign in to Quay Weekly…");
  await first().trigger("click");
  await flushPromises();
  expect(start).toHaveBeenCalledWith("book-6");
  wrapper.unmount();
});
