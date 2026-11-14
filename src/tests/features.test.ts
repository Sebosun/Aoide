import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi, initialPlayer } from "../api/mock";
import SettingsDialog from "../components/settings/SettingsDialog.vue";
import StatsView from "../views/StatsView.vue";
import BookToolsSheet from "../components/book/BookToolsSheet.vue";
import BookVoice from "../components/book/BookVoice.vue";
import UiHost from "../components/ui/UiHost.vue";
import LibraryView from "../views/LibraryView.vue";
import App from "../App.vue";
import { useReaderStore } from "../stores/reader";
import { usePlayerStore } from "../stores/player";
import { useLibraryStore } from "../stores/library";
import { useNavigationStore } from "../stores/navigation";

import { choose } from "./helpers";
const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("../api", () => ({ get api() { return backend.api; } }));
let mock: ReturnType<typeof createMockApi>;
async function settings(tab: string) {
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  await wrapper.findAll('[role="tab"]').find(t => t.text() === tab)!.trigger("click");
  await flushPromises();
  return wrapper;
}
const submit = (wrapper: ReturnType<typeof mount>, label: string) => wrapper.findAll('button[type="submit"]').find(b => b.text() === label)!.trigger("click");
beforeEach(() => { setActivePinia(createPinia()); mock = createMockApi(); backend.api = mock; });
afterEach(() => { mock.dispose(); });

it("draws engine settings from the adapter descriptor and saves the profile", async () => {
  const save = vi.spyOn(mock, "saveEngineProfile");
  const wrapper = await settings("Speech engine");
  expect(wrapper.get('[aria-label="Temperature"]').element).toBeInstanceOf(HTMLInputElement);
  await choose(wrapper, "Engine type", "OpenAI-compatible speech server");
  await flushPromises();
  expect(wrapper.find('[aria-label="Temperature"]').exists()).toBe(false);
  await wrapper.get('[aria-label="Speed"]').setValue("1.5");
  await wrapper.get('[aria-label="Server address"]').setValue("http://127.0.0.1:8880");
  await wrapper.get('[aria-label="Model"]').setValue("kokoro");
  await submit(wrapper, "Save engine");
  await flushPromises();
  expect(save).toHaveBeenCalledWith(expect.objectContaining({ kind: "openai-speech", baseUrl: "http://127.0.0.1:8880", model: "kokoro", params: { speed: 1.5 } }));
  expect(wrapper.text()).toContain("Saved");
  wrapper.unmount();
});

it("warns before sending book text over the home network and shows save errors", async () => {
  const wrapper = await settings("Speech engine");
  expect(wrapper.text()).not.toContain("Your book text will be sent");
  await wrapper.get('[aria-label="Allow home-network addresses"]').setValue(true);
  expect(wrapper.text()).toContain("Your book text will be sent over your home network");
  await wrapper.get('[aria-label="Allow home-network addresses"]').setValue(false);
  await wrapper.get('[aria-label="Server address"]').setValue("http://192.168.1.4:8004");
  await submit(wrapper, "Save engine");
  await flushPromises();
  expect(wrapper.get('[role="alert"]').text()).toContain("loopback");
  wrapper.unmount();
});

it("saves metadata lookup from Library and the audio cache limit from Storage, keeping other settings", async () => {
  const save = vi.spyOn(mock, "saveAppSettings");
  const wrapper = await settings("Library");
  await wrapper.get('[aria-label="Look up book details online"]').setValue(false);
  await flushPromises();
  expect(save).toHaveBeenLastCalledWith({ metadataLookup: false, cacheLimitBytes: null, aligner: null, librarySort: "recent", scan: { roots: [], exclude: [], includeHidden: [], onStartup: false }, resumePrepare: true, recognition: { engine: "builtIn", server: { baseUrl: "http://127.0.0.1:8000", model: "", network: "loopback" }, modelSize: "base", device: "auto", threads: 0, resumeOnStart: true } });
  await wrapper.findAll('[role="tab"]').find(t => t.text() === "Storage")!.trigger("click");
  await flushPromises();
  await wrapper.get('[aria-label="Audio cache limit in GB"]').setValue("2");
  await submit(wrapper, "Save limit");
  await flushPromises();
  expect(save).toHaveBeenLastCalledWith({ metadataLookup: false, cacheLimitBytes: 2 * 1024 ** 3, aligner: null, librarySort: "recent", scan: { roots: [], exclude: [], includeHidden: [], onStartup: false }, resumePrepare: true, recognition: { engine: "builtIn", server: { baseUrl: "http://127.0.0.1:8000", model: "", network: "loopback" }, modelSize: "base", device: "auto", threads: 0, resumeOnStart: true } });
  wrapper.unmount();
});

it("turns on exact word timing with an aligner address and launch command", async () => {
  const save = vi.spyOn(mock, "saveAppSettings");
  const wrapper = await settings("Word highlighting");
  expect(wrapper.find('[aria-label="Aligner address"]').exists()).toBe(false);
  await wrapper.get('[aria-label="Exact word timing"]').setValue(true);
  await wrapper.get('[aria-label="Aligner address"]').setValue("http://127.0.0.1:8890");
  await wrapper.get('[aria-label="Aligner launch program"]').setValue("/venv/bin/python");
  await wrapper.get('[aria-label="Aligner launch arguments"]').setValue("aoide_aligner.py\n--port\n8890");
  await submit(wrapper, "Save");
  await flushPromises();
  expect(save).toHaveBeenCalledWith(expect.objectContaining({
    aligner: { baseUrl: "http://127.0.0.1:8890", network: "loopback", launch: { program: "/venv/bin/python", args: ["aoide_aligner.py", "--port", "8890"], workingDir: null } },
  }));
  wrapper.unmount();
});

it("searches the open book and plays a hit", async () => {
  await useReaderStore().open("book-0");
  const play = vi.spyOn(mock, "play");
  const wrapper = mount(BookToolsSheet);
  await wrapper.get('[role="tab"][aria-controls="book-tools-search"]').trigger("click");
  await wrapper.get('[aria-label="Search in book"]').setValue("lantern");
  await wrapper.get(".book-search__form").trigger("submit");
  await flushPromises();
  const hit = wrapper.get(".book-search__hit");
  expect(hit.get("mark").text().toLowerCase()).toBe("lantern");
  await hit.trigger("click");
  expect(play).toHaveBeenCalledWith("book-0", 0, "0:1:0");
  wrapper.unmount();
});

it("adds, annotates, plays and deletes bookmarks", async () => {
  await useReaderStore().open("book-0");
  const play = vi.spyOn(mock, "play");
  const wrapper = mount(BookToolsSheet);
  await wrapper.get('[role="tab"][aria-controls="book-tools-bookmarks"]').trigger("click");
  await wrapper.get('[aria-label="Bookmark note"]').setValue("Lovely line");
  await wrapper.get(".book-bookmarks__add").trigger("submit");
  await flushPromises();
  expect(wrapper.findAll(".book-bookmarks__item")).toHaveLength(1);
  expect(wrapper.text()).toContain("Lovely line");
  await wrapper.get(".book-bookmarks__play").trigger("click");
  expect(play).toHaveBeenCalledWith("book-0", 0, "0:1:0");
  await wrapper.get('[aria-label="Delete bookmark"]').trigger("click");
  await flushPromises();
  expect(wrapper.findAll(".book-bookmarks__item")).toHaveLength(0);
  wrapper.unmount();
});

it("saves per-book voice settings and queues whole-book preparation", async () => {
  await useReaderStore().open("book-0");
  const saveSettings = vi.spyOn(mock, "saveBookSettings");
  const prepare = vi.spyOn(mock, "prepareBook");
  const wrapper = mount(BookToolsSheet);
  await wrapper.get('[role="tab"][aria-controls="book-tools-voice"]').trigger("click");
  await flushPromises();
  await choose(wrapper, "Voice for this book", "Adrian");
  await wrapper.get('[aria-label="Read footnotes aloud"]').setValue(true);
  await wrapper.get(".book-voice__form").trigger("submit");
  await flushPromises();
  expect(saveSettings).toHaveBeenCalledWith("book-0", expect.objectContaining({ voiceId: "Adrian.wav", readNotes: true }));
  await wrapper.get('[aria-label="Prepare whole book"]').trigger("click");
  expect(prepare).toHaveBeenCalledWith("book-0");
  wrapper.unmount();
});

it("asks before regenerating a book's audio", async () => {
  await useReaderStore().open("book-0");
  const regenerate = vi.spyOn(mock, "regenerateBook");
  const wrapper = mount(BookToolsSheet), host = mount(UiHost, { attachTo: document.body });
  await wrapper.get('[role="tab"][aria-controls="book-tools-voice"]').trigger("click");
  await flushPromises();
  await wrapper.get('[aria-label="Regenerate audio"]').trigger("click");
  await flushPromises();
  expect(regenerate).not.toHaveBeenCalled();
  expect(host.get('[role="alertdialog"]').text()).toContain("generate it again with the current voice");
  await host.get('[data-confirm="yes"]').trigger("click");
  await flushPromises();
  expect(regenerate).toHaveBeenCalledWith("book-0");
  wrapper.unmount();
  host.unmount();
});

it("looks up online details and applies the picked match", async () => {
  await useReaderStore().open("book-0");
  const apply = vi.spyOn(mock, "applyMetadata");
  const wrapper = mount(BookToolsSheet);
  await wrapper.get('[role="tab"][aria-controls="book-tools-details"]').trigger("click");
  await flushPromises();
  expect(wrapper.text()).toContain("An original story");
  await wrapper.get('[aria-label="Find details online"]').trigger("click");
  await flushPromises();
  await wrapper.findAll("button").find(b => b.text() === "Use this")!.trigger("click");
  await flushPromises();
  expect(apply).toHaveBeenCalledWith("book-0", expect.objectContaining({ key: "/works/OL0001W" }));
  expect(wrapper.text()).toContain("Preview Press");
  wrapper.unmount();
});

it("exports the book and reports where the file went", async () => {
  await useReaderStore().open("book-0");
  const wrapper = mount(BookToolsSheet);
  await wrapper.get('[role="tab"][aria-controls="book-tools-voice"]').trigger("click");
  await flushPromises();
  await wrapper.get('[aria-label="Export audiobook"]').trigger("click");
  await flushPromises();
  expect(wrapper.text()).toContain("The Lantern Keeper.m4b");
  wrapper.unmount();
});

it("shows listening for a chosen range with a streak, a calendar, weekdays and books", async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date("2026-11-11T12:00:00"));
  await useLibraryStore().start();
  const wrapper = mount(StatsView, { attachTo: document.body });
  await flushPromises();
  const tile = (label: string) => wrapper.get(`[aria-label="${label}"]`).text();
  expect(tile("Listened")).toContain("1 h 50 min");
  expect(tile("Daily average")).toContain("16 min");
  expect(tile("Streak")).toContain("3 days");
  expect(tile("Streak")).toContain("best 3");
  expect(wrapper.findAll('[aria-label="Listening by day"] li')).toHaveLength(7);
  const range = (name: string) => wrapper.findAll('[role="group"][aria-label="Range"] button').find(b => b.text() === name)!;
  await range("30 days").trigger("click");
  expect(range("30 days").attributes("aria-pressed")).toBe("true");
  expect(wrapper.findAll('[aria-label="Listening by day"] li')).toHaveLength(30);
  await range("12 months").trigger("click");
  expect(wrapper.findAll('[aria-label="Listening by month"] li')).toHaveLength(12);
  expect(wrapper.findAll('[aria-label="Last 26 weeks"] [data-day]').length).toBeGreaterThan(170);
  expect(wrapper.get('[aria-label="By weekday"]').text()).toContain("Most on Mondays");
  const books = wrapper.findAll('[aria-label="By book"] li');
  expect(books[0].text()).toContain("The Lantern Keeper");
  expect(books[0].text()).toContain("1 h 30 min");
  await books[1].get("button").trigger("click");
  expect(useNavigationStore().view).toBe("book");
  expect(useNavigationStore().bookId).toBe("book-1");
  wrapper.unmount();
  vi.useRealTimers();
});

it("pauses and resumes whole-book preparation", async () => {
  await useReaderStore().open("book-0");
  await useLibraryStore().start();
  await mock.prepareBook("book-0");
  await flushPromises();
  const paused = vi.spyOn(mock, "setPreparePaused");
  const wrapper = mount(BookToolsSheet);
  await wrapper.get('[role="tab"][aria-controls="book-tools-voice"]').trigger("click");
  await flushPromises();
  await wrapper.get('[aria-label="Pause preparing"]').trigger("click");
  await flushPromises();
  expect(paused).toHaveBeenCalledWith("book-0", true);
  expect(wrapper.text()).toContain("Paused");
  await wrapper.get('[aria-label="Resume preparing"]').trigger("click");
  expect(paused).toHaveBeenCalledWith("book-0", false);
  wrapper.unmount();
});

it("reuses cached lookups unless asked to search again", async () => {
  await useReaderStore().open("book-0");
  const lookup = vi.spyOn(mock, "lookupMetadata");
  const wrapper = mount(BookToolsSheet);
  await wrapper.get('[role="tab"][aria-controls="book-tools-details"]').trigger("click");
  await flushPromises();
  await wrapper.get('[aria-label="Find details online"]').trigger("click");
  await flushPromises();
  expect(lookup).toHaveBeenLastCalledWith("book-0", false);
  await wrapper.get('[aria-label="Search again"]').trigger("click");
  await flushPromises();
  expect(lookup).toHaveBeenLastCalledWith("book-0", true);
  wrapper.unmount();
});

it("turns off continuing preparation at startup from Library, keeping other settings", async () => {
  const save = vi.spyOn(mock, "saveAppSettings");
  const wrapper = await settings("Library");
  const box = wrapper.get<HTMLInputElement>('[aria-label="Continue preparing books when Aoide starts"]');
  expect(box.element.checked).toBe(true);
  await box.setValue(false);
  await flushPromises();
  expect(save).toHaveBeenLastCalledWith(expect.objectContaining({ resumePrepare: false, metadataLookup: true }));
  expect((await mock.appSettings()).resumePrepare).toBe(false);
  wrapper.unmount();
});

it("previews a book's voice in that book's language, not the open book's", async () => {
  await useReaderStore().open("book-0"); // An English book is open.
  const library = useLibraryStore();
  await library.start();
  library.books.find(b => b.id === "book-1")!.language = "pt";
  const preview = vi.spyOn(mock, "previewVoice");
  const wrapper = mount(BookVoice, { props: { bookId: "book-1" }, attachTo: document.body });
  await flushPromises();
  await wrapper.get('[aria-label="Preview voice"]').trigger("click");
  await flushPromises();
  expect(preview).toHaveBeenCalledWith(expect.anything(), null, expect.anything(), "pt");
  wrapper.unmount();
});
