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
const start = (search = "") => {
  mock = createMockApi(search);
  backend.api = mock;
};
beforeEach(() => {
  setActivePinia(createPinia());
  start();
});
afterEach(() => {
  mock.dispose();
  document.body.innerHTML = "";
});

async function openFeeds() {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  await wrapper.get('nav[aria-label="Main"] [aria-label="Feeds"]').trigger("click");
  await flushPromises();
  return wrapper;
}
const subscriptions = (wrapper: ReturnType<typeof mount>) =>
  wrapper.findAll('[aria-label="Subscriptions"] .feeds__line').map((e) =>
    e
      .findAll("span")
      .map((part) => part.text())
      .join(" "),
  );
const titles = (wrapper: ReturnType<typeof mount>) => wrapper.findAll(".feed-entry__title").map((e) => e.text());
const entry = (wrapper: ReturnType<typeof mount>, title: string) =>
  wrapper.findAll(".feed-entry").find((e) => e.find(".feed-entry__title").text() === title)!;

it("shows the unread count beside Feeds and lists subscriptions and entries by day", async () => {
  const wrapper = await openFeeds();
  expect(wrapper.get('nav[aria-label="Main"] [aria-label="Feeds"]').text()).toBe("Feeds5");
  expect(wrapper.get(".feeds__summary").text()).toBe("5 new · checked 20 minutes ago");
  expect(subscriptions(wrapper)).toEqual([
    "All new 5",
    "Saved for later",
    "Field Notes 1",
    "Small Cities 2",
    "Tidewatch 2",
  ]);
  expect(wrapper.findAll(".feeds__day").map((d) => d.text())[0]).toBe("Today");
  expect(titles(wrapper).slice(0, 3)).toEqual([
    "The ferry that runs on the tide table",
    "Who decides where the bus shelter goes",
    "The quiet return of the night train",
  ]);
  const ferry = entry(wrapper, "The ferry that runs on the tide table");
  expect(ferry.get(".feed-entry__meta").text()).toBe("Tidewatch · 2 hours ago · 1 min read");
  expect(ferry.find('[aria-label="Unread"]').exists()).toBe(true);
  const shelter = entry(wrapper, "Who decides where the bus shelter goes");
  expect(shelter.get(".feed-entry__meta").text()).toBe("Small Cities · 5 hours ago · summary only");
  expect(shelter.text()).toContain("The full article is fetched when you open it.");
});

it("shows one subscription's entries when it is chosen", async () => {
  const wrapper = await openFeeds();
  await wrapper
    .findAll(".feeds__source")
    .find((s) => s.text().startsWith("Small Cities"))!
    .trigger("click");
  await flushPromises();
  expect(titles(wrapper)).toEqual(["Who decides where the bus shelter goes", "The last public clock in town"]);
  expect(wrapper.get(".feeds__source[aria-current='true']").text()).toContain("Small Cities");
});

it("opens an entry in the reader and it is read after", async () => {
  const wrapper = await openFeeds();
  await entry(wrapper, "The ferry that runs on the tide table").get(".feed-entry__open").trigger("click");
  await flushPromises();
  expect(useNavigationStore().view).toBe("reader");
  expect(useReaderStore().book).not.toBeNull();
  useNavigationStore().leaveReader();
  await flushPromises();
  expect(entry(wrapper, "The ferry that runs on the tide table").find('[aria-label="Unread"]').exists()).toBe(false);
  expect(wrapper.get('nav[aria-label="Main"] [aria-label="Feeds"]').text()).toBe("Feeds4");
});

it("saves an entry to the library and takes it out again", async () => {
  const wrapper = await openFeeds();
  const save = () => entry(wrapper, "The quiet return of the night train").get(".feed-entry__save");
  expect(save().attributes("aria-pressed")).toBe("false");
  await save().trigger("click");
  await flushPromises();
  expect(save().attributes("aria-pressed")).toBe("true");
  expect(save().attributes("aria-label")).toBe("Saved to library");
  expect(entry(wrapper, "The quiet return of the night train").get(".feed-entry__meta").text()).toContain(
    "in your library",
  );
  expect((await mock.listBooks()).some((b) => b.kind === "article")).toBe(true);
  await save().trigger("click");
  await flushPromises();
  expect(save().attributes("aria-pressed")).toBe("false");
  expect((await mock.listBooks()).some((b) => b.kind === "article")).toBe(false);
});

it("says since when a feed is not reachable", async () => {
  start("?feeds=failing");
  const wrapper = await openFeeds();
  const harbour = wrapper.findAll(".feeds__source").find((s) => s.text().startsWith("Harbour Letters"))!;
  expect(harbour.get(".feeds__failing").text()).toMatch(/^Not reachable since \w+/);
});

it("loads older entries at the end of the list", async () => {
  mock.addFeedEntriesForTest(60);
  const wrapper = await openFeeds();
  expect(titles(wrapper)).toHaveLength(50);
  await wrapper.get(".feeds__more").trigger("click");
  await flushPromises();
  expect(titles(wrapper)).toHaveLength(66);
  expect(wrapper.find(".feeds__more").exists()).toBe(false);
});

it("invites a first subscription when there is none", async () => {
  start("?feeds=empty");
  const wrapper = await openFeeds();
  expect(wrapper.get(".feeds__empty").text()).toContain("No subscriptions yet");
  await wrapper.get(".feeds__empty button").trigger("click");
  expect(useNavigationStore().addOpen).toBe(true);
});

it("says when nothing is new", async () => {
  const wrapper = await openFeeds();
  await wrapper
    .findAll("button")
    .find((b) => b.text() === "Mark all read")!
    .trigger("click");
  await flushPromises();
  expect(wrapper.get('nav[aria-label="Main"] [aria-label="Feeds"]').text()).toBe("Feeds");
  expect(wrapper.get(".feeds__summary").text()).toBe("Nothing new · checked 20 minutes ago");
  await wrapper
    .findAll(".feeds__source")
    .find((s) => s.text() === "Saved for later")!
    .trigger("click");
  await flushPromises();
  expect(wrapper.get(".feeds__empty").text()).toContain("Nothing saved yet");
});
