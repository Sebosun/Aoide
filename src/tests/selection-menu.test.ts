/** The menu on selected text in the reader: what it offers, what each entry does, and when it
 *  goes away. */
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises, type VueWrapper } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import ReaderView from "@/views/ReaderView.vue";
import { useReaderStore } from "@/stores/reader";
import { usePlayerStore } from "@/stores/player";
import { useLibraryStore } from "@/stores/library";
import { usePreferencesStore } from "@/stores/preferences";
import { useNavigationStore } from "@/stores/navigation";
import { useUiStore } from "@/stores/ui";
import { pickMenuItem } from "@/tests/helpers";
import { defineComponent } from "vue";
import { useShortcuts } from "@/composables/useShortcuts";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("@/api", () => ({
  get api() {
    return backend.api;
  },
}));
let mock: ReturnType<typeof createMockApi>;
let wrapper: VueWrapper | null = null;
const clipboard = { writeText: vi.fn(async (_text: string) => {}) };
beforeEach(() => {
  setActivePinia(createPinia());
  mock = createMockApi();
  backend.api = mock;
  clipboard.writeText.mockClear();
  Object.defineProperty(navigator, "clipboard", { value: clipboard, configurable: true });
});
afterEach(() => {
  wrapper?.unmount();
  wrapper = null;
  mock.dispose();
  document.getSelection()?.removeAllRanges();
  document.body.innerHTML = "";
});

async function openReader(book = "book-0", options: { lines?: boolean } = {}) {
  if (options.lines) {
    mock.emitConfig({ appearance: { readerMode: "lines" } });
    await usePreferencesStore().start();
  }
  await useLibraryStore().start();
  await useReaderStore().open(book);
  await usePlayerStore().start();
  wrapper = mount(ReaderView, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}
/** The text node and offset where `words` begin in the page (searched across text nodes of
 *  sentence `id`, footnote markers left out). */
function find(id: string, words: string, end = false): [Node, number] {
  const sentence = document.querySelector(`[data-segment-id="${id}"]`)!;
  const walker = document.createTreeWalker(sentence, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode() as Text | null; node; node = walker.nextNode() as Text | null) {
    if (node.parentElement?.closest("[data-not-text]")) continue;
    const at = node.data.indexOf(words);
    if (at >= 0) return [node, at + (end ? words.length : 0)];
  }
  throw new Error(`"${words}" is not in ${id}`);
}
/** Selects from the start of `from` to the end of `to` as a drag of the pointer does. */
async function drag(from: [string, string], to: [string, string] = from) {
  const start = find(...from),
    end = find(to[0], to[1], true);
  (start[0].parentElement as HTMLElement).dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
  const range = document.createRange();
  range.setStart(...start);
  range.setEnd(...end);
  document.getSelection()!.removeAllRanges();
  document.getSelection()!.addRange(range);
  document.dispatchEvent(new Event("selectionchange"));
  (end[0].parentElement as HTMLElement).dispatchEvent(new PointerEvent("pointerup", { bubbles: true }));
  await settle(10);
}
const settle = async (ms: number) => {
  await new Promise((resolve) => setTimeout(resolve, ms));
  await flushPromises();
};
const menu = () => document.querySelector<HTMLElement>('[role="menu"][aria-label="Selected text"]');
const entries = () => [...menu()!.querySelectorAll('[role="menuitem"]')].map((item) => item.textContent?.trim());

it("offers Copy, Highlight, Search the book and Listen from here when words are selected", async () => {
  await openReader();
  expect(menu()).toBeNull();
  await drag(["0:1:0", "carried a lantern"], ["0:1:0", "end of the pier."]);
  expect(entries()).toEqual(["Copy", "Highlight", "Search the book", "Listen from here"]);
  await drag(["0:1:0", "lantern"]);
  expect(entries(), "a word or a few can be said differently").toEqual([
    "Copy",
    "Highlight",
    "Search the book",
    "Listen from here",
    "Say this differently…",
  ]);
});

it("copies the selected words without the footnote marker among them", async () => {
  await openReader();
  await drag(["0:1:0", "evening, Ada"], ["0:1:0", "carried"]);
  await pickMenuItem("Copy");
  expect(clipboard.writeText).toHaveBeenCalledWith("evening, Ada carried");
  expect(menu()).toBeNull();
  expect(useUiStore().toasts.map((t) => t.text)).toContain("Copied");
});

it("searches the book for the selected words", async () => {
  const search = vi.spyOn(mock, "searchBook");
  const view = await openReader();
  await drag(["0:1:1", "reflection"]);
  await pickMenuItem("Search the book");
  await flushPromises();
  expect(useNavigationStore().tools).toBe("search");
  expect((view.get('input[aria-label="Search in book"]').element as HTMLInputElement).value).toBe("reflection");
  expect(search).toHaveBeenCalledWith({ bookId: "book-0", query: "reflection" });
  expect(view.text()).toContain("reflection");
});

it("listens from the sentence the selection starts in, also while reading silently", async () => {
  const play = vi.spyOn(mock, "play");
  await openReader("book-1");
  expect(useReaderStore().shape).toBe("read");
  await drag(["0:1:0", "a bakery"], ["0:2:0", "Ren sat"]);
  await pickMenuItem("Listen from here");
  expect(play).toHaveBeenCalledWith({ bookId: "book-1", chapter: 0, segmentId: "0:1:0" });
  expect(useReaderStore().shape).toBe("readAlong");
});

it("says a selected word differently", async () => {
  const view = await openReader("book-1");
  await drag(["0:2:0", "Ren"]);
  await pickMenuItem("Say this differently…");
  const dialog = view.get('[role="dialog"][aria-label="Say this differently"]');
  expect((dialog.get('[aria-label="Written word"]').element as HTMLInputElement).value).toBe("Ren");
});

it("goes away on Escape without leaving the reader, on a click elsewhere, on scroll and when the selection goes", async () => {
  await openReader();
  const navigation = useNavigationStore();
  await drag(["0:1:0", "lantern"]);
  document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await flushPromises();
  expect(menu()).toBeNull();
  expect(navigation.view).toBe("reader");

  await drag(["0:1:0", "lantern"]);
  document.querySelector(".book-margin")!.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
  await flushPromises();
  expect(menu()).toBeNull();

  await drag(["0:1:0", "lantern"]);
  document.querySelector(".sheet__scroll")!.dispatchEvent(new Event("scroll"));
  await flushPromises();
  expect(menu()).toBeNull();

  await drag(["0:1:0", "lantern"]);
  document.getSelection()!.collapseToEnd();
  document.dispatchEvent(new Event("selectionchange"));
  await flushPromises();
  expect(menu()).toBeNull();
});

it("appears once a selection made with the keyboard settles", async () => {
  await openReader();
  const range = document.createRange();
  range.setStart(...find("0:1:0", "lantern"));
  range.setEnd(...find("0:1:0", "lantern", true));
  document.getSelection()!.addRange(range);
  document.dispatchEvent(new Event("selectionchange"));
  await settle(100);
  expect(menu(), "still selecting").toBeNull();
  await settle(300);
  expect(entries()).toContain("Copy");
});

it("logs which entry was chosen, never the text", async () => {
  const log = vi.spyOn(mock, "logUi");
  await openReader();
  await drag(["0:1:0", "lantern"]);
  await pickMenuItem("Copy");
  expect(log).toHaveBeenCalledWith({ entry: expect.objectContaining({ event: "ui.selection", command: "copy" }) });
  expect(JSON.stringify(log.mock.calls)).not.toContain("lantern");
});

it("offers the same menu in Lines", async () => {
  const play = vi.spyOn(mock, "play");
  await openReader("book-0", { lines: true });
  await drag(["0:1:1", "reflection"]);
  await pickMenuItem("Listen from here");
  expect(play).toHaveBeenCalledWith({ bookId: "book-0", chapter: 0, segmentId: "0:1:1" });
});

/** Right-clicks sentence `id` of the page. */
async function rightClick(id: string) {
  const sentence = document.querySelector(`[data-segment-id="${id}"]`)!;
  sentence.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 2 }));
  const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 300, clientY: 200 });
  sentence.dispatchEvent(event);
  sentence.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 2 }));
  await settle(5);
  return event;
}
const sentenceMenu = () => document.querySelector<HTMLElement>('[role="menu"][aria-label="Sentence"]');

it("right-clicking a sentence without selecting offers Listen from here, Say this again and Bookmark here", async () => {
  const play = vi.spyOn(mock, "play");
  await openReader();
  const event = await rightClick("0:1:1");
  expect(event.defaultPrevented, "the page's own menu does not open").toBe(true);
  expect([...sentenceMenu()!.querySelectorAll('[role="menuitem"]')].map((i) => i.textContent?.trim())).toEqual([
    "Listen from here",
    "Say this again",
    "Bookmark here",
  ]);
  expect(document.activeElement?.textContent?.trim(), "the menu takes the focus").toBe("Listen from here");
  await pickMenuItem("Listen from here");
  expect(play).toHaveBeenCalledWith({ bookId: "book-0", chapter: 0, segmentId: "0:1:1" });
});

it("says a right-clicked sentence again, and bookmarks it rather than the playhead", async () => {
  const again = vi.spyOn(mock, "sayAgain"),
    mark = vi.spyOn(mock, "addBookmark");
  await openReader();
  await mock.play({ bookId: "book-0", chapter: 0, segmentId: "0:1:0" });
  await flushPromises();
  await rightClick("0:2:0");
  await pickMenuItem("Say this again");
  expect(again).toHaveBeenCalledWith({ bookId: "book-0", segmentId: "0:2:0" });
  await rightClick("0:2:0");
  await pickMenuItem("Bookmark here");
  expect(mark).toHaveBeenCalledWith({ bookId: "book-0", note: "", segmentId: "0:2:0" });
  expect((await mock.bookmarks({ bookId: "book-0" })).at(-1)).toMatchObject({ chapter: 0, segmentId: "0:2:0" });
  expect(useUiStore().toasts.map((t) => t.text)).toContain("Bookmarked");
});

it("offers Say this again only for a sentence the speech engine says", async () => {
  await openReader("book-1");
  await rightClick("0:0:0");
  expect(sentenceMenu()!.textContent).toContain("Say this again");
  useReaderStore().chapter!.blocks[0].segments[0].canSayAgain = false;
  document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await rightClick("0:0:0");
  expect(sentenceMenu()!.textContent).not.toContain("Say this again");
});

it("right-clicking selected text opens the menu on the selection", async () => {
  await openReader();
  await drag(["0:1:0", "lantern"]);
  document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await flushPromises();
  await rightClick("0:1:0");
  expect(sentenceMenu()).toBeNull();
  expect(entries()).toContain("Copy");
  expect(document.getSelection()!.toString(), "the selection is kept").toBe("lantern");
});

it("does not play the sentence a right click opened the menu on, when WebKit follows it with a click", async () => {
  const play = vi.spyOn(mock, "play");
  await openReader();
  await rightClick("0:2:0");
  document
    .querySelector('[data-segment-id="0:2:0"]')!
    .dispatchEvent(new MouseEvent("click", { bubbles: true, button: 0 }));
  await flushPromises();
  expect(play).not.toHaveBeenCalled();
  expect(sentenceMenu()).not.toBeNull();
  const sentence = document.querySelector('[data-segment-id="0:2:0"]')!;
  sentence.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0 }));
  sentence.dispatchEvent(new MouseEvent("click", { bubbles: true, button: 0 }));
  await flushPromises();
  expect(play, "the next click plays").toHaveBeenCalledWith({ bookId: "book-0", chapter: 0, segmentId: "0:2:0" });
});

/** The app's keys (App mounts them), beside the reader. */
const Keys = defineComponent({
  setup() {
    useShortcuts();
    return () => null;
  },
});
/** Presses a key on the focused element. */
function press(key: string, init: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init });
  (document.activeElement ?? document.body).dispatchEvent(event);
  return event;
}
/** Focuses sentence `id` with the keyboard (as Tab does). */
function tabTo(id: string) {
  press("Tab");
  document.querySelector<HTMLElement>(`[data-segment-id="${id}"]`)!.focus();
}

it("starts a selection with Shift and an arrow on a sentence reached with the keyboard, and the menu key opens the menu in it", async () => {
  mount(Keys, { attachTo: document.body });
  const skip = vi.spyOn(mock, "skipChapter");
  await openReader();
  await mock.play({ bookId: "book-0", chapter: 0, segmentId: "0:1:0" });
  await mock.pause();
  await flushPromises();
  tabTo("0:1:1");
  expect(press("ArrowRight", { shiftKey: true }).defaultPrevented).toBe(true);
  expect(document.getSelection()!.toString()).toBe("The");
  expect(skip).not.toHaveBeenCalled();
  // Further on, WebKit extends the selection itself; the key is not a chapter skip.
  expect(press("ArrowRight", { shiftKey: true }).defaultPrevented).toBe(false);
  expect(skip).not.toHaveBeenCalled();

  document.dispatchEvent(new Event("selectionchange"));
  await settle(350);
  expect(entries(), "the selection settled").toContain("Copy");
  expect(document.activeElement?.getAttribute("data-segment-id")).toBe("0:1:1");
  press("ContextMenu");
  await flushPromises();
  expect(document.activeElement?.textContent?.trim()).toBe("Copy");
  press("ArrowDown");
  expect(document.activeElement?.textContent?.trim()).toBe("Highlight");
  press("Escape");
  await flushPromises();
  expect(menu()).toBeNull();
  expect(document.activeElement?.getAttribute("data-segment-id"), "back on the sentence").toBe("0:1:1");

  tabTo("0:2:0");
  press("ArrowLeft", { shiftKey: true });
  expect(document.getSelection()!.toString(), "backwards from its end").toBe("steps.");
  press("F10", { shiftKey: true });
  await flushPromises();
  (document.activeElement as HTMLElement).click();
  await flushPromises();
  expect(clipboard.writeText).toHaveBeenCalledWith("steps.");
});

it("opens the menu of the sentence reached with the keyboard when nothing is selected", async () => {
  await openReader();
  tabTo("0:2:0");
  press("ContextMenu");
  await flushPromises();
  expect(sentenceMenu()).not.toBeNull();
  expect(document.activeElement?.textContent?.trim()).toBe("Listen from here");
});

it("keeps Shift and the arrows for chapters and volume on a sentence that was clicked", async () => {
  mount(Keys, { attachTo: document.body });
  const skip = vi.spyOn(mock, "skipChapter");
  await openReader();
  await mock.play({ bookId: "book-0", chapter: 0, segmentId: "0:1:0" });
  await mock.pause();
  await flushPromises();
  const sentence = document.querySelector<HTMLElement>('[data-segment-id="0:1:1"]')!;
  sentence.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0 }));
  sentence.focus();
  press("ArrowRight", { shiftKey: true });
  await flushPromises();
  expect(document.getSelection()!.isCollapsed || !document.getSelection()!.rangeCount).toBe(true);
  expect(skip).toHaveBeenCalledWith({ delta: 1 });
});
