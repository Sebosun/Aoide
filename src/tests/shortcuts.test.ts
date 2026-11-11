import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "../api/mock";
import App from "../App.vue";
import { useNavigationStore } from "../stores/navigation";
import { usePlayerStore } from "../stores/player";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("../api", () => ({ get api() { return backend.api; } }));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => { setActivePinia(createPinia()); mock = createMockApi(); backend.api = mock; });
afterEach(() => { mock.dispose(); document.body.innerHTML = ""; });

/** Presses a key on the focused element (the body when nothing has focus). */
const press = (key: string, init: KeyboardEventInit = {}) => {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init });
  (document.activeElement ?? document.body).dispatchEvent(event);
  return event;
};
async function app() {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}
async function reading(wrapper: Awaited<ReturnType<typeof app>>) {
  await wrapper.get('[aria-label="Open The Lantern Keeper"]').trigger("click");
  await flushPromises();
  (document.activeElement as HTMLElement | null)?.blur();
}

it("plays and pauses with Space, except while typing, on a control or under a dialog", async () => {
  const wrapper = await app();
  expect(press(" ").defaultPrevented).toBe(false);
  await reading(wrapper);
  const player = usePlayerStore();
  expect(press(" ").defaultPrevented).toBe(true);
  await flushPromises();
  expect(player.state.status).toBe("playing");
  press(" ");
  await flushPromises();
  expect(player.state.status).toBe("paused");

  await wrapper.get('nav[aria-label="Main"] [aria-label="Library"]').trigger("click");
  await flushPromises();
  wrapper.get<HTMLInputElement>('[aria-label="Search library"]').element.focus();
  expect(press(" ").defaultPrevented).toBe(false);
  // A button reached with the keyboard keeps Space for itself…
  wrapper.get<HTMLElement>('[aria-label="Open Now Playing"]').element.focus();
  expect(press(" ").defaultPrevented).toBe(false);
  // …one that was clicked does not: Space still plays.
  document.body.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  wrapper.get<HTMLElement>('nav[aria-label="Main"] [aria-label="Stats"]').element.focus();
  expect(press(" ").defaultPrevented).toBe(true);
  await flushPromises();
  expect(player.state.status).toBe("playing");
  press(" ");
  await flushPromises();
  expect(player.state.status).toBe("paused");
  (document.activeElement as HTMLElement).blur();
  useNavigationStore().openSettings();
  await flushPromises();
  press(" ");
  await flushPromises();
  expect(player.state.status).toBe("paused");
  wrapper.unmount();
});

it("moves a sentence back or forward with the arrow keys, also when the text is not open", async () => {
  const play = vi.spyOn(mock, "play");
  const wrapper = await app();
  await mock.play("book-0", 0, "0:1:0");
  await mock.pause();
  await flushPromises();
  play.mockClear();
  expect(press("ArrowRight").defaultPrevented).toBe(true);
  await flushPromises();
  expect(play).toHaveBeenLastCalledWith("book-0", 0, "0:2:0");
  await mock.pause();
  press("ArrowLeft");
  await flushPromises();
  expect(play).toHaveBeenLastCalledWith("book-0", 0, "0:1:0");
  await mock.pause();
  play.mockClear();
  // Text fields keep their arrow keys.
  wrapper.get<HTMLInputElement>('[aria-label="Search library"]').element.focus();
  expect(press("ArrowRight").defaultPrevented).toBe(false);
  await flushPromises();
  expect(play).not.toHaveBeenCalled();
  wrapper.unmount();
});

it("searches with Ctrl F: the open book from Now Playing, the library from its screen", async () => {
  const wrapper = await app();
  expect(press("f", { ctrlKey: true }).defaultPrevented).toBe(true);
  expect((document.activeElement as HTMLElement).getAttribute("aria-label")).toBe("Search library");
  await reading(wrapper);
  press("f", { ctrlKey: true });
  await flushPromises();
  expect(wrapper.find('[role="dialog"][aria-label="Book tools"]').exists()).toBe(true);
  expect(wrapper.get('[role="tab"][aria-selected="true"]').text()).toBe("Search");
  expect((document.activeElement as HTMLElement).getAttribute("aria-label")).toBe("Search in book");
  // Again from another tab of the open sheet.
  await wrapper.get("#book-tools-tab-voice").trigger("click");
  await flushPromises();
  press("f", { ctrlKey: true });
  await flushPromises();
  expect(wrapper.get('[role="tab"][aria-selected="true"]').text()).toBe("Search");
  expect((document.activeElement as HTMLElement).getAttribute("aria-label")).toBe("Search in book");
  wrapper.unmount();
});

it("opens and closes Settings with Ctrl comma, and lists the shortcuts under About", async () => {
  const wrapper = await app();
  expect(press(",", { ctrlKey: true }).defaultPrevented).toBe(true);
  await flushPromises();
  expect(wrapper.find('[role="dialog"][aria-label="Settings"]').exists()).toBe(true);
  // Search stays out of the way of a dialog.
  expect(press("f", { ctrlKey: true }).defaultPrevented).toBe(false);
  await wrapper.get("#settings-tab-about").trigger("click");
  await flushPromises();
  const keys = wrapper.get('[aria-label="Keyboard shortcuts"]');
  for (const text of ["Space", "Play or pause", "Previous or next sentence", "Ctrl", "Settings", "Larger interface"]) expect(keys.text(), text).toContain(text);
  press(",", { ctrlKey: true });
  await flushPromises();
  expect(wrapper.find('[role="dialog"][aria-label="Settings"]').exists()).toBe(false);
  wrapper.unmount();
});
