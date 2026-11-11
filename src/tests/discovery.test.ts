import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "../api/mock";
import FindBooksView from "../views/FindBooksView.vue";
import App from "../App.vue";
import { useLibraryStore } from "../stores/library";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("../api", () => ({ get api() { return backend.api; } }));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => { setActivePinia(createPinia()); mock = createMockApi(); backend.api = mock; });
afterEach(() => { mock.dispose(); });

const titles = (wrapper: ReturnType<typeof mount>) => wrapper.findAll(".find__card h3").map(h => h.text());

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
  expect(fetch).toHaveBeenCalledWith(["h-tide", "h-glass"]);
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
  expect(importer).toHaveBeenCalledWith([{ id: "h-tide", applySuggestion: true }]);
  expect(titles(wrapper)).toEqual(["The Glass Meridian"]);
  expect(useLibraryStore().books.some(b => b.title === "Notes on the Tide")).toBe(true);
  await wrapper.get('[aria-label="Import all new books"]').trigger("click");
  await flushPromises();
  expect(importer).toHaveBeenLastCalledWith([{ id: "h-glass", applySuggestion: true }]);
  wrapper.unmount();
});

it("saves which folders are searched", async () => {
  const save = vi.spyOn(mock, "saveAppSettings");
  const wrapper = mount(FindBooksView);
  await flushPromises();
  await wrapper.get('[aria-label="Folders to search"]').setValue("/home/me/Books\n/media/shelf");
  await wrapper.get('[aria-label="Folders to skip"]').setValue("/home/me/Books/old");
  await wrapper.get('[aria-label="Look for new books when Aoide starts"]').setValue(true);
  await wrapper.get(".find__folders").trigger("submit");
  await flushPromises();
  expect(save).toHaveBeenCalledWith(expect.objectContaining({
    scan: { roots: ["/home/me/Books", "/media/shelf"], exclude: ["/home/me/Books/old"], includeHidden: [], onStartup: true },
  }));
  wrapper.unmount();
});
