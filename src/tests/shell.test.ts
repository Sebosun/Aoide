import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "../api/mock";
import App from "../App.vue";
import { useLibraryStore } from "../stores/library";
import { usePlayerStore } from "../stores/player";
import { openMenu, pickMenuItem } from "./helpers";

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
    ["Activity", ".stats"],
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
  await mock.play("book-0", 0, null);
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
  const link = wrapper.get('nav[aria-label="Main"] [aria-labelledby="sidebar-collections"] button');
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

it("reaches Find books and rescan from the Add menu, which closes on Escape", async () => {
  const rescan = vi.spyOn(mock, "rescanLibrary");
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  const add = wrapper.get('[aria-label="Add books"]');
  await add.trigger("click");
  expect(add.attributes("aria-expanded")).toBe("true");
  expect(openMenu()!.textContent).toContain("drop EPUB files anywhere on this window");
  expect(openMenu()!.textContent).not.toContain("~/Documents/Aoide/Books");
  await pickMenuItem("Rescan library folder");
  expect(rescan).toHaveBeenCalled();
  expect(openMenu()).toBeNull();

  await add.trigger("click");
  openMenu()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await flushPromises();
  expect(openMenu()).toBeNull();
  expect(document.activeElement).toBe(add.element);

  await add.trigger("click");
  await pickMenuItem("Find books on this computer");
  expect(wrapper.find(".find").exists()).toBe(true);
  await wrapper.get('[aria-label="Back to library"]').trigger("click");
  expect(useLibraryStore().books.length).toBe(5);
  expect(wrapper.find(".find").exists()).toBe(false);
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
