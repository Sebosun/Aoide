import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import App from "@/App.vue";
import LibraryView from "@/views/LibraryView.vue";
import MiniPlayer from "@/components/player/MiniPlayer.vue";
import UiHost from "@/components/ui/UiHost.vue";
import { useLibraryStore } from "@/stores/library";
import { usePlayerStore } from "@/stores/player";
import { useReaderStore } from "@/stores/reader";
import { useNavigationStore } from "@/stores/navigation";
import { useUiStore } from "@/stores/ui";
import { useRecordingsStore } from "@/stores/recordings";

import { choose, openMenu, pickMenuItem } from "@/tests/helpers";
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
  vi.useRealTimers();
});

async function library() {
  await useLibraryStore().start();
  await usePlayerStore().start();
  const wrapper = mount(LibraryView, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}
const titles = (wrapper: ReturnType<typeof mount>) => wrapper.findAll(".book-card__title").map((h) => h.text());

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
  expect(failed.text()).toContain("invalid EPUB");
  await failed.get('[aria-label="Remove The Torn Ledger"]').trigger("click");
  await flushPromises();
  expect(wrapper.find('[aria-label="Books that could not be imported"]').exists()).toBe(false);
  wrapper.unmount();
});

it("filters by unread, in progress, finished, can listen and preparing, from the core's fields", async () => {
  const wrapper = await library();
  const chip = (name: string) => wrapper.get(`[role="group"][aria-label="Filter books"] [aria-label^="${name}"]`);
  expect(chip("All").text()).toContain("4");
  await chip("Unread").trigger("click");
  expect(titles(wrapper)).toEqual(["An Unfinished Atlas", "The Absent Orchard", "A Map of Quiet Places"]);
  await chip("In progress").trigger("click");
  expect(titles(wrapper)).toEqual(["The Lantern Keeper"]);
  expect(chip("In progress").attributes("aria-pressed")).toBe("true");
  await chip("Finished").trigger("click");
  expect(titles(wrapper)).toEqual([]);
  mock.setAudioSourceForTest("book-3", "recording");
  await flushPromises();
  await chip("Can listen").trigger("click");
  expect(titles(wrapper)).toEqual(["The Lantern Keeper", "An Unfinished Atlas"]);
  expect(wrapper.find('[aria-label^="Preparing"]').exists()).toBe(false);
  await mock.prepareBook("book-1");
  await flushPromises();
  await chip("Preparing").trigger("click");
  expect(titles(wrapper)).toEqual(["A Map of Quiet Places"]);
  expect(wrapper.text()).toContain("Preparing · 3 / 120");
  wrapper.unmount();
});

it("says on each card what it has and how far along it is", async () => {
  const wrapper = await library();
  const line = (title: string) =>
    wrapper
      .findAll(".book-card")
      .find((card) => card.text().includes(title))!
      .get(".book-card__status")
      .text();
  expect(line("The Lantern Keeper")).toBe("Text + voice · 12% · <1 min left");
  expect(line("A Map of Quiet Places")).toBe("Text · unread");
  // A book read silently gives the time left to read it.
  await mock.setReadingPlace("book-1", 1, "1:1:0");
  await flushPromises();
  expect(line("A Map of Quiet Places")).toMatch(/^Text · \d+% · <1 min left to read$/);
  mock.setAudioSourceForTest("book-3", "recording");
  await flushPromises();
  expect(line("An Unfinished Atlas")).toBe("Text + recording · unread");
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
  const save = vi.spyOn(mock, "updateAppSettings");
  const wrapper = await library();
  expect(titles(wrapper)[0]).toBe("The Lantern Keeper");
  await choose(wrapper, "Sort library", "Title");
  await flushPromises();
  expect(titles(wrapper)).toEqual([
    "The Absent Orchard",
    "The Lantern Keeper",
    "A Map of Quiet Places",
    "An Unfinished Atlas",
  ]);
  expect(save).toHaveBeenCalledWith(expect.objectContaining({ librarySort: "title" }));
  wrapper.unmount();
});

it("removes a book after confirming in a dialog from its card menu", async () => {
  const remove = vi.spyOn(mock, "removeBook");
  const wrapper = await library();
  const host = mount(UiHost, { attachTo: document.body });
  const ask = async () => {
    await wrapper.get('[aria-label="More for An Unfinished Atlas"]').trigger("click");
    document.body
      .querySelector<HTMLElement>('[role="menuitem"][aria-label="Remove An Unfinished Atlas from the library"]')!
      .click();
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

it("shows the playing book in the now-playing card, and opens the reader from it", async () => {
  await useLibraryStore().start();
  await usePlayerStore().start();
  const wrapper = mount(MiniPlayer);
  await flushPromises();
  expect(wrapper.find('[aria-label="Now playing"]').exists()).toBe(false);
  await mock.play("book-0", 1, "1:1:0");
  await mock.setSpeed(1.25);
  await mock.setSleepTimer(30, false);
  await flushPromises();
  const card = wrapper.get('[aria-label="Now playing"]');
  expect(card.text()).toContain("The Lantern Keeper");
  expect(card.text()).toContain("Playing · Across the water · 1.25× · sleep 30:00");
  await card.get('[aria-label="Pause The Lantern Keeper"]').trigger("click");
  await flushPromises();
  expect(usePlayerStore().state.status).toBe("paused");
  expect(card.text()).toContain("Paused · 1.25×");
  await card.get('[aria-label="Open The Lantern Keeper in the reader"]').trigger("click");
  await flushPromises();
  expect(useNavigationStore().view).toBe("reader");
  expect(useReaderStore().book?.id).toBe("book-0");
  // The reader has the full transport, so the card steps aside.
  expect(wrapper.find('[aria-label="Now playing"]').exists()).toBe(false);
  wrapper.unmount();
});

it("keeps the playing book in the card when another book is open, and shows errors", async () => {
  await useReaderStore().open("book-1");
  useNavigationStore().view = "library";
  await usePlayerStore().start();
  const wrapper = mount(MiniPlayer);
  await mock.play("book-0", 1, "1:1:0");
  await flushPromises();
  expect(wrapper.text()).toContain("Across the water");
  mock.emitGeneration({
    bookId: "book-0",
    generatingSegmentId: null,
    bufferedAheadMs: 0,
    pending: 1,
    error: "Engine offline",
    preparing: null,
  });
  await flushPromises();
  expect(wrapper.text()).toContain("The Lantern Keeper");
  expect(wrapper.get('[role="alert"]').text()).toContain("Engine offline");
  wrapper.unmount();
});

const menuItems = () => [...document.body.querySelectorAll<HTMLElement>('[role="menu"] [role="menuitem"]')];
async function cardAction(wrapper: ReturnType<typeof mount>, title: string, item: string) {
  await wrapper.get(`[aria-label="More for ${title}"]`).trigger("click");
  const match = menuItems().find((m) => m.textContent?.trim() === item);
  if (!match)
    throw new Error(
      `No "${item}" in ${menuItems()
        .map((m) => m.textContent?.trim())
        .join(", ")}`,
    );
  match.click();
  await flushPromises();
}
const toasts = () => useUiStore().toasts.map((t) => t.text);

it("prepares, pauses, resumes and cancels a whole book from its card menu", async () => {
  const prepare = vi.spyOn(mock, "prepareBook"),
    pause = vi.spyOn(mock, "setPreparePaused"),
    cancel = vi.spyOn(mock, "cancelPrepare");
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
  const exportBook = vi.spyOn(mock, "exportBook"),
    regenerate = vi.spyOn(mock, "regenerateBook");
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
  expect(menuItems().map((m) => m.textContent?.trim())).not.toContain("Delete generated audio");
  wrapper.unmount();
  host.unmount();
});

it("offers only removal for a book whose file is missing", async () => {
  const wrapper = await library();
  await wrapper.get('[aria-label="More for The Absent Orchard"]').trigger("click");
  expect(menuItems().map((m) => m.textContent?.trim())).toEqual(["Remove from library"]);
  wrapper.unmount();
});

async function answerPrompt(host: ReturnType<typeof mount>, text: string) {
  const dialog = host.get('[role="dialog"]');
  await dialog.get("input").setValue(text);
  await dialog.get("form").trigger("submit");
  await flushPromises();
}
/** The app on Library, with the sidebar's collections. */
async function app() {
  useNavigationStore().view = "library";
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}
const sidebar = (wrapper: ReturnType<typeof mount>) => wrapper.get('[aria-labelledby="sidebar-collections"]');
const shelf = (wrapper: ReturnType<typeof mount>, name: string) =>
  sidebar(wrapper)
    .findAll(".sidebar__collection")
    .find((b) => b.find(".sidebar__collection-name").text() === name)!;

it("creates a collection in the sidebar, puts a book on it from the card menu, and opens it", async () => {
  const create = vi.spyOn(mock, "createCollection"),
    shelve = vi.spyOn(mock, "setBookCollections");
  const wrapper = await app();
  expect(wrapper.find('[aria-label="Filter books"]').text()).not.toContain("collection");
  await sidebar(wrapper).get('[aria-label="New collection"]').trigger("click");
  await flushPromises();
  await answerPrompt(wrapper, "Night reading");
  expect(create).toHaveBeenCalledWith("Night reading");
  expect(shelf(wrapper, "Night reading").text()).toContain("0");

  await wrapper.get('[aria-label="More for The Lantern Keeper"]').trigger("click");
  const item = document.body.querySelector<HTMLElement>('[role="menuitemcheckbox"][aria-label="Night reading"]')!;
  expect(item.getAttribute("aria-checked")).toBe("false");
  item.click();
  await flushPromises();
  expect(shelve).toHaveBeenCalledWith("book-0", [expect.any(Number)]);
  expect(shelf(wrapper, "Night reading").text()).toContain("1");

  await shelf(wrapper, "Night reading").trigger("click");
  await flushPromises();
  expect(wrapper.get(".library__title").text()).toBe("Night reading");
  expect(titles(wrapper)).toEqual(["The Lantern Keeper"]);
  wrapper.unmount();
});

it("adds a book to a new collection straight from its card menu", async () => {
  const wrapper = await app();
  await wrapper.get('[aria-label="More for A Map of Quiet Places"]').trigger("click");
  [...document.body.querySelectorAll<HTMLElement>('[role="menuitem"]')]
    .find((m) => m.textContent?.trim() === "New collection…")!
    .click();
  await flushPromises();
  await answerPrompt(wrapper, "Maps");
  expect(shelf(wrapper, "Maps").text()).toContain("1");
  wrapper.unmount();
});

it("renames and deletes a collection from the sidebar", async () => {
  const rename = vi.spyOn(mock, "renameCollection"),
    remove = vi.spyOn(mock, "deleteCollection");
  const id = await mock.createCollection("Shelf");
  await mock.setBookCollections("book-1", [id]);
  const wrapper = await app();
  await shelf(wrapper, "Shelf").trigger("click");
  await flushPromises();
  await sidebar(wrapper).get('[aria-label="More for Shelf"]').trigger("click");
  await pickMenuItem("Rename…");
  expect((wrapper.get('[role="dialog"] input').element as HTMLInputElement).value).toBe("Shelf");
  await answerPrompt(wrapper, "Favourites");
  expect(rename).toHaveBeenCalledWith(id, "Favourites");
  expect(shelf(wrapper, "Favourites").attributes("aria-current")).toBe("page");

  await sidebar(wrapper).get('[aria-label="More for Favourites"]').trigger("click");
  await pickMenuItem("Delete…");
  expect(wrapper.get('[role="alertdialog"]').text()).toContain("The books stay in your library");
  await wrapper.get('[data-confirm="yes"]').trigger("click");
  await flushPromises();
  expect(remove).toHaveBeenCalledWith(id);
  expect(sidebar(wrapper).text()).not.toContain("Favourites");
  expect(wrapper.get(".library__title").text()).toBe("Library");
  expect(titles(wrapper)).toHaveLength(4);
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

it("marks a book finished and unread again from its card menu", async () => {
  const wrapper = await library();
  const chip = (name: string) => wrapper.get(`[role="group"][aria-label="Filter books"] [aria-label^="${name}"]`);
  const line = () =>
    wrapper
      .findAll(".book-card")
      .find((card) => card.text().includes("The Lantern Keeper"))!
      .get(".book-card__status")
      .text();
  await wrapper.get('[aria-label="More for The Lantern Keeper"]').trigger("click");
  await flushPromises();
  await pickMenuItem("Mark as finished");
  expect(line()).toBe("Text + voice · finished");
  await chip("Finished").trigger("click");
  expect(titles(wrapper)).toEqual(["The Lantern Keeper"]);

  await wrapper.get('[aria-label="More for The Lantern Keeper"]').trigger("click");
  await flushPromises();
  expect(openMenu()?.textContent).not.toContain("Mark as finished");
  await pickMenuItem("Mark as unread");
  expect(titles(wrapper)).toEqual([]);
  await chip("Unread").trigger("click");
  expect(titles(wrapper)).toContain("The Lantern Keeper");
  expect(line()).toBe("Text + voice · unread");
  wrapper.unmount();
});

it("switches between kinds once there are two, and shows an audiobook square with its audio", async () => {
  const wrapper = await library();
  expect(wrapper.find('[role="group"][aria-label="Kind"]').exists(), "books only").toBe(false);
  await mock.addAudiobook(["~/Audiobooks/Winter Radio"]);
  await flushPromises();
  const kinds = wrapper.get('[role="group"][aria-label="Kind"]');
  expect(kinds.findAll("button").map((b) => b.text())).toEqual(["All5", "Books4", "Audiobooks1"]);
  const card = wrapper.findAll(".book-card").find((c) => c.text().includes("Winter Radio"))!;
  expect(card.find(".cover-art--square").exists()).toBe(true);
  expect(card.get(".book-card__status").text()).toBe("Audio only · unplayed");

  await kinds.findAll("button")[2].trigger("click");
  expect(titles(wrapper)).toEqual(["Winter Radio"]);
  expect(kinds.findAll("button")[2].attributes("aria-pressed")).toBe("true");
  await kinds.findAll("button")[1].trigger("click");
  expect(titles(wrapper)).not.toContain("Winter Radio");
  expect(titles(wrapper)).toHaveLength(4);
  wrapper.unmount();
});

it("offers no text actions on an audiobook's card", async () => {
  await mock.addAudiobook(["~/Audiobooks/Winter Radio"]);
  const wrapper = await library();
  await wrapper.get('[aria-label="More for Winter Radio"]').trigger("click");
  await flushPromises();
  const labels = menuItems().map((item) => item.textContent?.trim() ?? "");
  expect(labels).toContain("Details");
  for (const text of ["Add audiobook…", "Export as M4B", "Prepare for listening"])
    expect(
      labels.some((l) => l.includes(text)),
      text,
    ).toBe(false);
  wrapper.unmount();
});

it("lists articles as their own kind, with a text tile and their fetch on the card", async () => {
  const card = (wrapper: ReturnType<typeof mount>) =>
    wrapper.findAll(".book-card").find((c) => c.find(".book-card__tile").exists())!;
  const cases: [string, string, string | null][] = [
    ["ready", "Article · unread", null],
    ["fetching", "Fetching…", null],
    ["needsyou-signin", "Needs you · sign in to get the rest", null],
    ["needsyou-cutshort", "Needs you · only part of it was saved", null],
    ["failed", "Could not be fetched", "The site answered with status 503."],
  ];
  for (const [stage, status, tooltip] of cases) {
    mock.dispose();
    setActivePinia(createPinia());
    mock = createMockApi(`?article=${stage}`);
    backend.api = mock;
    const wrapper = await library();
    const line = card(wrapper).get(".book-card__status");
    expect(line.text(), stage).toBe(status);
    expect(line.classes("book-card__status--warn"), stage).toBe(stage.startsWith("needsyou"));
    expect(line.attributes("title"), stage).toBe(tooltip ?? undefined);
    expect(card(wrapper).find(".book-card__tile--loading").exists(), stage).toBe(stage === "fetching");
    if (stage === "ready") {
      expect(card(wrapper).get(".book-card__tile").text()).toContain("Every evening at dusk");
      const kinds = wrapper.get('[role="group"][aria-label="Kind"]');
      expect(kinds.findAll("button").map((b) => b.text())).toEqual(["All5", "Books4", "Articles1"]);
      await kinds.findAll("button")[2].trigger("click");
      expect(titles(wrapper)).toEqual(["The Quay Lights"]);
    }
    wrapper.unmount();
  }
});

it("a refresh keeps the summaries that did not change, so only changed cards are drawn again", async () => {
  const library = useLibraryStore();
  await library.start();
  const before = library.books;
  await library.refresh();
  expect(library.books).toBe(before);

  const [changed, ...others] = before.map((b) => b.id);
  const kept = others.map((id) => library.books.find((b) => b.id === id));
  await library.markRead(changed, "finished");
  await library.refresh();
  expect(library.books.find((b) => b.id === changed)?.readState).toBe("finished");
  expect(others.map((id) => library.books.find((b) => b.id === id))).toEqual(kept);
  others.forEach((id, i) => expect(library.books.find((b) => b.id === id)).toBe(kept[i]));
});
