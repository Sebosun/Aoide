import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import ReaderView from "@/views/ReaderView.vue";
import { useReaderStore } from "@/stores/reader";
import { usePlayerStore } from "@/stores/player";
import { useLibraryStore } from "@/stores/library";
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
  document.body.innerHTML = "";
});

async function reader(book = "book-0") {
  await useLibraryStore().start();
  await useReaderStore().open(book);
  await usePlayerStore().start();
  const wrapper = mount(ReaderView, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}

it("keeps every book tool at the top: the player at the foot only plays", async () => {
  const wrapper = await reader();
  await usePlayerStore().play("book-0", 0, null);
  await flushPromises();
  const dock = wrapper.get('[aria-label="Player"]');
  expect(dock.find('[aria-label="Add bookmark"]').exists()).toBe(false);
  expect(dock.find('[aria-label="Book tools"]').exists()).toBe(false);
  const header = wrapper.get(".reader__actions");
  for (const label of ["Search this book", "Narration", "Bookmark this place", "Notes and details"])
    expect(header.find(`[aria-label="${label}"]`).exists(), label).toBe(true);
  wrapper.unmount();
});

it("opens book tools as a modal bottom sheet with three tabs, and Escape returns to its button", async () => {
  const wrapper = await reader();
  const button = wrapper.get('[aria-label="Notes and details"]');
  await button.trigger("click");
  await flushPromises();
  const sheet = wrapper.get('[role="dialog"]');
  expect(sheet.attributes("aria-modal")).toBe("true");
  expect(sheet.attributes("aria-label")).toBe("Book tools");
  expect(sheet.findAll('[role="tab"]').map((t) => t.text())).toEqual(["Notes", "Search", "Details"]);
  const selected = sheet.get('[role="tab"][aria-selected="true"]');
  expect(selected.text()).toBe("Notes");
  expect(document.activeElement).toBe(selected.element);

  await sheet.trigger("keydown", { key: "Escape" });
  await flushPromises();
  expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
  expect(document.activeElement).toBe(button.element);
  wrapper.unmount();
});

it("moves between tabs with the arrow keys and closes from the backdrop", async () => {
  const wrapper = await reader();
  await wrapper.get('[aria-label="Notes and details"]').trigger("click");
  await flushPromises();
  await wrapper.get('[role="tab"][aria-selected="true"]').trigger("keydown", { key: "ArrowRight" });
  expect(wrapper.get('[role="tab"][aria-selected="true"]').text()).toBe("Search");
  expect(wrapper.find('[role="search"]').exists()).toBe(true);
  await wrapper.get('[role="tab"][aria-selected="true"]').trigger("keydown", { key: "ArrowLeft" });
  await wrapper.get('[role="tab"][aria-selected="true"]').trigger("keydown", { key: "ArrowLeft" });
  expect(wrapper.get('[role="tab"][aria-selected="true"]').text()).toBe("Details");
  await wrapper.get(".ui-dialog__backdrop").trigger("click");
  expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
  wrapper.unmount();
});

it("opens the extraction notes in Details from the warning in the margin", async () => {
  const wrapper = await reader("book-1");
  const warning = wrapper.get('[aria-label="About this book"] .book-margin__warning');
  expect(warning.text()).toContain("3 extraction notes");
  await warning.trigger("click");
  await flushPromises();
  const sheet = wrapper.get('[role="dialog"]');
  expect(sheet.get('[role="tab"][aria-selected="true"]').text()).toBe("Details");
  expect(sheet.text()).toContain("2 warnings, 1 section left out");
  expect(sheet.text()).toContain("Copyright");
  expect(sheet.text()).toContain("An empty section was skipped");
  wrapper.unmount();
});

it("searches from the top of the reader, and a picked result closes the search", async () => {
  const play = vi.spyOn(mock, "play");
  const wrapper = await reader();
  await wrapper.get('[aria-label="Search this book"]').trigger("click");
  await flushPromises();
  const sheet = wrapper.get('[role="dialog"]');
  expect(sheet.get('[role="tab"][aria-selected="true"]').text()).toBe("Search");
  expect(document.activeElement).toBe(sheet.get('[aria-label="Search in book"]').element);
  await sheet.get('[aria-label="Search in book"]').setValue("lantern");
  await sheet.get(".book-search__form").trigger("submit");
  await flushPromises();
  await wrapper.get(".book-search__hit").trigger("click");
  await flushPromises();
  expect(play).toHaveBeenCalledWith({ bookId: "book-0", chapter: 0, segmentId: "0:1:0" });
  expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
  wrapper.unmount();
});

it("opens who reads the book in its own dialog from the top", async () => {
  const wrapper = await reader();
  await wrapper.get('[aria-label="Narration"]').trigger("click");
  await flushPromises();
  const dialog = wrapper.get('[role="dialog"][aria-label="Narration"]');
  expect(dialog.find('[aria-label="Search in book"]').exists()).toBe(false);
  expect(dialog.text()).toContain("Voice");
  await dialog.trigger("keydown", { key: "Escape" });
  await flushPromises();
  expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
  wrapper.unmount();
});

it("flags a book voice the engine does not offer and opens Narration from the margin", async () => {
  await mock.saveBookSettings({
    bookId: "book-0",
    settings: {
      voiceId: "Abigail.wav",
      params: {},
      language: null,
      readNotes: false,
      narration: null,
      speakMissing: false,
      mode: null,
      leftOut: [],
      playbackSpeed: null,
      shortenSilences: null,
    },
  });
  const wrapper = await reader("book-0");
  const warning = wrapper.get('[aria-label="About this book"] .book-margin__warning--voice');
  expect(warning.text()).toContain("Voice not available");
  expect(warning.attributes("title")).toContain("Abigail");
  await warning.trigger("click");
  await flushPromises();
  expect(wrapper.find('[role="dialog"][aria-label="Narration"]').exists()).toBe(true);
  wrapper.unmount();
});

it("shows the book's file in the file manager from Details", async () => {
  const show = vi.spyOn(mock, "showInFolder");
  const wrapper = await reader("book-1");
  await wrapper.get('[aria-label="About this book"] .book-margin__warning').trigger("click");
  await flushPromises();
  const button = wrapper
    .get('[role="dialog"]')
    .findAll("button")
    .find((b) => b.text() === "Show in folder");
  await button!.trigger("click");
  expect(show).toHaveBeenCalledWith({ bookId: "book-1" });
  wrapper.unmount();
});

it("goes from the reader to the book's page through its cover and title in the margin", async () => {
  const wrapper = await reader("book-0");
  await wrapper.get('[aria-label="About this book"] [aria-label="Book page for The Lantern Keeper"]').trigger("click");
  expect(useNavigationStore().view).toBe("item");
  expect(useNavigationStore().bookId).toBe("book-0");
  wrapper.unmount();
});
