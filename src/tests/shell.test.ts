import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "../api/mock";
import App from "../App.vue";
import { useLibraryStore } from "../stores/library";
import { usePlayerStore } from "../stores/player";
import { openMenu, pickMenuItem } from "./helpers";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("../api", () => ({ get api() { return backend.api; } }));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => { setActivePinia(createPinia()); mock = createMockApi(); backend.api = mock; });
afterEach(() => { mock.dispose(); });

const nav = (wrapper: ReturnType<typeof mount>, label: string) => wrapper.get(`nav[aria-label="Main"] [aria-label="${label}"]`);

it("navigates with the top bar pills and the settings button, marking the current screen", async () => {
  const wrapper = mount(App);
  await flushPromises();
  expect(wrapper.get("header").text()).toContain("aoide");
  expect(nav(wrapper, "Library").attributes("aria-current")).toBe("page");

  await nav(wrapper, "Stats").trigger("click");
  await flushPromises();
  expect(wrapper.find(".stats").exists()).toBe(true);
  expect(nav(wrapper, "Stats").attributes("aria-current")).toBe("page");
  expect(nav(wrapper, "Library").attributes("aria-current")).toBeUndefined();

  await wrapper.get('header [aria-label="Settings"]').trigger("click");
  await flushPromises();
  expect(wrapper.find('[role="dialog"][aria-label="Settings"]').exists()).toBe(true);
  expect(wrapper.find(".stats").exists()).toBe(true);
  wrapper.unmount();
});

it("enables Now Playing once a book is open and returns to it", async () => {
  const wrapper = mount(App);
  await flushPromises();
  expect(nav(wrapper, "Now Playing").attributes("disabled")).toBeDefined();
  await wrapper.get('[aria-label="Open The Lantern Keeper"]').trigger("click");
  await flushPromises();
  expect(nav(wrapper, "Now Playing").attributes("aria-current")).toBe("page");
  await nav(wrapper, "Library").trigger("click");
  await nav(wrapper, "Now Playing").trigger("click");
  await flushPromises();
  expect(wrapper.find('[aria-label="Chapter text"]').exists()).toBe(true);
  wrapper.unmount();
});

it("opens the playing book from Now Playing when no book is open", async () => {
  const wrapper = mount(App);
  await flushPromises();
  await mock.play("book-0", 0, null);
  await flushPromises();
  await nav(wrapper, "Now Playing").trigger("click");
  await flushPromises();
  expect(wrapper.find('[aria-label="Chapter text"]').exists()).toBe(true);
  wrapper.unmount();
});

it("shows every engine state in the top bar pill, with the detail as its title", async () => {
  const wrapper = mount(App);
  await flushPromises();
  for (const state of ["offline", "loading", "ready", "busy", "error"] as const) {
    mock.emitEngine({ profileName: "Local voice", state, detail: `detail ${state}` });
    await flushPromises();
    const pill = wrapper.get("header .engine-pill");
    expect(pill.text()).toContain("Local voice");
    expect(pill.text()).toContain(state);
    expect(pill.attributes("title")).toBe(`detail ${state}`);
    expect(pill.classes()).toContain(`engine-pill--${state}`);
  }
  wrapper.unmount();
});

it("reaches Find books and rescan from the Add EPUB menu, which closes on Escape", async () => {
  const rescan = vi.spyOn(mock, "rescanLibrary");
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  const add = wrapper.get('[aria-label="Add EPUB"]');
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
  await wrapper.get('[aria-label="Open The Lantern Keeper"]').trigger("click");
  await flushPromises();
  expect(wrapper.find('[aria-label="Chapter text"]').exists()).toBe(true);
  await wrapper.get('[aria-label="Library"]').trigger("click");
  expect(wrapper.findAll(".book-card")).toHaveLength(4);
  wrapper.unmount();
});


it("shows the drop overlay only for files dragged in from outside, never for things dragged inside the app", async () => {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  const app = wrapper.get(".app"), main = wrapper.get("main");
  const inside = { dataTransfer: { types: ["text/uri-list", "text/html"] } }, files = { dataTransfer: { types: ["Files"] } };
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

it("takes the wordmark to Now Playing when a book is open, else to the library", async () => {
  const wrapper = mount(App);
  await flushPromises();
  const home = () => wrapper.get('header [aria-label="Aoide home"]');
  await nav(wrapper, "Stats").trigger("click");
  await home().trigger("click");
  await flushPromises();
  expect(nav(wrapper, "Library").attributes("aria-current")).toBe("page");

  await mock.play("book-0", 0, null);
  await flushPromises();
  await nav(wrapper, "Stats").trigger("click");
  await home().trigger("click");
  await flushPromises();
  expect(nav(wrapper, "Now Playing").attributes("aria-current")).toBe("page");
  expect(wrapper.find('[aria-label="Chapter text"]').exists()).toBe(true);
  wrapper.unmount();
});

it("opens Now Playing from the title on the Continue listening card", async () => {
  const play = vi.spyOn(mock, "play");
  const wrapper = mount(App);
  await flushPromises();
  await wrapper.get('[aria-label="Continue listening"] [aria-label="Open The Lantern Keeper in Now Playing"]').trigger("click");
  await flushPromises();
  expect(nav(wrapper, "Now Playing").attributes("aria-current")).toBe("page");
  expect(wrapper.find('[aria-label="Chapter text"]').exists()).toBe(true);
  expect(play).not.toHaveBeenCalled();
  wrapper.unmount();
});
