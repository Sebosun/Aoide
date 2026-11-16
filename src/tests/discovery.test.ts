import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import FindBooksView from "@/views/FindBooksView.vue";
import App from "@/App.vue";
import { useLibraryStore } from "@/stores/library";
import { useRecordingsStore } from "@/stores/recordings";

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

const titles = (wrapper: ReturnType<typeof mount>) => wrapper.findAll(".find__card h3").map((h) => h.text());

it("scans and shows only books that are not in the library, with counts per group", async () => {
  const wrapper = mount(FindBooksView);
  await flushPromises();
  expect(wrapper.text()).toContain("Scan this computer");
  await wrapper.get('[aria-label="Scan for books"]').trigger("click");
  await flushPromises();
  expect(titles(wrapper)).toEqual(["Notes on the Tide", "The Glass Meridian"]);
  expect(wrapper.get('[aria-label="Show new books"]').text()).toContain("2");
  expect(wrapper.text()).toContain("2 copies");
  await wrapper.get('[aria-label="Show books already in the library"]').trigger("click");
  expect(titles(wrapper)).toEqual(["The Lantern Keeper"]);
  await wrapper.get('[aria-label="Show unreadable files"]').trigger("click");
  expect(wrapper.text()).toContain("not a readable EPUB");
  wrapper.unmount();
});

it("fetches online details only when asked and shows the suggestion", async () => {
  const fetch = vi.spyOn(mock, "fetchDiscoveredDetails");
  const wrapper = mount(FindBooksView);
  await wrapper.get('[aria-label="Scan for books"]').trigger("click");
  await flushPromises();
  expect(fetch).not.toHaveBeenCalled();
  await wrapper.get('[aria-label="Fetch details online"]').trigger("click");
  await flushPromises();
  expect(fetch).toHaveBeenCalledWith({ ids: ["h-tide", "h-glass"] });
  expect(wrapper.text()).toContain("Preview Press");
  wrapper.unmount();
});

it("imports the selected books, applying chosen online details, and refreshes the library", async () => {
  await useLibraryStore().start();
  const importer = vi.spyOn(mock, "importDiscovered");
  const wrapper = mount(FindBooksView);
  await wrapper.get('[aria-label="Scan for books"]').trigger("click");
  await flushPromises();
  await wrapper.get('[aria-label="Fetch details online"]').trigger("click");
  await flushPromises();
  await wrapper.get('[aria-label="Select Notes on the Tide"]').setValue(true);
  await wrapper.get('[aria-label="Import selected books"]').trigger("click");
  await flushPromises();
  expect(importer).toHaveBeenCalledWith({ choices: [{ id: "h-tide", applySuggestion: true, pairAudiobook: true }] });
  expect(titles(wrapper)).toEqual(["The Glass Meridian"]);
  expect(useLibraryStore().books.some((b) => b.title === "Notes on the Tide")).toBe(true);
  await wrapper.get('[aria-label="Import all new books"]').trigger("click");
  await flushPromises();
  expect(importer).toHaveBeenLastCalledWith({
    choices: [{ id: "h-glass", applySuggestion: true, pairAudiobook: false }],
  });
  wrapper.unmount();
});

it("adds the folders to search with the system's chooser and saves each change", async () => {
  const save = vi.spyOn(mock, "updateAppSettings");
  const chooser = vi
    .spyOn(mock, "chooseFolder")
    .mockResolvedValueOnce("/home/me/Books")
    .mockResolvedValueOnce("/media/shelf")
    .mockResolvedValueOnce(null)
    .mockResolvedValueOnce("/home/me/Books/old");
  const wrapper = mount(FindBooksView);
  await flushPromises();
  const folders = wrapper.get('[aria-label="Folders to search"]');
  // Nothing chosen: the home folder is searched, and no path has to be typed anywhere.
  expect(folders.text()).toContain("Your home folder");
  expect(wrapper.find("textarea").exists()).toBe(false);
  const add = (label: string) => wrapper.get(`[aria-label="${label}"]`).trigger("click").then(flushPromises);
  await add("Add a folder to search");
  await add("Add a folder to search");
  await add("Add a folder to search"); // cancelled: nothing changes
  await add("Add a folder to skip");
  expect(chooser).toHaveBeenCalledTimes(4);
  await wrapper.get('[aria-label="Look for new books when Aoide starts"]').setValue(true);
  await flushPromises();
  expect(save).toHaveBeenLastCalledWith({
    patch: expect.objectContaining({
      scan: {
        roots: ["/home/me/Books", "/media/shelf"],
        exclude: ["/home/me/Books/old"],
        includeHidden: [],
        onStartup: true,
      },
    }),
  });
  await wrapper.get('[aria-label="Remove /media/shelf"]').trigger("click");
  await flushPromises();
  expect(save).toHaveBeenLastCalledWith({
    patch: expect.objectContaining({ scan: expect.objectContaining({ roots: ["/home/me/Books"] }) }),
  });
  expect(wrapper.get('[aria-label="Folders to search"]').text()).not.toContain("/media/shelf");
  wrapper.unmount();
});

it("shows audiobooks found beside new books and pairs them on import unless told not to", async () => {
  const importer = vi.spyOn(mock, "importDiscovered");
  const wrapper = mount(FindBooksView);
  await wrapper.get('[aria-label="Scan for books"]').trigger("click");
  await flushPromises();
  const tide = wrapper.findAll(".find__card").find((c) => c.text().includes("Notes on the Tide"))!;
  expect(tide.get(".find__audiobook").text()).toContain("Notes on the Tide (Unabridged).m4b");
  expect(wrapper.findAll(".find__audiobook")).toHaveLength(1);
  await wrapper.get('[aria-label="Pair audiobooks found beside books when importing"]').setValue(false);
  await wrapper.get('[aria-label="Select Notes on the Tide"]').setValue(true);
  await wrapper.get('[aria-label="Import selected books"]').trigger("click");
  await flushPromises();
  expect(importer).toHaveBeenCalledWith({ choices: [{ id: "h-tide", applySuggestion: false, pairAudiobook: false }] });
  wrapper.unmount();
});

it("pairs the audiobook found beside a book already in the library through the pairing dialog", async () => {
  const wrapper = mount(FindBooksView);
  await wrapper.get('[aria-label="Scan for books"]').trigger("click");
  await flushPromises();
  await wrapper.get('[aria-label="Show books already in the library"]').trigger("click");
  expect(wrapper.get(".find__audiobook").text()).toContain("The Lantern Keeper");
  await wrapper.get('[aria-label="Pair the audiobook beside The Lantern Keeper"]').trigger("click");
  expect(useRecordingsStore().pairing).toEqual({
    paths: ["~/Documents/The Lantern Keeper.m4b"],
    bookId: "book-0",
  });
  await useRecordingsStore().attach("book-0", ["~/Documents/The Lantern Keeper.m4b"]);
  await flushPromises();
  expect(wrapper.find('[aria-label="Pair the audiobook beside The Lantern Keeper"]').exists()).toBe(false);
  expect(wrapper.text()).toContain("Paired");
  wrapper.unmount();
});
