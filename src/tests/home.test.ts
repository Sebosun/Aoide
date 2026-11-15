import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import App from "@/App.vue";
import { useNavigationStore } from "@/stores/navigation";
import { useReaderStore } from "@/stores/reader";

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
  await welcome.get('[aria-label="Add"]').trigger("click");
  await flushPromises();
  expect(wrapper.find('[role="dialog"][aria-label="Add to your library"]').exists()).toBe(true);
  expect(wrapper.find('[aria-labelledby="home-week"]').exists()).toBe(false);
  wrapper.unmount();
});

it("sums the last 7 days of reading and listening with one stacked bar per day", async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date("2026-11-11T12:00:00"));
  const wrapper = mount(App);
  await flushPromises();
  const week = wrapper.get('[aria-labelledby="home-week"] [aria-label="Last 7 days"]');
  expect(week.text()).toContain("30 min reading");
  expect(week.text()).toContain("1 h 50 min listening");
  const days = week.findAll(".week-card__day");
  expect(days).toHaveLength(7);
  // The 9th has both; today has neither.
  const ninth = days.find((d) => d.attributes("title")?.includes("20 min reading"))!;
  expect(ninth.findAll(".week-card__bar--reading")).toHaveLength(1);
  expect(ninth.findAll(".week-card__bar--listening")).toHaveLength(1);
  expect(days[6].findAll(".week-card__bar--none")).toHaveLength(1);
  wrapper.unmount();
  vi.useRealTimers();
});

it("shows an article to continue, next and recently added by its site and headline", async () => {
  mock.dispose();
  mock = createMockApi("?article=ready");
  backend.api = mock;
  mock.setHomeForTest({ continueIds: ["book-6"], upNextIds: ["book-6"], recentIds: ["book-6"] });
  const wrapper = mount(App);
  await flushPromises();
  const card = wrapper.get('[aria-labelledby="home-continue"] .continue-card');
  expect(card.get(".continue-card__kind").text()).toBe("Article · Quay Weekly");
  expect(card.find(".continue-card__cover").exists(), "no cover: the headline leads").toBe(false);
  expect(card.get(".continue-card__details").text()).toMatch(/^0% · .+ left/);

  const row = wrapper.get('[aria-labelledby="home-next"] .home__row');
  expect(row.get(".home__row-meta").text()).toBe("Quay Weekly · saved today");
  expect(row.get(".home__row-time").text()).toMatch(/min$/);

  const tile = wrapper.get('[aria-labelledby="home-recent"] button');
  expect(tile.attributes("aria-label")).toBe("The Quay Lights, article");
  expect(tile.text()).toContain("Quay Weekly");
  expect(tile.text()).toContain("The Quay Lights");
  wrapper.unmount();
});

it("marks an article in Up next that needs the user", async () => {
  mock.dispose();
  mock = createMockApi("?article=needsyou-check");
  backend.api = mock;
  const wrapper = mount(App);
  await flushPromises();
  const rows = wrapper.findAll('[aria-labelledby="home-next"] .home__row-meta');
  expect(rows[0].text()).toMatch(/· Needs you · the site asks for a check$/);
  expect(wrapper.findAll(".home__row-needs")).toHaveLength(1);
});
