import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "../api/mock";
import App from "../App.vue";
import ReaderView from "../views/ReaderView.vue";
import BookNarration from "../components/book/BookNarration.vue";
import { useLibraryStore } from "../stores/library";
import { useNavigationStore } from "../stores/navigation";
import { usePlayerStore } from "../stores/player";
import { useReaderStore } from "../stores/reader";
import { useRecordingsStore } from "../stores/recordings";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("../api", () => ({ get api() { return backend.api; } }));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => { setActivePinia(createPinia()); mock = createMockApi(); backend.api = mock; });
afterEach(() => { mock.dispose(); document.body.innerHTML = ""; });

const dialog = (label: string) => document.body.querySelector<HTMLElement>(`[role="dialog"][aria-label="${label}"]`);
const click = async (element: HTMLElement | null | undefined) => { element!.click(); await flushPromises(); };
const byText = (root: ParentNode, selector: string, text: string) => [...root.querySelectorAll<HTMLElement>(selector)].find(e => e.textContent?.includes(text));

it("pairs dropped audio with the suggested book and shows its sync on the book's page", async () => {
  mount(App, { attachTo: document.body });
  await flushPromises();
  useRecordingsStore().openPairing(["~/Audiobooks/the-lantern-keeper.m4b"]);
  await flushPromises();
  const pair = dialog("Pair an audiobook")!;
  expect(pair.textContent).toContain("1 file");
  expect(pair.textContent).toContain("tagged “The Lantern Keeper”");
  expect(pair.querySelector('[aria-label="The book it reads"]')?.textContent).toContain("The Lantern Keeper");

  await click(byText(pair, "button", "Check and sync"));
  expect(dialog("Pair an audiobook")).toBeNull();
  expect(useNavigationStore().view).toBe("book");
  const card = document.body.querySelector<HTMLElement>('[aria-label="Recording"]')!;
  expect(card.textContent).toMatch(/Checking the audio|Syncing/);
});

it("refuses DRM-protected files and says why", async () => {
  mount(App, { attachTo: document.body });
  await flushPromises();
  useRecordingsStore().openPairing(["~/Audiobooks/lantern.aax"], "book-0");
  await flushPromises();
  const pair = dialog("Pair an audiobook")!;
  expect(pair.querySelector('[aria-label="Files that cannot be used"]')?.textContent).toContain("Protected by Audible DRM");
  expect(byText(pair, "button", "Check and sync")!.hasAttribute("disabled")).toBe(true);
});

it("switches narration once part of the recording is synced", async () => {
  await useLibraryStore().start();
  mock.setRecordingForTest("book-0", "checking");
  const wrapper = mount(BookNarration, { props: { bookId: "book-0" }, attachTo: document.body });
  await flushPromises();
  expect(wrapper.get('[role="radio"]:first-child').attributes("disabled")).toBeDefined();
  expect(wrapper.get('[role="radio"][aria-checked="true"]').text()).toBe("Speech engine");

  mock.setRecordingForTest("book-0", "transcribing");
  await flushPromises();
  expect(wrapper.text()).toContain("Syncing");
  expect(wrapper.find('[aria-label="Pause syncing"]').exists()).toBe(true);
  expect(wrapper.get('[role="radio"]:first-child').attributes("disabled"), "what is synced can be listened to").toBeUndefined();

  mock.setRecordingForTest("book-0", "ready", "speech");
  await flushPromises();
  await wrapper.get('[role="radio"]:first-child').trigger("click");
  await flushPromises();
  expect((await mock.bookSettings("book-0")).narration).toBe("recording");
  expect(wrapper.get('[role="radio"][aria-checked="true"]').text()).toBe("Recording");
  expect(wrapper.text()).toContain("The narrator's recording plays for this book");
});

it("lists recordings being synced in the queue with their pause button", async () => {
  mount(App, { attachTo: document.body });
  await flushPromises();
  mock.setRecordingForTest("book-1", "transcribing");
  await flushPromises();
  await click(document.body.querySelector<HTMLElement>("header .engine-pill"));
  const rows = [...document.body.querySelectorAll<HTMLElement>('[aria-label="Recordings being synced"] > li')];
  expect(rows.map(r => r.textContent)).toEqual([expect.stringContaining("A Map of Quiet Places")]);
  expect(rows[0].textContent).toContain("about 14 min left");
  const pause = vi.spyOn(mock, "syncAction");
  await click(rows[0].querySelector<HTMLElement>('[aria-label="Pause syncing A Map of Quiet Places"]'));
  expect(pause).toHaveBeenCalledWith("book-1", "pause");
});

it("reports chapters and narration missing from the text, each playable", async () => {
  mount(App, { attachTo: document.body });
  await flushPromises();
  mock.setRecordingForTest("book-0", "ready");
  useRecordingsStore().openReport("book-0");
  await flushPromises();
  const report = dialog("Sync report")!;
  expect(report.textContent).toContain("94%");
  expect(report.textContent).toContain("This is a recording of The Lantern Keeper");
  const play = vi.spyOn(mock, "playRecordingAt");
  await click(report.querySelector<HTMLElement>('[aria-label="Play the stretch at 0:00:00"]'));
  expect(play).toHaveBeenCalledWith("book-0", 0);
});

it("says when the narration playing is not in the text", async () => {
  await useLibraryStore().start();
  await useReaderStore().open("book-0");
  const player = usePlayerStore();
  await player.start();
  const wrapper = mount(ReaderView, { attachTo: document.body });
  await flushPromises();
  mock.setRecordingForTest("book-0", "ready");
  await player.play("book-0", 0, null);
  await flushPromises();
  expect(wrapper.get('[aria-label="Player"]').text()).toContain("Recording");
  expect(wrapper.find(".reader__chip").exists()).toBe(false);
  mock.emitPlayer({ ...player.state, offText: true });
  await flushPromises();
  expect(wrapper.get(".reader__chip").text()).toBe("Not in the text");
});

it("shows the exact time left in the chapter while a recording plays", async () => {
  await useLibraryStore().start();
  await useReaderStore().open("book-0");
  const player = usePlayerStore();
  await player.start();
  const wrapper = mount(ReaderView, { attachTo: document.body });
  await flushPromises();
  mock.setRecordingForTest("book-0", "ready");
  await player.play("book-0", 0, null);
  await flushPromises();
  mock.emitPlayer({ ...player.state, chapterLeftMs: 47 * 60_000 });
  await flushPromises();
  expect(wrapper.get('[aria-label="Player"]').text()).toContain("47 min left");
});

it("marks the sentences the recording lacks, and says so when it lacks the whole chapter", async () => {
  await useLibraryStore().start();
  const reader = useReaderStore();
  await reader.open("book-0");
  await useRecordingsStore().start();
  const wrapper = mount(ReaderView, { attachTo: document.body });
  await flushPromises();
  expect(wrapper.find(".chapter-text__segment--unnarrated").exists()).toBe(false);
  mock.setRecordingForTest("book-0", "ready");
  await flushPromises();
  await reader.loadChapter(1);
  await flushPromises();
  const segments = wrapper.findAll(".chapter-text__segment");
  expect(segments.length).toBeGreaterThan(0);
  expect(segments.every(s => s.classes("chapter-text__segment--unnarrated"))).toBe(true);
  expect(wrapper.text()).toContain("This chapter is not in the recording");
});

it("tells a book's listening progress apart from its recording's sync on the card", async () => {
  mock.setRecordingForTest("book-1", "transcribing");
  mock.setRecordingForTest("book-0", "ready");
  await useLibraryStore().start();
  await flushPromises();
  const { default: LibraryView } = await import("../views/LibraryView.vue");
  const wrapper = mount(LibraryView, { attachTo: document.body });
  await flushPromises();
  const card = (title: string) => wrapper.findAll(".book-card").find(c => c.text().includes(title))!.findAll(".book-card__line").at(-1)!.text();
  expect(card("A Map of Quiet Places")).toBe("Syncing recording");
  expect(card("The Lantern Keeper")).toBe("12% · narrated recording");
  wrapper.unmount();
});

it("starts listening while the recording syncs, and says when playback waits for the sync", async () => {
  await useLibraryStore().start();
  await useReaderStore().open("book-0");
  const player = usePlayerStore();
  await player.start();
  mock.setRecordingForTest("book-0", "transcribing");
  const reader = mount(ReaderView, { attachTo: document.body });
  const panel = mount(BookNarration, { props: { bookId: "book-0" }, attachTo: document.body });
  await flushPromises();
  await panel.get('[aria-label="Listen now"]').trigger("click");
  await flushPromises();
  expect((await mock.bookSettings("book-0")).narration).toBe("recording");
  expect(player.state.bookId).toBe("book-0");
  expect(player.state.source).toBe("recording");
  mock.emitPlayer({ ...player.state, status: "buffering", waitingForSync: true });
  await flushPromises();
  expect(reader.get('[aria-label="Player"]').text()).toContain("Waiting for the sync to reach here");
  panel.unmount();
  reader.unmount();
});

it("lets files be put in order by hand before pairing", async () => {
  const attach = vi.spyOn(mock, "attachRecording");
  mount(App, { attachTo: document.body });
  await flushPromises();
  const paths = ["~/Audiobooks/lantern/part-a.mp3", "~/Audiobooks/lantern/part-b.mp3"];
  useRecordingsStore().openPairing(paths, "book-0");
  await flushPromises();
  const pair = dialog("Pair an audiobook")!;
  const order = () => [...pair.querySelectorAll('[aria-label="Audio files in order"] li')].map(li => li.querySelector(".pair__name")?.textContent);
  expect(order()).toEqual(["part-a.mp3", "part-b.mp3"]);
  expect(pair.querySelector<HTMLButtonElement>('[aria-label="Move part-a.mp3 earlier"]')!.disabled).toBe(true);
  await click(pair.querySelector<HTMLElement>('[aria-label="Move part-b.mp3 earlier"]'));
  expect(order()).toEqual(["part-b.mp3", "part-a.mp3"]);
  await click(byText(pair, "button", "Check and sync"));
  expect(attach).toHaveBeenCalledWith("book-0", [paths[1], paths[0]], true);
});

it("exports a synced book as a read-along EPUB from its card and its recording panel", async () => {
  const exportReadAlong = vi.spyOn(mock, "exportReadAlong");
  mock.setRecordingForTest("book-0", "ready");
  await useLibraryStore().start();
  await flushPromises();
  const { default: LibraryView } = await import("../views/LibraryView.vue");
  const { default: UiHost } = await import("../components/ui/UiHost.vue");
  const host = mount(UiHost, { attachTo: document.body });
  const library = mount(LibraryView, { attachTo: document.body });
  await flushPromises();
  await library.get('[aria-label="More for The Lantern Keeper"]').trigger("click");
  const labels = () => [...document.body.querySelectorAll<HTMLElement>('[role="menuitem"]')];
  expect(labels().map(m => m.textContent?.trim())).not.toContain("Export as M4B");
  await click(labels().find(m => m.textContent?.includes("Export read-along EPUB")));
  expect(exportReadAlong).toHaveBeenCalledWith("book-0");
  expect(host.text()).toContain("Exported to ~/Documents/Aoide/Exports/The Lantern Keeper (read-along).epub");
  library.unmount();

  const panel = mount(BookNarration, { props: { bookId: "book-0" }, attachTo: document.body });
  await flushPromises();
  await panel.get('[aria-label="Export read-along EPUB"]').trigger("click");
  await flushPromises();
  expect(exportReadAlong).toHaveBeenCalledTimes(2);
  panel.unmount();
  host.unmount();
});
