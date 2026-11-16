import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import App from "@/App.vue";
import { useLibraryStore } from "@/stores/library";
import { usePlayerStore } from "@/stores/player";
import { useRecordingsStore } from "@/stores/recordings";

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
});

const nav = (wrapper: ReturnType<typeof mount>, label: string) =>
  wrapper.get(`nav[aria-label="Main"] [aria-label="${label}"]`);

it("opens on Home and shows each sidebar entry's screen, marking the current one", async () => {
  const wrapper = mount(App);
  await flushPromises();
  expect(wrapper.get('nav[aria-label="Main"]').text()).toContain("aoide");
  expect(wrapper.find(".home").exists()).toBe(true);
  expect(nav(wrapper, "Home").attributes("aria-current")).toBe("page");

  for (const [label, screen] of [
    ["Library", ".library"],
    ["Activity", ".activity"],
    ["Home", ".home"],
  ]) {
    await nav(wrapper, label).trigger("click");
    await flushPromises();
    expect(wrapper.find(screen).exists()).toBe(true);
    expect(nav(wrapper, label).attributes("aria-current")).toBe("page");
  }
  expect(nav(wrapper, "Library").attributes("aria-current")).toBeUndefined();

  await nav(wrapper, "Settings").trigger("click");
  await flushPromises();
  expect(wrapper.find('[role="dialog"][aria-label="Settings"]').exists()).toBe(true);
  expect(wrapper.find(".home").exists()).toBe(true);
  wrapper.unmount();
});

it("hides the sidebar in the reader and goes back to the screen the reader was opened from", async () => {
  const wrapper = mount(App);
  await flushPromises();
  await nav(wrapper, "Library").trigger("click");
  await flushPromises();
  await wrapper.get('[aria-label="Open The Lantern Keeper"]').trigger("click");
  await flushPromises();
  expect(wrapper.find('[aria-label="Chapter text"]').exists()).toBe(true);
  expect(wrapper.find('nav[aria-label="Main"]').exists()).toBe(false);

  await wrapper.get('[aria-label="Back to Library"]').trigger("click");
  await flushPromises();
  expect(wrapper.find(".library").exists()).toBe(true);
  expect(nav(wrapper, "Library").attributes("aria-current")).toBe("page");
  wrapper.unmount();
});

it("shows what is playing in the sidebar and opens the reader from it", async () => {
  const wrapper = mount(App);
  await flushPromises();
  expect(wrapper.find('nav [aria-label="Now playing"]').exists()).toBe(false);
  await mock.play({ bookId: "book-0", chapter: 0, segmentId: null });
  await flushPromises();
  const card = wrapper.get('nav [aria-label="Now playing"]');
  expect(card.text()).toContain("The Lantern Keeper");
  expect(card.text()).toContain("Playing");
  await card.get('[aria-label="Open The Lantern Keeper in the reader"]').trigger("click");
  await flushPromises();
  expect(wrapper.find('[aria-label="Chapter text"]').exists()).toBe(true);
  await wrapper.get('[aria-label="Back to Home"]').trigger("click");
  await flushPromises();
  expect(wrapper.find(".home").exists()).toBe(true);
  wrapper.unmount();
});

it("opens Library filtered to a collection from the sidebar", async () => {
  const wrapper = mount(App);
  await flushPromises();
  const library = useLibraryStore();
  const night = await library.createCollection("Night reading");
  await library.setBookCollections("book-1", [night]);
  await flushPromises();
  const link = wrapper.get('nav[aria-label="Main"] [aria-labelledby="sidebar-collections"] .sidebar__collection');
  expect(link.text()).toBe("Night reading1");
  await link.trigger("click");
  await flushPromises();
  expect(link.attributes("aria-current")).toBe("page");
  expect(nav(wrapper, "Library").attributes("aria-current")).toBeUndefined();
  expect(wrapper.findAll(".book-card")).toHaveLength(1);
  await nav(wrapper, "Library").trigger("click");
  await flushPromises();
  expect(wrapper.findAll(".book-card")).toHaveLength(4);
  wrapper.unmount();
});

const dialog = (wrapper: ReturnType<typeof mount>) => wrapper.find('[role="dialog"][aria-label="Add to your library"]');
const button = (wrapper: ReturnType<typeof mount>, text: string) =>
  dialog(wrapper)
    .findAll("button")
    .find((b) => b.text().includes(text))!;

it("adds from one dialog, opened by the sidebar's Add and by Ctrl N, and closed by Escape", async () => {
  const rescan = vi.spyOn(mock, "rescanLibrary");
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  await nav(wrapper, "Add").trigger("click");
  await flushPromises();
  expect(dialog(wrapper).text()).toContain("Drop books or audiobooks anywhere on this window");
  await button(wrapper, "Rescan library folder").trigger("click");
  await flushPromises();
  expect(rescan).toHaveBeenCalled();
  expect(dialog(wrapper).exists()).toBe(false);

  window.dispatchEvent(new KeyboardEvent("keydown", { key: "n", ctrlKey: true, cancelable: true }));
  await flushPromises();
  expect(dialog(wrapper).exists()).toBe(true);
  await dialog(wrapper).trigger("keydown", { key: "Escape" });
  await flushPromises();
  expect(dialog(wrapper).exists()).toBe(false);

  await nav(wrapper, "Add").trigger("click");
  await flushPromises();
  await button(wrapper, "Scan for books").trigger("click");
  await flushPromises();
  expect(wrapper.find(".find").exists()).toBe(true);
  await wrapper.get('[aria-label="Back to library"]').trigger("click");
  expect(useLibraryStore().books.length).toBe(5);
  expect(wrapper.find(".find").exists()).toBe(false);
  wrapper.unmount();
});

it("imports chosen EPUBs, closing the dialog on Library", async () => {
  const imported = vi.spyOn(mock, "importFiles");
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  await nav(wrapper, "Add").trigger("click");
  await flushPromises();
  await button(wrapper, "Choose books").trigger("click");
  await flushPromises();
  expect(imported).toHaveBeenCalledWith({ paths: ["~/Downloads/the-lantern-keeper.epub"] });
  expect(dialog(wrapper).exists()).toBe(false);
  expect(nav(wrapper, "Library").attributes("aria-current")).toBe("page");
  wrapper.unmount();
});

it("pairs a chosen audiobook with a book, as a dropped one is", async () => {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  await nav(wrapper, "Add").trigger("click");
  await flushPromises();
  await button(wrapper, "Choose an audiobook…").trigger("click");
  await flushPromises();
  expect(dialog(wrapper).exists()).toBe(false);
  expect(useRecordingsStore().pairing?.paths).toEqual(["~/Audiobooks/the-lantern-keeper.m4b"]);
  wrapper.unmount();
});

it("starts subscriptions once and switches from library to the selected reader", async () => {
  const subscribe = vi.spyOn(mock, "onPlayerState");
  const wrapper = mount(App);
  await flushPromises();
  await usePlayerStore().start();
  expect(subscribe).toHaveBeenCalledTimes(1);
  await nav(wrapper, "Library").trigger("click");
  await flushPromises();
  await wrapper.get('[aria-label="Open The Lantern Keeper"]').trigger("click");
  await flushPromises();
  expect(wrapper.find('[aria-label="Chapter text"]').exists()).toBe(true);
  await wrapper.get('[aria-label="Back to Library"]').trigger("click");
  await flushPromises();
  expect(wrapper.findAll(".book-card")).toHaveLength(4);
  wrapper.unmount();
});

it("shows the drop overlay only for files dragged in from outside, never for things dragged inside the app", async () => {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  const app = wrapper.get(".app"),
    main = wrapper.get("main");
  const inside = { dataTransfer: { types: ["text/uri-list", "text/html"] } },
    files = { dataTransfer: { types: ["Files"] } };
  await app.trigger("dragenter", inside);
  await app.trigger("dragover", inside);
  expect(wrapper.find(".drop-overlay").exists()).toBe(false);

  await app.trigger("dragenter", files);
  await main.trigger("dragenter", files);
  await app.trigger("dragleave", files);
  expect(wrapper.find(".drop-overlay").exists()).toBe(true);
  await main.trigger("dragleave", files);
  expect(wrapper.find(".drop-overlay").exists()).toBe(false);

  await app.trigger("dragenter", files);
  window.dispatchEvent(new Event("blur"));
  await flushPromises();
  expect(wrapper.find(".drop-overlay").exists()).toBe(false);

  for (const image of wrapper.findAll("img")) expect(image.attributes("draggable")).toBe("false");
  wrapper.unmount();
});

const linkField = (wrapper: ReturnType<typeof mount>) => dialog(wrapper).get('input[type="url"]');

it("saves a link from the Add dialog: the card shows the fetch, then the article", async () => {
  const save = vi.spyOn(mock, "saveArticle");
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  await nav(wrapper, "Add").trigger("click");
  await flushPromises();
  expect(dialog(wrapper).text()).toContain("A link to an article");
  await linkField(wrapper).setValue("https://quay.example/lights");
  await dialog(wrapper).get("form").trigger("submit");
  await flushPromises();
  expect(save).toHaveBeenCalledWith({ url: "https://quay.example/lights" });
  expect(dialog(wrapper).exists()).toBe(false);
  expect(wrapper.find(".library").exists()).toBe(true);
  const card = () => wrapper.findAll(".book-card").find((c) => c.find(".book-card__tile").exists())!;
  expect(card().get(".book-card__status").text()).toBe("Fetching…");
  expect(card().find(".book-card__tile--loading").exists()).toBe(true);
  await vi.waitFor(() => expect(card().get(".book-card__title").text()).toBe("The Quay Lights"), { timeout: 3000 });
  expect(card().get(".book-card__tile").text()).toContain("Quay Weekly");
  expect(card().get(".book-card__status").text()).toBe("Article · unread");
  wrapper.unmount();
});

it("shows why a link is not one and saves nothing", async () => {
  const save = vi.spyOn(mock, "saveArticle");
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  await nav(wrapper, "Add").trigger("click");
  await flushPromises();
  await linkField(wrapper).setValue("not a link");
  await dialog(wrapper).get("form").trigger("submit");
  await flushPromises();
  expect(save).not.toHaveBeenCalled();
  expect(dialog(wrapper).get('[role="alert"]').text()).toBe("That is not a web address.");
  expect(linkField(wrapper).attributes("aria-invalid")).toBe("true");
  wrapper.unmount();
});

it("opens Add with a link pasted outside a text field", async () => {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  const paste = (text: string, target: EventTarget = document.body) => {
    const event = new Event("paste", { bubbles: true, cancelable: true });
    Object.defineProperty(event, "clipboardData", { value: { getData: () => text } });
    target.dispatchEvent(event);
  };
  paste("just some words");
  await flushPromises();
  expect(dialog(wrapper).exists()).toBe(false);
  paste(" https://quay.example/lights ");
  await flushPromises();
  expect((linkField(wrapper).element as HTMLInputElement).value).toBe("https://quay.example/lights");
  wrapper.unmount();
});
