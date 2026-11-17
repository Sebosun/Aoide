import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import NarrationDialog from "@/components/book/NarrationDialog.vue";
import SettingsDialog from "@/components/settings/SettingsDialog.vue";
import ReaderView from "@/views/ReaderView.vue";
import { useLibraryStore } from "@/stores/library";
import { useReaderStore } from "@/stores/reader";
import { usePlayerStore } from "@/stores/player";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("@/api", () => ({
  get api() {
    return backend.api;
  },
}));
let mock: ReturnType<typeof createMockApi>;
const play = vi.fn(() => Promise.resolve());
beforeEach(() => {
  setActivePinia(createPinia());
  mock = createMockApi();
  backend.api = mock;
  play.mockClear();
  window.HTMLMediaElement.prototype.play = play;
  window.HTMLMediaElement.prototype.pause = vi.fn();
  URL.createObjectURL = vi.fn(() => "blob:preview");
  URL.revokeObjectURL = vi.fn();
});
afterEach(() => {
  mock.dispose();
  document.body.innerHTML = "";
});

/** Lets the form's count of sentences come back. */
const counted = async () => {
  await new Promise((resolve) => setTimeout(resolve, 350));
  await flushPromises();
};
const button = (wrapper: { findAll: ReturnType<typeof mount>["findAll"] }, text: string) =>
  wrapper.findAll("button").find((b) => b.text() === text)!;

it("keeps a book's pronunciations: says how many sentences a word is in, hears it, adds, changes and removes it", async () => {
  const hear = vi.spyOn(mock, "previewPronunciation");
  const save = vi.spyOn(mock, "savePronunciation");
  const wrapper = mount(NarrationDialog, { props: { bookId: "book-1", title: "Ren" }, attachTo: document.body });
  await flushPromises();
  const list = () => wrapper.get('[aria-label="Pronunciations for this book"]');
  expect(list().text()).toContain("No words yet");

  await list().get('[aria-label="Written word"]').setValue("Ren");
  await list().get('[aria-label="Said as"]').setValue("wren");
  await counted();
  expect(list().text()).toContain("Said differently in 4 sentences.");
  await list().get('[aria-label="Hear it"]').trigger("click");
  await flushPromises();
  expect(hear).toHaveBeenCalledWith({ bookId: "book-1", spoken: "wren" });
  expect(play).toHaveBeenCalled();

  await button(list(), "Add").trigger("click");
  await flushPromises();
  expect(save).toHaveBeenCalledWith({ bookId: "book-1", written: "Ren", spoken: "wren", matchCase: false });
  expect(list().get("li").text()).toContain("Ren");
  expect(list().get("li").text()).toContain("wren");
  expect((list().get('[aria-label="Written word"]').element as HTMLInputElement).value, "the form is empty").toBe("");

  await list().get('[aria-label="Change Ren"]').trigger("click");
  expect((list().get('[aria-label="Said as"]').element as HTMLInputElement).value).toBe("wren");
  await list().get('[aria-label="Said as"]').setValue("rain");
  await button(list(), "Save").trigger("click");
  await flushPromises();
  expect(list().findAll("li")).toHaveLength(1);
  expect(list().get("li").text()).toContain("rain");

  await list().get('[aria-label="Hear Ren"]').trigger("click");
  await flushPromises();
  expect(hear).toHaveBeenLastCalledWith({ bookId: "book-1", spoken: "rain" });

  await list().get('[aria-label="Remove Ren"]').trigger("click");
  await flushPromises();
  expect(await mock.pronunciations({ bookId: "book-1" })).toEqual([]);
  expect(list().text()).toContain("No words yet");
  wrapper.unmount();
});

it("keeps the pronunciations for all books under Settings, Speech engine", async () => {
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  await wrapper
    .findAll('[role="tab"]')
    .find((t) => t.text() === "Speech engine")!
    .trigger("click");
  await flushPromises();
  const list = wrapper.get('[aria-label="Pronunciations for all books"]');
  await list.get('[aria-label="Written word"]').setValue("SQL");
  await list.get('[aria-label="Said as"]').setValue("sequel");
  await list.get('[aria-label="Match case"]').setValue(true);
  await button(list, "Add").trigger("click");
  await flushPromises();
  expect(await mock.pronunciations({ bookId: null })).toMatchObject([
    { bookId: null, written: "SQL", spoken: "sequel", matchCase: true },
  ]);
  expect(list.get("li").text()).toContain("SQL");
  wrapper.unmount();
});

it("says a word selected in the reader differently, in that book", async () => {
  await useLibraryStore().start();
  await useReaderStore().open("book-1");
  await usePlayerStore().start();
  const wrapper = mount(ReaderView, { attachTo: document.body });
  await flushPromises();
  const offer = () => wrapper.find('[aria-label="Say this differently…"]');
  expect(offer().exists(), "nothing is selected").toBe(false);

  const walker = document.createTreeWalker(wrapper.get('[aria-label="Chapter text"]').element, NodeFilter.SHOW_TEXT);
  let text = walker.nextNode();
  while (text && !/\bRen\b/.test(text.textContent ?? "")) text = walker.nextNode();
  const at = text!.textContent!.search(/\bRen\b/);
  const range = document.createRange();
  range.setStart(text!, at);
  range.setEnd(text!, at + 3);
  document.getSelection()!.removeAllRanges();
  document.getSelection()!.addRange(range);
  document.dispatchEvent(new Event("selectionchange"));
  await flushPromises();

  await offer().trigger("click");
  await flushPromises();
  const dialog = wrapper.get('[role="dialog"][aria-label="Say this differently"]');
  expect((dialog.get('[aria-label="Written word"]').element as HTMLInputElement).value).toBe("Ren");
  await dialog.get('[aria-label="Said as"]').setValue("wren");
  await button(dialog, "Save for this book").trigger("click");
  await flushPromises();
  expect(await mock.pronunciations({ bookId: "book-1" })).toMatchObject([{ written: "Ren", spoken: "wren" }]);
  expect(wrapper.find('[role="dialog"][aria-label="Say this differently"]').exists()).toBe(false);
  wrapper.unmount();
});
