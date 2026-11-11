import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "../api/mock";
import LibraryView from "../views/LibraryView.vue";
import MiniPlayer from "../components/player/MiniPlayer.vue";
import UiHost from "../components/ui/UiHost.vue";
import { useLibraryStore } from "../stores/library";
import { usePlayerStore } from "../stores/player";
import { useReaderStore } from "../stores/reader";
import { useNavigationStore } from "../stores/navigation";
import { useUiStore } from "../stores/ui";

import { choose, pickMenuItem } from "./helpers";
const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("../api", () => ({ get api() { return backend.api; } }));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => { setActivePinia(createPinia()); mock = createMockApi(); backend.api = mock; });
afterEach(() => { mock.dispose(); vi.useRealTimers(); });

async function library() {
  await useLibraryStore().start();
  await usePlayerStore().start();
  const wrapper = mount(LibraryView, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}
const titles = (wrapper: ReturnType<typeof mount>) => wrapper.findAll(".book-card__title").map(h => h.text());

it("leads with the book to continue: chapter, progress, time left and Resume", async () => {
  const play = vi.spyOn(mock, "play");
  const wrapper = await library();
  const hero = wrapper.get('[aria-label="Continue listening"]');
  expect(hero.text()).toContain("The Lantern Keeper");
  expect(hero.text()).toContain("Mira Vale · The small light");
  expect(hero.text()).toContain("12%");
  expect(hero.text()).toMatch(/about .*min left at 1×/);
  await hero.get('[aria-label="Resume"]').trigger("click");
  expect(play).toHaveBeenCalledWith("book-0", null, null);
  wrapper.unmount();
});

it("opens a chapter of the continued book from the hero", async () => {
  const play = vi.spyOn(mock, "play");
  const wrapper = await library();
  await wrapper.get('[aria-label="Continue listening"] [aria-haspopup="menu"]').trigger("click");
  await pickMenuItem("Chapter 2: Across the water");
  await flushPromises();
  expect(play).toHaveBeenCalledWith("book-0", 1, "1:0:0");
  expect(useNavigationStore().view).toBe("reader");
  expect(useReaderStore().book?.id).toBe("book-0");
  // Now Playing has the full transport, so the pill steps aside.
  expect(wrapper.find('[aria-label="Now playing"]').exists()).toBe(false);
  wrapper.unmount();
});

it("sums the last 7 days of listening with one bar per day", async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date("2026-11-11T12:00:00"));
  const wrapper = await library();
  const week = wrapper.get('[aria-label="Last 7 days"]');
  expect(week.text()).toContain("1 h 50 min");
  expect(week.findAll(".week-card__bar")).toHaveLength(7);
  wrapper.unmount();
});

it("keeps missing books greyed in the grid and lists books that failed to import apart", async () => {
  const wrapper = await library();
  expect(titles(wrapper)).not.toContain("The Torn Ledger");
  expect(wrapper.findAll(".book-card")).toHaveLength(4);
  expect(wrapper.findAll(".book-card--unavailable")).toHaveLength(1);
  expect(wrapper.text()).toContain("File missing");
  expect(wrapper.get('[aria-label="All"] .library__count').text()).toBe("4");
  expect(wrapper.get('[aria-label="Open The Absent Orchard"]').attributes("disabled")).toBeDefined();

  const failed = wrapper.get('[aria-label="Books that could not be imported"]');
  expect(failed.text()).toContain("1 book could not be imported");
  expect(failed.text()).toContain("The Torn Ledger");
  expect(failed.text()).toContain("This EPUB could not be parsed.");
  await failed.get('[aria-label="Remove The Torn Ledger"]').trigger("click");
  await flushPromises();
  expect(wrapper.find('[aria-label="Books that could not be imported"]').exists()).toBe(false);
  wrapper.unmount();
});

it("filters by in progress, ready offline and preparing", async () => {
  const wrapper = await library();
  const chip = (name: string) => wrapper.get(`[role="group"][aria-label="Filter books"] [aria-label^="${name}"]`);
  expect(chip("All").text()).toContain("4");
  await chip("In progress").trigger("click");
  expect(titles(wrapper)).toEqual(["The Lantern Keeper"]);
  expect(chip("In progress").attributes("aria-pressed")).toBe("true");
  await chip("Ready offline").trigger("click");
  expect(titles(wrapper)).toEqual([]);
  expect(wrapper.text()).toContain("No books here yet");
  await mock.prepareBook("book-1");
  await flushPromises();
  await chip("Preparing").trigger("click");
  expect(titles(wrapper)).toEqual(["A Map of Quiet Places"]);
  expect(wrapper.text()).toContain("Preparing · 3 / 120");
  wrapper.unmount();
});

it("searches the library by title or author", async () => {
  const wrapper = await library();
  await wrapper.get('[aria-label="Search library"]').setValue("map");
  expect(titles(wrapper)).toEqual(["A Map of Quiet Places"]);
  await wrapper.get('[aria-label="Search library"]').setValue("mira");
  expect(titles(wrapper)).toEqual(["The Lantern Keeper"]);
  await wrapper.get('[aria-label="Search library"]').setValue("nothing like this");
  expect(wrapper.text()).toContain("No books match");
  wrapper.unmount();
});

it("sorts the library and remembers the order", async () => {
  const save = vi.spyOn(mock, "saveAppSettings");
  const wrapper = await library();
  expect(titles(wrapper)[0]).toBe("The Lantern Keeper");
  await choose(wrapper, "Sort library", "Title");
  await flushPromises();
  expect(titles(wrapper)).toEqual(["The Absent Orchard", "The Lantern Keeper", "A Map of Quiet Places", "An Unfinished Atlas"]);
  expect(save).toHaveBeenCalledWith(expect.objectContaining({ librarySort: "title" }));
  wrapper.unmount();
});

it("removes a book after confirming in a dialog from its card menu", async () => {
  const remove = vi.spyOn(mock, "removeBook");
  const wrapper = await library();
  const host = mount(UiHost, { attachTo: document.body });
  const ask = async () => {
    await wrapper.get('[aria-label="More for An Unfinished Atlas"]').trigger("click");
    document.body.querySelector<HTMLElement>('[role="menuitem"][aria-label="Remove An Unfinished Atlas from the library"]')!.click();
    await flushPromises();
    return host.get('[role="alertdialog"]');
  };
  let dialog = await ask();
  expect(dialog.text()).toContain("Remove An Unfinished Atlas from the library?");
  expect(dialog.text()).toContain("The EPUB file stays where it is");
  await dialog.get('[data-confirm="no"]').trigger("click");
  await flushPromises();
  expect(remove).not.toHaveBeenCalled();
  dialog = await ask();
  await dialog.get('[data-confirm="yes"]').trigger("click");
  await flushPromises();
  expect(remove).toHaveBeenCalledWith("book-3");
  expect(titles(wrapper)).not.toContain("An Unfinished Atlas");
  wrapper.unmount();
  host.unmount();
});

it("floats a mini player once something is playing, and opens Now Playing from it", async () => {
  await useLibraryStore().start();
  await usePlayerStore().start();
  const wrapper = mount(MiniPlayer);
  await flushPromises();
  expect(wrapper.find('[aria-label="Now playing"]').exists()).toBe(false);
  await mock.play("book-0", 1, "1:1:0");
  await mock.setSpeed(1.25);
  await mock.setSleepTimer(30, false);
  await flushPromises();
  const mini = wrapper.get('[aria-label="Now playing"]');
  expect(mini.text()).toContain("Across the water");
  expect(mini.text()).toContain("The Lantern Keeper · 1.25× · sleep 30:00");
  await mini.get('[aria-label="Pause"]').trigger("click");
  await flushPromises();
  expect(usePlayerStore().state.status).toBe("paused");
  const skip = vi.spyOn(mock, "skipChapter");
  await mini.get('[aria-label="Next chapter"]').trigger("click");
  expect(skip).toHaveBeenCalledWith(1);
  await mini.get('[aria-label="Open Now Playing"]').trigger("click");
  await flushPromises();
  expect(useNavigationStore().view).toBe("reader");
  expect(useReaderStore().book?.id).toBe("book-0");
  // Now Playing has the full transport, so the pill steps aside.
  expect(wrapper.find('[aria-label="Now playing"]').exists()).toBe(false);
  wrapper.unmount();
});

it("keeps the playing chapter in the mini player when another book is open, and shows errors", async () => {
  await useReaderStore().open("book-1");
  useNavigationStore().view = "library";
  await usePlayerStore().start();
  const wrapper = mount(MiniPlayer);
  await mock.play("book-0", 1, "1:1:0");
  mock.emitGeneration({ bookId: "book-0", generatingSegmentId: null, bufferedAheadMs: 0, pending: 1, error: "Engine offline", preparing: null });
  await flushPromises();
  expect(wrapper.text()).toContain("Across the water");
  expect(wrapper.text()).toContain("The Lantern Keeper");
  expect(wrapper.get('[role="alert"]').text()).toContain("Engine offline");
  wrapper.unmount();
});

const menuItems = () => [...document.body.querySelectorAll<HTMLElement>('[role="menu"] [role="menuitem"]')];
async function cardAction(wrapper: ReturnType<typeof mount>, title: string, item: string) {
  await wrapper.get(`[aria-label="More for ${title}"]`).trigger("click");
  const match = menuItems().find(m => m.textContent?.trim() === item);
  if (!match) throw new Error(`No "${item}" in ${menuItems().map(m => m.textContent?.trim()).join(", ")}`);
  match.click();
  await flushPromises();
}
const toasts = () => useUiStore().toasts.map(t => t.text);

it("prepares, pauses, resumes and cancels a whole book from its card menu", async () => {
  const prepare = vi.spyOn(mock, "prepareBook"), pause = vi.spyOn(mock, "setPreparePaused"), cancel = vi.spyOn(mock, "cancelPrepare");
  const wrapper = await library();
  await cardAction(wrapper, "A Map of Quiet Places", "Prepare whole book");
  expect(prepare).toHaveBeenCalledWith("book-1");
  await cardAction(wrapper, "A Map of Quiet Places", "Pause preparing");
  expect(pause).toHaveBeenLastCalledWith("book-1", true);
  await cardAction(wrapper, "A Map of Quiet Places", "Resume preparing");
  expect(pause).toHaveBeenLastCalledWith("book-1", false);
  await cardAction(wrapper, "A Map of Quiet Places", "Cancel preparing");
  expect(cancel).toHaveBeenCalledWith("book-1");
  wrapper.unmount();
});

it("exports a book and deletes its generated audio after confirming, from the card menu", async () => {
  const exportBook = vi.spyOn(mock, "exportBook"), regenerate = vi.spyOn(mock, "regenerateBook");
  const wrapper = await library();
  const host = mount(UiHost, { attachTo: document.body });
  await cardAction(wrapper, "The Lantern Keeper", "Export as M4B");
  expect(exportBook).toHaveBeenCalledWith("book-0");
  expect(toasts().join(" ")).toContain("Exported to ~/Documents/Aoide/Exports/The Lantern Keeper.m4b");

  await cardAction(wrapper, "The Lantern Keeper", "Delete generated audio");
  const dialog = host.get('[role="alertdialog"]');
  expect(dialog.text()).toContain("Delete the generated audio of The Lantern Keeper?");
  await dialog.get('[data-confirm="yes"]').trigger("click");
  await flushPromises();
  expect(regenerate).toHaveBeenCalledWith("book-0");
  await wrapper.get('[aria-label="More for The Lantern Keeper"]').trigger("click");
  expect(menuItems().map(m => m.textContent?.trim())).not.toContain("Delete generated audio");
  wrapper.unmount();
  host.unmount();
});

it("offers only removal for a book whose file is missing", async () => {
  const wrapper = await library();
  await wrapper.get('[aria-label="More for The Absent Orchard"]').trigger("click");
  expect(menuItems().map(m => m.textContent?.trim())).toEqual(["Remove from library"]);
  wrapper.unmount();
});

const chip = (wrapper: ReturnType<typeof mount>, name: string) => wrapper.get(`[role="group"][aria-label="Filter books"] [aria-label^="${name}"]`);
async function answerPrompt(host: ReturnType<typeof mount>, text: string) {
  const dialog = host.get('[role="dialog"]');
  await dialog.get("input").setValue(text);
  await dialog.get("form").trigger("submit");
  await flushPromises();
}

it("creates a collection, puts a book on it from the card menu, and filters by it", async () => {
  const create = vi.spyOn(mock, "createCollection"), shelve = vi.spyOn(mock, "setBookCollections");
  const wrapper = await library();
  const host = mount(UiHost, { attachTo: document.body });
  await chip(wrapper, "New collection").trigger("click");
  await flushPromises();
  expect(host.get('[role="dialog"]').text()).toContain("New collection");
  await answerPrompt(host, "Night reading");
  expect(create).toHaveBeenCalledWith("Night reading");
  expect(chip(wrapper, "Night reading").text()).toContain("0");

  await wrapper.get('[aria-label="More for The Lantern Keeper"]').trigger("click");
  const item = document.body.querySelector<HTMLElement>('[role="menuitemcheckbox"][aria-label="Night reading"]')!;
  expect(item.getAttribute("aria-checked")).toBe("false");
  item.click();
  await flushPromises();
  expect(shelve).toHaveBeenCalledWith("book-0", [expect.any(Number)]);
  expect(chip(wrapper, "Night reading").text()).toContain("1");

  await chip(wrapper, "Night reading").trigger("click");
  expect(titles(wrapper)).toEqual(["The Lantern Keeper"]);
  await wrapper.get('[aria-label="More for The Lantern Keeper"]').trigger("click");
  expect(document.body.querySelector('[role="menuitemcheckbox"][aria-label="Night reading"]')!.getAttribute("aria-checked")).toBe("true");
  wrapper.unmount();
  host.unmount();
});

it("adds a book to a new collection straight from its card menu", async () => {
  const wrapper = await library();
  const host = mount(UiHost, { attachTo: document.body });
  await wrapper.get('[aria-label="More for A Map of Quiet Places"]').trigger("click");
  [...document.body.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(m => m.textContent?.trim() === "New collection…")!.click();
  await flushPromises();
  await answerPrompt(host, "Maps");
  expect(chip(wrapper, "Maps").text()).toContain("1");
  wrapper.unmount();
  host.unmount();
});

it("renames and deletes the selected collection", async () => {
  const rename = vi.spyOn(mock, "renameCollection"), remove = vi.spyOn(mock, "deleteCollection");
  const id = await mock.createCollection("Shelf");
  await mock.setBookCollections("book-1", [id]);
  const wrapper = await library();
  const host = mount(UiHost, { attachTo: document.body });
  await chip(wrapper, "Shelf").trigger("click");
  await wrapper.get('[aria-label="Rename collection"]').trigger("click");
  await flushPromises();
  expect((host.get('[role="dialog"] input').element as HTMLInputElement).value).toBe("Shelf");
  await answerPrompt(host, "Favourites");
  expect(rename).toHaveBeenCalledWith(id, "Favourites");
  expect(chip(wrapper, "Favourites").attributes("aria-pressed")).toBe("true");

  await wrapper.get('[aria-label="Delete collection"]').trigger("click");
  await flushPromises();
  expect(host.get('[role="alertdialog"]').text()).toContain("The books stay in your library");
  await host.get('[data-confirm="yes"]').trigger("click");
  await flushPromises();
  expect(remove).toHaveBeenCalledWith(id);
  expect(wrapper.find('[aria-label^="Favourites"]').exists()).toBe(false);
  expect(chip(wrapper, "All").attributes("aria-pressed")).toBe("true");
  expect(titles(wrapper)).toHaveLength(4);
  wrapper.unmount();
  host.unmount();
});

it("writes the size of a book's audio the way Storage does", async () => {
  const wrapper = await library();
  expect(wrapper.findAll(".book-card").find(card => card.text().includes("The Lantern Keeper"))!.text()).toContain("12% · 2.3 MB");
  wrapper.unmount();
});

it("shows a book's file in the file manager from its card menu", async () => {
  const show = vi.spyOn(mock, "showInFolder");
  const wrapper = await library();
  await wrapper.get('[aria-label="More for A Map of Quiet Places"]').trigger("click");
  await pickMenuItem("Show in folder");
  expect(show).toHaveBeenCalledWith("book-1");
  wrapper.unmount();
});

it("switches playback to another started book clicked while one is playing", async () => {
  mock.setProgressForTest("book-1", 0.3);
  const wrapper = await library();
  await usePlayerStore().play("book-0", null, null);
  await flushPromises();
  const play = vi.spyOn(mock, "play");
  await wrapper.get('[aria-label="Open A Map of Quiet Places"]').trigger("click");
  await flushPromises();
  expect(useReaderStore().book?.id).toBe("book-1");
  expect(play).toHaveBeenCalledWith("book-1", null, null);
  wrapper.unmount();
});

it("only opens a started book when nothing is playing", async () => {
  mock.setProgressForTest("book-1", 0.3);
  const wrapper = await library();
  const play = vi.spyOn(mock, "play");
  await wrapper.get('[aria-label="Open A Map of Quiet Places"]').trigger("click");
  await flushPromises();
  expect(useReaderStore().book?.id).toBe("book-1");
  expect(play).not.toHaveBeenCalled();
  wrapper.unmount();
});
