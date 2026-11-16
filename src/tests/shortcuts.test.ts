import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import App from "@/App.vue";
import { useNavigationStore } from "@/stores/navigation";
import { usePlayerStore } from "@/stores/player";
import { SHORTCUTS, keyLabels, matchShortcut } from "@/shortcuts";
import readme from "@/../README.md?raw";

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

/** Presses a key on the focused element (the body when nothing has focus). */
const press = (key: string, init: KeyboardEventInit = {}) => {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init });
  (document.activeElement ?? document.body).dispatchEvent(event);
  return event;
};
/** The app, on Library unless a test has opened another screen first. */
async function app() {
  if (useNavigationStore().view === "home") useNavigationStore().view = "library";
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

  await wrapper.get('[aria-label="Back to Library"]').trigger("click");
  await flushPromises();
  wrapper.get<HTMLInputElement>('[aria-label="Search library"]').element.focus();
  expect(press(" ").defaultPrevented).toBe(false);
  // A button reached with the keyboard keeps Space for itself…
  wrapper.get<HTMLElement>('[aria-label="Open The Lantern Keeper in the reader"]').element.focus();
  expect(press(" ").defaultPrevented).toBe(false);
  // …one that was clicked does not: Space still plays.
  document.body.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  wrapper.get<HTMLElement>('nav[aria-label="Main"] [aria-label="Activity"]').element.focus();
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
  await mock.play({ bookId: "book-0", chapter: 0, segmentId: "0:1:0" });
  await mock.pause();
  await flushPromises();
  play.mockClear();
  expect(press("ArrowRight").defaultPrevented).toBe(true);
  await flushPromises();
  expect(play).toHaveBeenLastCalledWith({ bookId: "book-0", chapter: 0, segmentId: "0:1:1" });
  await mock.pause();
  press("ArrowLeft");
  await flushPromises();
  expect(play).toHaveBeenLastCalledWith({ bookId: "book-0", chapter: 0, segmentId: "0:1:0" });
  await mock.pause();
  play.mockClear();
  // Text fields keep their arrow keys.
  wrapper.get<HTMLInputElement>('[aria-label="Search library"]').element.focus();
  expect(press("ArrowRight").defaultPrevented).toBe(false);
  await flushPromises();
  expect(play).not.toHaveBeenCalled();
  wrapper.unmount();
});

it("searches with Ctrl F: the open book from the reader, the library from its screen", async () => {
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
  await wrapper.get("#book-tools-tab-details").trigger("click");
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
  for (const text of ["Space", "Play or pause", "Previous or next sentence", "Ctrl", "Settings", "Larger interface"])
    expect(keys.text(), text).toContain(text);
  press(",", { ctrlKey: true });
  await flushPromises();
  expect(wrapper.find('[role="dialog"][aria-label="Settings"]').exists()).toBe(false);
  wrapper.unmount();
});

it("leaves the reader with Escape, unless Escape is closing something over it", async () => {
  const wrapper = await app();
  await reading(wrapper);
  await wrapper.get('[aria-label="Bookmarks and details"]').trigger("click");
  await flushPromises();
  expect(wrapper.find('[role="dialog"][aria-label="Book tools"]').exists()).toBe(true);
  press("Escape");
  await flushPromises();
  expect(useNavigationStore().view).toBe("reader");

  useNavigationStore().closeTools();
  await flushPromises();
  await wrapper.get('[aria-label="Text size"]').trigger("click");
  press("Escape");
  await flushPromises();
  expect(useNavigationStore().view).toBe("reader");

  (document.activeElement as HTMLElement | null)?.blur();
  await wrapper.get('[aria-label="Text size"]').trigger("click");
  await flushPromises();
  expect(press("Escape").defaultPrevented).toBe(true);
  await flushPromises();
  expect(useNavigationStore().view).toBe("library");
  wrapper.unmount();
});

it("finds each listed shortcut from its keys, so the list and the handler share one table", () => {
  for (const shortcut of SHORTCUTS)
    for (const combo of shortcut.keys) {
      const event = new KeyboardEvent("keydown", { key: combo.key, ctrlKey: combo.ctrl, shiftKey: combo.shift });
      expect(matchShortcut(event)?.shortcut.id, keyLabels(combo).join(" ")).toBe(shortcut.id);
    }
  // Ctrl with a letter ignores the letter's case; Shift is part of a key, except on a symbol it types.
  expect(matchShortcut(new KeyboardEvent("keydown", { key: "F", ctrlKey: true }))?.shortcut.id).toBe("find");
  expect(matchShortcut(new KeyboardEvent("keydown", { key: " ", shiftKey: true }))).toBeNull();
  expect(matchShortcut(new KeyboardEvent("keydown", { key: "+", ctrlKey: true, shiftKey: true }))?.shortcut.id).toBe(
    "zoomIn",
  );
  expect(matchShortcut(new KeyboardEvent("keydown", { key: "=", ctrlKey: true }))?.shortcut.id).toBe("zoomIn");
  expect(keyLabels({ key: " " })).toEqual(["Space"]);
  expect(keyLabels({ key: "-", ctrl: true })).toEqual(["Ctrl", "−"]);
});

/** The keys an audiobook player has, each with the call it makes (from a book paused on 0:1:0
 *  at 1×, at full volume). */
const PLAYER_KEYS: { keys: string; key: string; init?: KeyboardEventInit; call: string; with: unknown }[] = [
  { keys: "Shift →", key: "ArrowRight", init: { shiftKey: true }, call: "skipChapter", with: { delta: 1 } },
  { keys: "Shift ←", key: "ArrowLeft", init: { shiftKey: true }, call: "skipChapter", with: { delta: -1 } },
  {
    keys: "Ctrl →",
    key: "ArrowRight",
    init: { ctrlKey: true },
    call: "play",
    with: { bookId: "book-0", chapter: 0, segmentId: "0:2:1" },
  },
  { keys: "]", key: "]", call: "setSpeed", with: { speed: 1.1 } },
  { keys: "[", key: "[", call: "setSpeed", with: { speed: 0.9 } },
  { keys: "\\", key: "\\", call: "setSpeed", with: { speed: 1 } },
  { keys: "Shift ↓", key: "ArrowDown", init: { shiftKey: true }, call: "setVolume", with: { volume: 0.9 } },
  { keys: "Shift ↑", key: "ArrowUp", init: { shiftKey: true }, call: "setVolume", with: { volume: 1 } },
  { keys: "M", key: "m", call: "setVolume", with: { volume: 0 } },
  { keys: "B", key: "b", call: "addBookmark", with: { bookId: "book-0", note: "" } },
];

async function pausedOnABook() {
  const wrapper = await app();
  await mock.play({ bookId: "book-0", chapter: 0, segmentId: "0:1:0" });
  await mock.pause();
  await flushPromises();
  return wrapper;
}

it.each(PLAYER_KEYS)(
  "$keys makes its call from the library while a book is loaded",
  async ({ key, init, call, with: args }) => {
    const wrapper = await pausedOnABook();
    const spy = vi.spyOn(mock, call as keyof typeof mock);
    expect(press(key, init).defaultPrevented).toBe(true);
    await flushPromises();
    expect(spy).toHaveBeenLastCalledWith(args);
    wrapper.unmount();
  },
);

it.each(PLAYER_KEYS)("$keys does nothing in a text field", async ({ key, init, call }) => {
  const wrapper = await pausedOnABook();
  const spy = vi.spyOn(mock, call as keyof typeof mock);
  wrapper.get<HTMLInputElement>('[aria-label="Search library"]').element.focus();
  expect(press(key, init).defaultPrevented).toBe(false);
  await flushPromises();
  expect(spy).not.toHaveBeenCalled();
  wrapper.unmount();
});

it("steps speed by tenths within 0.5× and 3×, and volume by tenths, saying where they are", async () => {
  const wrapper = await pausedOnABook();
  const speed = vi.spyOn(mock, "setSpeed"),
    volume = vi.spyOn(mock, "setVolume");
  await usePlayerStore().setSpeed(1.25);
  press("]");
  await flushPromises();
  expect(speed).toHaveBeenLastCalledWith({ speed: 1.3 });
  expect(document.body.textContent).toContain("Speed 1.3×");
  await usePlayerStore().setSpeed(3);
  press("]");
  await flushPromises();
  expect(speed).toHaveBeenLastCalledWith({ speed: 3 });
  await usePlayerStore().setSpeed(0.5);
  press("[");
  await flushPromises();
  expect(speed).toHaveBeenLastCalledWith({ speed: 0.5 });
  // Muting and back, then from silence up a step.
  press("m");
  await flushPromises();
  expect(document.body.textContent).toContain("Muted");
  press("m");
  await flushPromises();
  expect(volume).toHaveBeenLastCalledWith({ volume: 1 });
  await usePlayerStore().setVolume(0);
  press("ArrowUp", { shiftKey: true });
  await flushPromises();
  expect(volume).toHaveBeenLastCalledWith({ volume: 0.1 });
  expect(document.body.textContent).toContain("Volume 10%");
  wrapper.unmount();
});

it("moves 30 s with Ctrl and the arrows in an audiobook on its own", async () => {
  const wrapper = await app();
  await mock.addAudiobook({ paths: ["~/Audiobooks/Winter Radio"] });
  await mock.play({ bookId: "book-5", chapter: 0, segmentId: null });
  await mock.pause();
  await flushPromises();
  const skip = vi.spyOn(mock, "skip");
  press("ArrowRight", { ctrlKey: true });
  await flushPromises();
  expect(skip).toHaveBeenLastCalledWith({ bookId: "book-5", deltaMs: 30_000 });
  press("ArrowLeft", { ctrlKey: true });
  await flushPromises();
  expect(skip).toHaveBeenLastCalledWith({ bookId: "book-5", deltaMs: -30_000 });
  wrapper.unmount();
});

it("opens the sleep timer menu with T, in the library and in the reader", async () => {
  const wrapper = await pausedOnABook();
  const menu = () => document.querySelector('[role="listbox"][aria-label="Sleep timer"]');
  expect(press("t").defaultPrevented).toBe(true);
  await flushPromises();
  expect(menu()).not.toBeNull();
  // Escape on the open menu closes it.
  press("Escape");
  await flushPromises();
  expect(menu()).toBeNull();
  await reading(wrapper);
  await flushPromises();
  expect(menu()).toBeNull();
  press("t");
  await flushPromises();
  expect(menu()).not.toBeNull();
  wrapper.unmount();
});

it("shows the list of shortcuts with ?, the same list as Settings › About, and Escape closes it", async () => {
  const wrapper = await app();
  expect(press("?", { shiftKey: true }).defaultPrevented).toBe(true);
  await flushPromises();
  const sheet = wrapper.get('[role="dialog"][aria-label="Keyboard shortcuts"]');
  for (const shortcut of SHORTCUTS) expect(sheet.text()).toContain(shortcut.label);
  expect(sheet.text()).toContain("Shift");
  press("Escape");
  await flushPromises();
  expect(wrapper.find('[role="dialog"][aria-label="Keyboard shortcuts"]').exists()).toBe(false);
  // Not while typing.
  wrapper.get<HTMLInputElement>('[aria-label="Search library"]').element.focus();
  expect(press("?", { shiftKey: true }).defaultPrevented).toBe(false);
  await flushPromises();
  expect(wrapper.find('[role="dialog"][aria-label="Keyboard shortcuts"]').exists()).toBe(false);
  wrapper.unmount();
});

it("lists the same shortcuts in the README as in the app", () => {
  // A lone backslash would escape the tag after it in Markdown.
  const kbd = (key: string) => `<kbd>${key.replace("\\", "\\\\")}</kbd>`;
  const rows = SHORTCUTS.map(
    (shortcut) =>
      `| ${shortcut.keys.map((combo) => keyLabels(combo).map(kbd).join(" ")).join(" / ")} | ${shortcut.label} |`,
  );
  const section = readme.split("### Keyboard shortcuts")[1]?.split("\n## ")[0] ?? "";
  const table = section.split("\n").filter((line) => line.startsWith("| ") && !/^\| (Keys|---)/.test(line));
  expect(table).toEqual(rows);
});
