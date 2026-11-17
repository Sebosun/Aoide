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

it("offers Copy, Search the book and Listen from here when words are selected", async () => {
  await openReader();
  expect(menu()).toBeNull();
  await drag(["0:1:0", "carried a lantern"], ["0:1:0", "end of the pier."]);
  expect(entries()).toEqual(["Copy", "Search the book", "Listen from here"]);
  await drag(["0:1:0", "lantern"]);
  expect(entries(), "a word or a few can be said differently").toEqual([
    "Copy",
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
