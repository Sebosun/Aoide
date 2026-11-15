import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "../api/mock";
import App from "../App.vue";
import { useNavigationStore } from "../stores/navigation";
import { useReaderStore } from "../stores/reader";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("../api", () => ({
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

const texts = (wrapper: ReturnType<typeof mount>, selector: string) => wrapper.findAll(selector).map((e) => e.text());

it("lists what to continue, what is next and what was added, in the core's order", async () => {
  mock.setHomeForTest({ continueIds: ["book-0"], upNextIds: ["book-3", "book-1"], recentIds: ["book-2"] });
  const wrapper = mount(App);
  await flushPromises();
  expect(texts(wrapper, '[aria-labelledby="home-continue"] .continue-card__title')).toEqual(["The Lantern Keeper"]);
  expect(texts(wrapper, '[aria-labelledby="home-next"] .home__row-title')).toEqual([
    "An Unfinished Atlas",
    "A Map of Quiet Places",
  ]);
  expect(wrapper.findAll('[aria-labelledby="home-recent"] button').map((b) => b.attributes("aria-label"))).toEqual([
    "The Absent Orchard, book",
  ]);
  expect(wrapper.find('[aria-labelledby="home-week"]').exists()).toBe(true);
  wrapper.unmount();
});

it("says a book is read along when its recording narrates it", async () => {
  mock.setAudioSourceForTest("book-0", "recording");
  const wrapper = mount(App);
  await flushPromises();
  const card = wrapper.get('[aria-label="Continue The Lantern Keeper"]');
  expect(card.get(".continue-card__kind").text()).toBe("Book · read-along");
  expect(card.text()).toContain("Chapter 1 · 12%");
  wrapper.unmount();
});

it("opens the reader on a Continue card's book without playing it", async () => {
  const play = vi.spyOn(mock, "play");
  const wrapper = mount(App);
  await flushPromises();
  await wrapper.get('[aria-label="Continue The Lantern Keeper"]').trigger("click");
  await flushPromises();
  expect(useNavigationStore().view).toBe("reader");
  expect(useReaderStore().book?.id).toBe("book-0");
  expect(play).not.toHaveBeenCalled();
  wrapper.unmount();
});

it("leaves out a section with nothing in it, and opens an Up next book's page", async () => {
  mock.setHomeForTest({ continueIds: [], upNextIds: ["book-1"], recentIds: [] });
  const wrapper = mount(App);
  await flushPromises();
  expect(wrapper.find('[aria-labelledby="home-continue"]').exists()).toBe(false);
  expect(wrapper.find('[aria-labelledby="home-recent"]').exists()).toBe(false);
  await wrapper.get('[aria-label="About A Map of Quiet Places"]').trigger("click");
  expect(useNavigationStore().view).toBe("item");
  expect(useNavigationStore().bookId).toBe("book-1");
  wrapper.unmount();
});

it("invites the first add when there is nothing to show", async () => {
  mock.setHomeForTest({ continueIds: [], upNextIds: [], recentIds: [] });
  const wrapper = mount(App);
  await flushPromises();
  const welcome = wrapper.get('[aria-labelledby="home-welcome"]');
  expect(welcome.text()).toContain("Nothing to read yet");
  expect(welcome.find('[aria-label="Add books"]').exists()).toBe(true);
  expect(wrapper.find('[aria-labelledby="home-week"]').exists()).toBe(false);
  wrapper.unmount();
});

it("sums the last 7 days of listening with one bar per day", async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date("2026-11-11T12:00:00"));
  const wrapper = mount(App);
  await flushPromises();
  const week = wrapper.get('[aria-labelledby="home-week"] [aria-label="Last 7 days"]');
  expect(week.text()).toContain("1 h 50 min");
  expect(week.findAll(".week-card__bar")).toHaveLength(7);
  wrapper.unmount();
  vi.useRealTimers();
});
