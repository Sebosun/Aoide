import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "../api/mock";
import BookDetailsView from "../views/BookDetailsView.vue";
import { describePreparation, preparationStatus, runPrepareAction } from "../preparation";
import { useUiStore } from "../stores/ui";
import { useLibraryStore } from "../stores/library";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("../api", () => ({ get api() { return backend.api; } }));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => { setActivePinia(createPinia()); mock = createMockApi(); backend.api = mock; });
afterEach(() => { mock.dispose(); document.body.innerHTML = ""; });

const at = (state: "waiting" | "queued" | "preparing" | "paused" | "done", done = 3, total = 120, chapters: number[] | null = null) => ({ state, done, total, position: 0, chapters });

it("words each stage and offers only the actions that apply to it", () => {
  expect(preparationStatus(null)).toEqual({ state: null, label: null, scope: null, count: null, percent: 0, actions: ["prepare"] });
  expect(preparationStatus(at("waiting", 0, 0))).toMatchObject({ label: "Waiting to start", count: null, actions: ["start", "cancel"] });
  expect(preparationStatus(at("queued"))).toMatchObject({ label: "Queued", count: "3 / 120", percent: 2, actions: ["pause", "cancel"] });
  expect(preparationStatus(at("preparing", 60))).toMatchObject({ label: "Preparing", count: "60 / 120", percent: 50, actions: ["pause", "cancel"] });
  expect(preparationStatus(at("paused"))).toMatchObject({ label: "Paused", actions: ["resume", "cancel"] });
  expect(preparationStatus(at("done", 120))).toMatchObject({ label: "Ready offline", count: null, percent: 100, actions: [] });
});

it("runs each action and says so once when a book starts preparing", async () => {
  const prepare = vi.spyOn(mock, "prepareBook"), pause = vi.spyOn(mock, "setPreparePaused"), cancel = vi.spyOn(mock, "cancelPrepare");
  await runPrepareAction("book-1", "prepare");
  await runPrepareAction("book-1", "pause");
  await runPrepareAction("book-1", "resume");
  await runPrepareAction("book-1", "start");
  await runPrepareAction("book-1", "cancel");
  expect(prepare.mock.calls).toEqual([["book-1"], ["book-1"]]);
  expect(pause.mock.calls).toEqual([["book-1", true], ["book-1", false]]);
  expect(cancel).toHaveBeenCalledWith("book-1");
  expect(useUiStore().toasts.map(t => t.text)).toEqual(["Preparing the whole book in the background"]);
});

it("reports a failed action as an error message", async () => {
  vi.spyOn(mock, "prepareBook").mockRejectedValue(new Error("engine offline"));
  await runPrepareAction("book-1", "prepare");
  expect(useUiStore().toasts.map(t => [t.text, t.tone])).toEqual([["Error: engine offline", "error"]]);
});

it("shows a queued book as queued on its page, not as preparing", async () => {
  await mock.prepareBook("book-1");
  await mock.prepareBook("book-0");
  await useLibraryStore().start(); // App.vue starts it.
  const wrapper = mount(BookDetailsView, { props: { bookId: "book-0" }, attachTo: document.body });
  await flushPromises();
  expect(wrapper.text()).toContain("Queued · 3 / 120");
  expect(wrapper.text()).not.toContain("Preparing");
  expect(wrapper.find('[aria-label="Prepare whole book"]').exists()).toBe(false);
  wrapper.unmount();
});

it("names the chapters of a chapter-only preparation and still offers the whole book", () => {
  const one = preparationStatus(at("preparing", 4, 12, [2]));
  expect(one).toMatchObject({ label: "Preparing", scope: "Chapter 3", actions: ["prepare", "pause", "cancel"] });
  expect(describePreparation(one)).toBe("Preparing · Chapter 3 · 4 / 12");
  expect(preparationStatus(at("queued", 0, 20, [1, 4])).scope).toBe("Chapters 2 and 5");
  expect(preparationStatus(at("queued", 0, 20, [0, 1, 4])).scope).toBe("Chapters 1, 2 and 5");
  expect(preparationStatus(at("queued")).scope).toBe(null);
});

it("prepares one chapter from its row in the contents and shows each chapter's audio", async () => {
  const prepare = vi.spyOn(mock, "prepareChapter");
  await useLibraryStore().start();
  const wrapper = mount(BookDetailsView, { props: { bookId: "book-0" }, attachTo: document.body });
  await flushPromises();
  const rows = wrapper.findAll('[aria-label="Contents"] li');
  expect(rows[0].text()).toContain("Ready");
  expect(rows[0].find('[aria-label^="Prepare chapter"]').exists()).toBe(false);
  await rows[1].get('[aria-label="Prepare chapter 2: Across the water"]').trigger("click");
  await flushPromises();
  expect(prepare).toHaveBeenCalledWith("book-0", 1);
  expect(useUiStore().toasts.at(-1)?.text).toBe("Preparing chapter 2 in the background");
  expect(wrapper.findAll('[aria-label="Contents"] li')[1].text()).toMatch(/Queued|Preparing/);
  wrapper.unmount();
});
