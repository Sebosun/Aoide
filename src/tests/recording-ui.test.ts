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

it("switches narration only once the recording is synced", async () => {
  await useLibraryStore().start();
  mock.setRecordingForTest("book-0", "transcribing");
  const wrapper = mount(BookNarration, { props: { bookId: "book-0" }, attachTo: document.body });
  await flushPromises();
  const recording = wrapper.get('[role="radio"]:first-child');
  expect(recording.attributes("disabled")).toBeDefined();
  expect(wrapper.get('[role="radio"][aria-checked="true"]').text()).toBe("Speech engine");
  expect(wrapper.text()).toContain("Syncing");
  expect(wrapper.find('[aria-label="Pause syncing"]').exists()).toBe(true);

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
