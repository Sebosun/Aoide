import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "../api/mock";
import App from "../App.vue";
import { useRecordingsStore } from "../stores/recordings";
import { usePlayerStore } from "../stores/player";
import { useNavigationStore } from "../stores/navigation";

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

/** The app, on Library unless a test has opened another screen first. */
async function app() {
  if (useNavigationStore().view === "home") useNavigationStore().view = "library";
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}
const page = (wrapper: ReturnType<typeof mount>) => wrapper.get('[aria-label="About A Map of Quiet Places"]');

it("opens an unread book on its page, without playing", async () => {
  const play = vi.spyOn(mock, "play");
  const wrapper = await app();
  await wrapper.get('[aria-label="Open A Map of Quiet Places"]').trigger("click");
  await flushPromises();
  expect(useNavigationStore().view).toBe("item");
  const about = page(wrapper);
  expect(about.get("h1").text()).toBe("A Map of Quiet Places");
  expect(about.get(".item-page__kind").text()).toBe("Book · unread");
  expect(about.text()).toContain("Ellis Reed");
  expect(about.text()).toContain("2 extraction notes");
  expect(about.text()).not.toContain("Start listening");
  expect(about.text()).not.toContain("Listening time");
  expect(play).not.toHaveBeenCalled();
  wrapper.unmount();
});

it("reads without playing, and listens from the saved place", async () => {
  const play = vi.spyOn(mock, "play");
  useNavigationStore().openItem("book-1");
  const wrapper = await app();
  await page(wrapper).get('[aria-label="Read"]').trigger("click");
  await flushPromises();
  expect(useNavigationStore().view).toBe("reader");
  expect(wrapper.find('[aria-label="Chapter text"]').exists()).toBe(true);
  expect(play).not.toHaveBeenCalled();
  expect(usePlayerStore().state.status).not.toBe("playing");

  useNavigationStore().openItem("book-1");
  await flushPromises();
  await page(wrapper).get('[aria-label="Listen"]').trigger("click");
  await flushPromises();
  expect(play).toHaveBeenCalledWith("book-1", null, null);
  expect(useNavigationStore().view).toBe("reader");

  useNavigationStore().openItem("book-1");
  await flushPromises();
  await page(wrapper).get('[aria-label="Contents"] [aria-label="Open chapter 2: The long way home"]').trigger("click");
  await flushPromises();
  expect(play).toHaveBeenLastCalledWith("book-1", 1, "1:0:0");
  expect(useNavigationStore().view).toBe("reader");
  wrapper.unmount();
});

const audioCard = (wrapper: ReturnType<typeof mount>) => wrapper.get('[aria-labelledby="item-audio"]');

it("offers a voice or an audiobook when nothing is set up to play", async () => {
  const pair = vi.spyOn(mock, "chooseAudio");
  useNavigationStore().openItem("book-1");
  const wrapper = await app();
  const card = audioCard(wrapper);
  expect(card.attributes("data-audio")).toBe("none");
  expect(card.text()).toContain("None yet");
  expect(card.get('[aria-label="Narrate with a voice"]').text()).toContain("Read aloud by Local Chatterbox");
  await card.get('[aria-label="Pair an audiobook"]').trigger("click");
  await flushPromises();
  expect(pair).toHaveBeenCalledWith(false);
  expect(useRecordingsStore().pairing?.bookId).toBe("book-1");
  wrapper.unmount();
});

it("shows the voice and what is prepared when speech plays the book", async () => {
  const prepare = vi.spyOn(mock, "prepareBook");
  useNavigationStore().openItem("book-0");
  const wrapper = await app();
  const card = audioCard(wrapper);
  expect(card.attributes("data-audio")).toBe("speech");
  expect(card.text()).toContain("Local Chatterbox · 2.3 MB made so far");
  await card.get('[aria-label="Prepare whole book"]').trigger("click");
  await flushPromises();
  expect(prepare).toHaveBeenCalledWith("book-0");
  await card.get('[aria-label="Pause preparing"]').trigger("click");
  wrapper.unmount();
});

it("shows the recording and the narration switch when the book has a recording", async () => {
  mock.setRecordingForTest("book-0", "ready");
  useNavigationStore().openItem("book-0");
  const wrapper = await app();
  const card = audioCard(wrapper);
  expect(card.attributes("data-audio")).toBe("recording");
  expect(card.find('[role="radiogroup"][aria-label="Narration"]').exists()).toBe(true);
  expect(card.text()).toContain("Recording");
  wrapper.unmount();
});

it("puts the book on a collection from its page", async () => {
  const id = await mock.createCollection("Night reading");
  const shelve = vi.spyOn(mock, "setBookCollections");
  useNavigationStore().openItem("book-1");
  const wrapper = await app();
  await page(wrapper).get('[aria-label="Add to collection"]').trigger("click");
  document.body.querySelector<HTMLElement>('[role="menuitemcheckbox"][aria-label="Night reading"]')!.click();
  await flushPromises();
  expect(shelve).toHaveBeenCalledWith("book-1", [id]);
  wrapper.unmount();
});

it("shows the time spent with a started book: total, days, a bar per day and time left", async () => {
  useNavigationStore().openItem("book-0");
  const wrapper = await app();
  const listening = wrapper.get('[aria-label="About The Lantern Keeper"] [aria-label="Your listening"]');
  expect(listening.get(".book-listening__total").text()).toBe("1 h 30 min");
  expect(listening.text()).toContain("on 3 days");
  expect(listening.findAll(".book-listening__bar")).toHaveLength(3);
  expect(listening.text()).toMatch(/left at 1×/);
  wrapper.unmount();
});

it("leaves the listening section out for a book nobody has listened to", async () => {
  useNavigationStore().openItem("book-1");
  const wrapper = await app();
  expect(wrapper.find('[aria-label="Your listening"]').exists()).toBe(false);
  wrapper.unmount();
});
