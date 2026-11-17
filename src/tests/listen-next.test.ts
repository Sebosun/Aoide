/** Listen next: items lined up by the player, from the library's card menus, an item's page and
 *  feed entries; reordered and taken off in the player's queue; the next named when what plays is
 *  nearly over. */
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import App from "@/App.vue";
import LibraryView from "@/views/LibraryView.vue";
import PlayerDock from "@/components/player/PlayerDock.vue";
import { useLibraryStore } from "@/stores/library";
import { usePlayerStore } from "@/stores/player";
import { pickMenuItem } from "@/tests/helpers";

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

async function libraryAndDock() {
  await useLibraryStore().start();
  await usePlayerStore().start();
  const library = mount(LibraryView, { attachTo: document.body });
  const dock = mount(PlayerDock, { attachTo: document.body });
  await flushPromises();
  return { library, dock };
}
const panel = () => document.body.querySelector<HTMLElement>('[role="dialog"][aria-label="Queue"]');
const queued = () => [...(panel()?.querySelectorAll(".queue-pill__title") ?? [])].map((t) => t.textContent?.trim());
async function click(label: string) {
  const button = document.body.querySelector<HTMLElement>(`[aria-label="${label}"]`);
  if (!button) throw new Error(`No button "${label}"`);
  button.click();
  await flushPromises();
}

it("queues items from their card menu and lists them by the player, to reorder and take off", async () => {
  const { library, dock } = await libraryAndDock();
  const menu = async (title: string, item: string) => {
    await library.get(`[aria-label="More for ${title}"]`).trigger("click");
    await pickMenuItem(item);
  };
  await menu("An Unfinished Atlas", "Add to queue");
  await menu("A Map of Quiet Places", "Listen next");
  const pill = dock.get('[aria-label="Queue"]');
  expect(pill.text()).toBe("2");
  await pill.trigger("click");
  await flushPromises();
  expect(queued()).toEqual(["A Map of Quiet Places", "An Unfinished Atlas"]);

  await click("Move An Unfinished Atlas up");
  expect(queued()).toEqual(["An Unfinished Atlas", "A Map of Quiet Places"]);
  await click("Take An Unfinished Atlas off the queue");
  expect(queued()).toEqual(["A Map of Quiet Places"]);
  expect(pill.text()).toBe("1");

  // The card's menu knows it is queued.
  await library.get('[aria-label="More for A Map of Quiet Places"]').trigger("click");
  await pickMenuItem("Remove from queue");
  expect(queued()).toEqual([]);
  expect(panel()?.textContent).toContain("Nothing is queued");
  library.unmount();
  dock.unmount();
});

it("names the item up next under the time left while what plays is nearly over", async () => {
  const { library, dock } = await libraryAndDock();
  await mock.queueAdd({ bookId: "book-1", place: "last" });
  // The core's times left (player.json): its last sentence leaves under two minutes.
  const chapters = await mock.bookOverview({ bookId: "book-0" });
  const last = chapters.chapters.length - 1;
  const text = await mock.chapter({ bookId: "book-0", chapter: last });
  const sentence = text.blocks
    .flatMap((b) => b.segments)
    .filter((s) => s.speakable)
    .at(-1)!;
  await mock.play({ bookId: "book-0", chapter: last, segmentId: sentence.id });
  await mock.pause();
  await flushPromises();
  expect(dock.get(".dock__status").text()).toContain("Then: A Map of Quiet Places");
  library.unmount();
  dock.unmount();
});

it("queues a feed entry without marking it read", async () => {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  await wrapper.get('nav[aria-label="Main"] [aria-label="Feeds"]').trigger("click");
  await flushPromises();
  const entry = () =>
    wrapper
      .findAll(".feed-entry")
      .find((e) => e.find(".feed-entry__title").text() === "The quiet return of the night train")!;
  await entry().get('[aria-label="Queue The quiet return of the night train"]').trigger("click");
  await pickMenuItem("Add to queue");
  expect(entry().find('[aria-label="Unread"]').exists()).toBe(true);
  expect((await mock.queue()).items).toHaveLength(1);
  wrapper.unmount();
});

it("shows a queued article's reason gone once it has been fetched", async () => {
  vi.useFakeTimers();
  try {
    const { library, dock } = await libraryAndDock();
    const id = await mock.saveArticle({ url: "https://quay.example/tides" });
    await mock.queueAdd({ bookId: id, place: "last" });
    await dock.get('[aria-label="Queue"]').trigger("click");
    await flushPromises();
    expect(panel()?.querySelector(".queue-pill__why")?.textContent).toContain("Still being fetched");

    await vi.advanceTimersByTimeAsync(2_000);
    await flushPromises();
    expect(panel()?.querySelector(".queue-pill__why")).toBeNull();
    library.unmount();
    dock.unmount();
  } finally {
    vi.useRealTimers();
  }
});
