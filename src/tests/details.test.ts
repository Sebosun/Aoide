import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import App from "@/App.vue";
import { useRecordingsStore } from "@/stores/recordings";
import { usePlayerStore } from "@/stores/player";
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
  expect(about.text()).toContain("3 extraction notes");
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
  expect(play).toHaveBeenCalledWith({ bookId: "book-1", chapter: null, segmentId: null });
  expect(useNavigationStore().view).toBe("reader");

  useNavigationStore().openItem("book-1");
  await flushPromises();
  await page(wrapper).get('[aria-label="Contents"] [aria-label="Open chapter 2: The long way home"]').trigger("click");
  await flushPromises();
  expect(play).toHaveBeenLastCalledWith({ bookId: "book-1", chapter: 1, segmentId: "1:0:0" });
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
  expect(pair).toHaveBeenCalledWith({ folder: false });
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
  expect(prepare).toHaveBeenCalledWith({ bookId: "book-0" });
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

it("puts the book on a collection from its page, with its button or the C key", async () => {
  const id = await mock.createCollection({ name: "Night reading" });
  const shelve = vi.spyOn(mock, "setBookCollections");
  useNavigationStore().openItem("book-1");
  const wrapper = await app();
  const picker = () =>
    document.body.querySelector<HTMLElement>('[role="dialog"][aria-label="Collections for A Map of Quiet Places"]');
  await page(wrapper).get('[aria-label="Add to collection"]').trigger("click");
  await flushPromises();
  picker()!.querySelector<HTMLElement>('[role="option"][aria-label="Night reading"]')!.click();
  await flushPromises();
  expect(shelve).toHaveBeenCalledWith({ bookId: "book-1", ids: [id] });
  picker()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await flushPromises();
  expect(picker()).toBeNull();

  window.dispatchEvent(new KeyboardEvent("keydown", { key: "c" }));
  await flushPromises();
  const field = picker()!.querySelector<HTMLInputElement>('[aria-label="Find or create a collection"]')!;
  expect(document.activeElement).toBe(field);
  field.value = "nig";
  field.dispatchEvent(new Event("input"));
  await flushPromises();
  field.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await flushPromises();
  expect(shelve).toHaveBeenLastCalledWith({ bookId: "book-1", ids: [] });
  expect(picker()).toBeNull();
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

it("marks the book finished, and unread again, from its page", async () => {
  const wrapper = await app();
  await wrapper.get('[aria-label="Open A Map of Quiet Places"]').trigger("click");
  await flushPromises();
  const about = page(wrapper);
  await about
    .findAll("button")
    .find((b) => b.text() === "Mark as finished")!
    .trigger("click");
  await flushPromises();
  expect(about.get(".item-page__kind").text()).toBe("Book · finished");
  const button = about.findAll("button").find((b) => b.text() === "Mark as unread");
  expect(button).toBeDefined();
  await button!.trigger("click");
  await flushPromises();
  expect(about.get(".item-page__kind").text()).toBe("Book · unread");
  expect(about.findAll("button").some((b) => b.text() === "Mark as finished")).toBe(true);
  wrapper.unmount();
});

it("archives the item from its page, says so there, and takes it out again", async () => {
  const wrapper = await app();
  await wrapper.get('[aria-label="Open A Map of Quiet Places"]').trigger("click");
  await flushPromises();
  const about = page(wrapper);
  const button = (text: string) => about.findAll("button").find((b) => b.text() === text);
  await button("Archive")!.trigger("click");
  await flushPromises();
  expect((await mock.listBooks()).find((b) => b.title === "A Map of Quiet Places")!.archived).toBe(true);
  expect(about.get(".item-page__kind").text()).toBe("Book · unread · archived");
  await button("Take out of the archive")!.trigger("click");
  await flushPromises();
  expect(about.get(".item-page__kind").text()).toBe("Book · unread");
  expect(button("Archive")).toBeDefined();
  wrapper.unmount();
});

it("leads with Read for a book read silently and with Listen for one listened to", async () => {
  const wrapper = await app();
  await wrapper.get('[aria-label="Open A Map of Quiet Places"]').trigger("click");
  await flushPromises();
  let about = page(wrapper);
  expect(about.get('[aria-label="Read"]').classes()).toContain("ui-button--accent");
  expect(about.get('[aria-label="Listen"]').classes()).toContain("ui-button--raised");
  expect(about.get('[aria-labelledby="item-text"]').text()).toMatch(/About .+ to read/);

  await mock.setMode({ bookId: "book-1", mode: "listen" });
  await flushPromises();
  about = page(wrapper);
  expect(about.get('[aria-label="Listen"]').classes()).toContain("ui-button--accent");
  // Read from here reads silently again.
  const setMode = vi.spyOn(mock, "setMode");
  await about.get('[aria-label="Read"]').trigger("click");
  await flushPromises();
  expect(setMode).toHaveBeenCalledWith({ bookId: "book-1", mode: "read" });
  expect(useReaderStore().shape).toBe("read");
  wrapper.unmount();
});

it("shows an audiobook's page: Listen only, no text, its length, parts and files", async () => {
  await mock.addAudiobook({ paths: ["~/Audiobooks/Winter Radio"] });
  const play = vi.spyOn(mock, "play");
  useNavigationStore().openItem("book-5");
  const wrapper = await app();
  const about = wrapper.get('[aria-label="About Winter Radio"]');
  expect(about.get(".item-page__kind").text()).toBe("Audiobook · unplayed");
  expect(about.find('[aria-label="Read"]').exists()).toBe(false);
  expect(about.get('[aria-label="Listen"]').classes()).toContain("ui-button--accent");
  expect(about.get('[aria-labelledby="item-text"]').text()).toContain("No text");
  const audio = about.get('[aria-labelledby="item-audio"]');
  expect(audio.text()).toContain("2 parts · 2 files");
  expect(audio.findAll('[aria-label="Audio files"] li')).toHaveLength(2);
  const contents = about.get('[aria-labelledby="item-contents"]');
  expect(contents.text()).toContain("Part One");
  expect(contents.text()).toContain("0:01");
  expect(about.find('[aria-labelledby="item-online"]').exists(), "no details online").toBe(false);

  await contents.get('[aria-label="Play part 2: Part Two"]').trigger("click");
  await flushPromises();
  expect(play).toHaveBeenCalledWith({ bookId: "book-5", chapter: 1, segmentId: null });
  expect(useNavigationStore().view).toBe("reader");
  await mock.pause();
  wrapper.unmount();
});

/** The app on the item page of the preview's article (book-6), saved in `stage`. */
async function articlePage(stage: string) {
  mock.dispose();
  mock = createMockApi(`?article=${stage}`);
  backend.api = mock;
  useNavigationStore().openItem("book-6");
  const wrapper = await app();
  // Its title is its site's address until a copy is fetched.
  return { wrapper, about: () => wrapper.get(".item-page") };
}

it("shows an article's page: its site and fetch, narration only, the original and fetching again", async () => {
  const { wrapper, about } = await articlePage("needsyou-cutshort");
  const open = vi.spyOn(mock, "openOriginal"),
    refetch = vi.spyOn(mock, "refetchArticle");
  expect(about().get(".item-page__kind").text()).toBe("Article · unread");
  expect(about().find('[aria-label="Read"]').exists()).toBe(true);
  const text = about().get('[aria-labelledby="item-text"]');
  expect(text.text()).toContain("Quay Weekly");
  expect(text.text()).toContain("93 words");
  expect(text.text()).toContain("Needs you · only part of it was saved");
  const audio = about().get('[aria-labelledby="item-audio"]');
  expect(audio.find('[aria-label="Narrate with a voice"]').exists()).toBe(true);
  expect(audio.find('[aria-label="Pair an audiobook"]').exists(), "no pairing").toBe(false);
  expect(about().find('[aria-labelledby="item-contents"]').exists(), "one chapter: no contents").toBe(false);
  expect(about().find('[aria-labelledby="item-online"]').exists(), "no details online").toBe(false);

  await about().get('[aria-label="Open original"]').trigger("click");
  expect(open).toHaveBeenCalledWith({ bookId: "book-6" });
  await about().get('[aria-label="Fetch again"]').trigger("click");
  await flushPromises();
  expect(refetch).toHaveBeenCalledWith({ bookId: "book-6", source: "http" });
  expect(about().get('[aria-labelledby="item-text"]').text()).toContain("Fetching…");
  expect(about().get('[aria-label="Fetch again"]').attributes("disabled")).toBeDefined();
  wrapper.unmount();
});

it("offers nothing that needs a web page on a saved email's page", async () => {
  mock.dispose();
  mock = createMockApi("?article=email");
  backend.api = mock;
  useNavigationStore().openItem("book-7");
  const wrapper = await app();
  const about = () => wrapper.get(".item-page");
  expect(about().get(".item-page__kind").text()).toBe("Article · unread");
  expect(about().get('[aria-labelledby="item-text"]').text()).toContain("Harbour Weekly");
  expect(about().find('[aria-label="Read"]').exists()).toBe(true);
  expect(about().find('[aria-label="Open original"]').exists()).toBe(false);
  expect(about().find('[aria-label="Fetch again"]').exists()).toBe(false);
  wrapper.unmount();
});

it("shows why an article could not be fetched, and fetches it again", async () => {
  const { wrapper, about } = await articlePage("failed");
  expect(wrapper.find(".item-page__error").exists(), "not an error loading the page").toBe(false);
  const text = about().get('[aria-labelledby="item-text"]');
  expect(text.text()).toContain("Could not be fetched");
  expect(text.text()).toContain("The site answered with status 503.");
  expect(about().find('[aria-label="Read"]').exists(), "nothing to read").toBe(false);
  expect(about().find('[aria-labelledby="item-audio"]').exists(), "nothing to narrate").toBe(false);

  // A fuller fetch arrives: the page shows the text it got.
  vi.useFakeTimers();
  await about().get('[aria-label="Fetch again"]').trigger("click");
  await vi.advanceTimersByTimeAsync(1500);
  vi.useRealTimers();
  await flushPromises();
  expect(about().find('[aria-label="Read"]').exists()).toBe(true);
  expect(about().get('[aria-labelledby="item-text"]').text()).toContain("93 words");
  wrapper.unmount();
});

it("says an article was rendered in the browser, which the mock, like the core, refuses while it is off", async () => {
  const { wrapper, about } = await articlePage("failed");
  await expect(mock.refetchArticle({ bookId: "book-6", source: "browser" })).rejects.toThrow("The browser is off.");
  await mock.updateAppSettings({ patch: { browser: { use: "fallback", path: null, idleSeconds: 60 } } });
  vi.useFakeTimers();
  await mock.refetchArticle({ bookId: "book-6", source: "browser" });
  await vi.advanceTimersByTimeAsync(1500);
  vi.useRealTimers();
  await flushPromises();
  expect(about().get('[aria-labelledby="item-text"]').text()).toContain("rendered in the browser");
  wrapper.unmount();
});

it("lists an item's tags on its page and edits them there", async () => {
  useNavigationStore().openItem("book-1");
  const wrapper = await app();
  const tags = () => page(wrapper).get('[aria-label="Tags"]');
  expect(
    tags()
      .findAll("li")
      .map((t) => t.text()),
  ).toEqual(["Cities", "Fiction", "Literary"]);
  await page(wrapper).get('[aria-label="Edit tags"]').trigger("click");
  await flushPromises();
  const dialog = document.body.querySelector('[role="dialog"][aria-label="Tags for A Map of Quiet Places"]');
  expect(dialog).not.toBeNull();
  dialog!.querySelector<HTMLButtonElement>('[aria-label="Remove the tag Cities"]')!.click();
  await flushPromises();
  expect(
    tags()
      .findAll("li")
      .map((t) => t.text()),
  ).toEqual(["Fiction", "Literary"]);
  wrapper.unmount();
});
