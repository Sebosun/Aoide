import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi, initialPlayer } from "../api/mock";
import ReaderView from "../views/ReaderView.vue";
import { useReaderStore } from "../stores/reader";
import { usePlayerStore } from "../stores/player";
import { useLibraryStore } from "../stores/library";
import { usePreferencesStore } from "../stores/preferences";
import { useNavigationStore } from "../stores/navigation";
import { pickMenuItem } from "./helpers";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("../api", () => ({ get api() { return backend.api; } }));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => { setActivePinia(createPinia()); mock = createMockApi(); backend.api = mock; });
afterEach(() => { mock.dispose(); document.body.innerHTML = ""; });

async function lines() {
  mock.emitConfig({ appearance: { readerMode: "lines" } });
  await usePreferencesStore().start();
  await useLibraryStore().start();
  await useReaderStore().open("book-0");
  await usePlayerStore().start();
  const wrapper = mount(ReaderView, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}

it("enlarges the spoken sentence, filling it word by word between its neighbours", async () => {
  const wrapper = await lines();
  expect(wrapper.find('[aria-label="Chapter text"]').exists()).toBe(false);
  mock.emitPlayer({ ...initialPlayer(), bookId: "book-0", status: "playing", segmentId: "0:1:0", wordIndex: 2 });
  await flushPromises();
  const now = wrapper.get(".reader-lines__line--now");
  expect(now.findAll(".reader-lines__word--done").map(w => w.text())).toEqual(["Every", "evening,"]);
  expect(now.get(".reader-lines__word--current").text()).toBe("Ada");
  expect(now.findAll(".reader-lines__word--ahead").length).toBeGreaterThan(5);
  const others = wrapper.findAll(".reader-lines__line:not(.reader-lines__line--now)").map(l => l.text());
  expect(others).toEqual(["The small light", "Tonight a paper boat waited beside the steps. Inside it lay a key, warm as if someone had held it all the way across the bay."]);
  wrapper.unmount();
});

it("plays a neighbouring sentence when it is clicked", async () => {
  const play = vi.spyOn(mock, "play");
  const wrapper = await lines();
  mock.emitPlayer({ ...initialPlayer(), bookId: "book-0", status: "playing", segmentId: "0:1:0", wordIndex: 0 });
  await flushPromises();
  await wrapper.get('[data-segment-id="0:2:0"]').trigger("click");
  expect(play).toHaveBeenCalledWith("book-0", 0, "0:2:0");
  wrapper.unmount();
});

it("shows the book on the left with a chapter picker", async () => {
  const play = vi.spyOn(mock, "play");
  const wrapper = await lines();
  const side = wrapper.get('[aria-label="About this book"]');
  expect(side.text()).toContain("The Lantern Keeper");
  expect(side.text()).toContain("Mira Vale");
  await side.get('[aria-haspopup="menu"]').trigger("click");
  expect(side.get('[aria-haspopup="menu"]').text()).toContain("Chapter 1 · The small light");
  await pickMenuItem("Chapter 2: Across the water");
  await flushPromises();
  expect(play).toHaveBeenCalledWith("book-0", 1, "1:0:0");
  wrapper.unmount();
});

it("has the full transport, and switches back to the page", async () => {
  const wrapper = await lines();
  const transport = wrapper.get('[aria-label="Player"]');
  for (const label of ["Previous chapter", "Play", "Next chapter", "Add bookmark", "Book tools", "Stop"]) expect(transport.find(`[aria-label="${label}"]`).exists(), label).toBe(true);
  expect(transport.find('[aria-label="Playback speed"][aria-haspopup="listbox"]').exists()).toBe(true);
  expect(transport.find('[aria-label="Sleep timer"][aria-haspopup="listbox"]').exists()).toBe(true);
  expect(transport.find('input[aria-label="Volume"]').exists()).toBe(true);
  expect(transport.find('[role="slider"]').exists()).toBe(true);
  await transport.get('[aria-label="Book tools"]').trigger("click");
  await flushPromises();
  expect(wrapper.find('[role="dialog"]').exists()).toBe(true);
  await wrapper.get('[role="dialog"]').trigger("keydown", { key: "Escape" });
  await wrapper.get('[aria-label="Page"]').trigger("click");
  await flushPromises();
  expect(wrapper.find('[aria-label="Chapter text"]').exists()).toBe(true);
  wrapper.unmount();
});

it("keeps each sentence's element as the playhead moves, so lines can glide to their new place", async () => {
  const wrapper = await lines();
  mock.emitPlayer({ ...initialPlayer(), bookId: "book-0", status: "playing", segmentId: "0:1:0", wordIndex: 0 });
  await flushPromises();
  const next = wrapper.get('[data-segment-id="0:2:0"]').element, spoken = wrapper.get('[data-segment-id="0:1:0"]').element;
  expect(spoken.classList.contains("reader-lines__line--now")).toBe(true);
  mock.emitPlayer({ ...initialPlayer(), bookId: "book-0", status: "playing", segmentId: "0:2:0", wordIndex: 1 });
  await flushPromises();
  expect(wrapper.get('[data-segment-id="0:2:0"]').element).toBe(next);
  expect(next.classList.contains("reader-lines__line--now")).toBe(true);
  expect(wrapper.get('[data-segment-id="0:1:0"]').element).toBe(spoken);
  expect(spoken.classList.contains("reader-lines__line--now")).toBe(false);
  expect(wrapper.get(".reader-lines__word--current").text()).toBe("a");
  wrapper.unmount();
});

it("goes to the book's page from the cover in lines mode", async () => {
  const wrapper = await lines();
  await wrapper.get('[aria-label="Book page for The Lantern Keeper"]').trigger("click");
  expect(useNavigationStore().view).toBe("book");
  wrapper.unmount();
});

it("prepares the chapter being read from the chapter menu", async () => {
  const prepare = vi.spyOn(mock, "prepareChapter");
  const wrapper = await lines();
  await wrapper.get('[aria-label^="Chapter 1 of"]').trigger("click");
  await pickMenuItem("Prepare this chapter");
  expect(prepare).toHaveBeenCalledWith("book-0", 0);
  wrapper.unmount();
});
