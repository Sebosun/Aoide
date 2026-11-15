import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "../api/mock";
import { initialGeneration } from "../api/initial";
import App from "../App.vue";
import { useNavigationStore } from "../stores/navigation";
import { usePlayerStore } from "../stores/player";
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

const panel = () => document.body.querySelector<HTMLElement>('[role="dialog"][aria-label="Speech queue"]');
const rows = () => [...document.body.querySelectorAll<HTMLElement>('[aria-label="Preparation queue"] > li')];
const button = (label: string) => {
  const found = document.body.querySelector<HTMLButtonElement>(`[role="dialog"] [aria-label="${label}"]`);
  if (!found) throw new Error(`No button "${label}"`);
  return found;
};
async function openQueue() {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  const pill = wrapper.get('nav [aria-haspopup="dialog"].engine-card');
  expect(pill.attributes("aria-expanded")).toBe("false");
  await pill.trigger("click");
  await flushPromises();
  expect(pill.attributes("aria-expanded")).toBe("true");
  return { wrapper, pill };
}

it("shows the books waiting for preparation in queue order, from the sidebar card", async () => {
  await mock.prepareBook("book-1");
  await mock.prepareBook("book-0");
  const { wrapper } = await openQueue();
  expect(panel()?.textContent).toContain("Local Chatterbox");
  expect(rows().map((r) => r.querySelector("button")?.textContent?.trim())).toEqual([
    "A Map of Quiet Places",
    "The Lantern Keeper",
  ]);
  expect(rows()[0].textContent).toContain("Preparing");
  expect(rows()[0].textContent).toContain("3 / 120");
  expect(rows()[1].textContent).toContain("Queued");
  // While another book is being made, a queued one waits its turn.
  expect(document.body.querySelector('[aria-label="Start preparing The Lantern Keeper"]')).toBeNull();
  wrapper.unmount();
});

it("offers to start a book left waiting in the queue, as after a start with resuming turned off", async () => {
  mock.queueWaitingForTest("book-1");
  const prepare = vi.spyOn(mock, "prepareBook");
  const { wrapper } = await openQueue();
  expect(rows()[0].textContent).toContain("Waiting");
  expect(rows()[0].textContent).not.toContain("0 / 0");
  expect(document.body.querySelector('[aria-label="Pause preparing A Map of Quiet Places"]')).toBeNull();
  button("Start preparing A Map of Quiet Places").click();
  await flushPromises();
  expect(prepare).toHaveBeenCalledWith("book-1");
  expect(rows()[0].textContent).toContain("Preparing");
  wrapper.unmount();
});

it("pauses, resumes and cancels a book from the queue", async () => {
  await mock.prepareBook("book-1");
  await mock.prepareBook("book-0");
  const pause = vi.spyOn(mock, "setPreparePaused"),
    cancel = vi.spyOn(mock, "cancelPrepare");
  const { wrapper } = await openQueue();
  button("Pause preparing A Map of Quiet Places").click();
  await flushPromises();
  expect(pause).toHaveBeenLastCalledWith("book-1", true);
  expect(rows()[0].textContent).toContain("Paused");
  expect(rows()[1].textContent).toContain("Preparing");
  button("Resume preparing A Map of Quiet Places").click();
  await flushPromises();
  expect(pause).toHaveBeenLastCalledWith("book-1", false);
  button("Cancel preparing The Lantern Keeper").click();
  await flushPromises();
  expect(cancel).toHaveBeenCalledWith("book-0");
  expect(rows()).toHaveLength(1);
  wrapper.unmount();
});

it("shows what is being made for the book playing now", async () => {
  await mock.prepareBook("book-1");
  const { wrapper } = await openQueue();
  mock.emitGeneration({
    ...initialGeneration(),
    bookId: "book-0",
    pending: 4,
    bufferedAheadMs: 12_000,
    generatingSegmentId: "0:1:0",
  });
  await flushPromises();
  const now = document.body.querySelector('[aria-label="Playing now"]');
  expect(now?.textContent).toContain("The Lantern Keeper");
  expect(now?.textContent).toContain("4 sentences to make");
  expect(now?.textContent).toContain("12 s ready ahead");
  wrapper.unmount();
});

it("shows the engine in the sidebar only while a book is queued or the engine has failed", async () => {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  const card = () => wrapper.find("nav .engine-card");
  expect(card().exists()).toBe(false);
  expect(wrapper.text()).not.toContain("Local Chatterbox");

  await mock.prepareBook("book-1");
  await flushPromises();
  expect(card().text()).toContain("Narrating 1 book");
  expect(card().text()).toContain("A Map of Quiet Places · 3 / 120");
  await mock.cancelPrepare("book-1");
  await flushPromises();
  expect(card().exists()).toBe(false);

  mock.emitEngine({ profileName: "Local Chatterbox", state: "error", detail: "The server stopped" });
  await flushPromises();
  expect(card().text()).toContain("Local Chatterbox has a problem");
  expect(card().get('[role="alert"]').text()).toBe("The server stopped");
  wrapper.unmount();
});

it("opens a book or the engine settings from the queue, and closes on Escape", async () => {
  await mock.prepareBook("book-1");
  const { wrapper, pill } = await openQueue();
  button("Speech engine settings").click();
  await flushPromises();
  const navigation = useNavigationStore();
  expect(navigation.settingsOpen).toBe(true);
  expect(navigation.settingsTab).toBe("engine");
  expect(panel()).toBeNull();
  navigation.closeSettings();
  await flushPromises();

  await pill.trigger("click");
  await flushPromises();
  rows()[0].querySelector("button")!.click();
  await flushPromises();
  expect(navigation.view).toBe("item");
  expect(navigation.bookId).toBe("book-1");
  expect(panel()).toBeNull();

  await pill.trigger("click");
  await flushPromises();
  panel()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await flushPromises();
  expect(panel()).toBeNull();
  expect(document.activeElement).toBe(pill.element);
  wrapper.unmount();
});

it("says in the reader's player when the speech engine cannot speak", async () => {
  useNavigationStore().view = "library";
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  await wrapper.get('[aria-label="Open The Lantern Keeper"]').trigger("click");
  await flushPromises();
  const status = () => wrapper.get(".dock__status").text();
  expect(status()).toBe("");
  mock.emitEngine({ profileName: "Local Chatterbox", state: "offline", detail: null });
  await flushPromises();
  expect(status()).toBe("Local Chatterbox is offline");
  mock.emitEngine({ profileName: "Local Chatterbox", state: "busy", detail: null });
  await flushPromises();
  expect(status()).toBe("");
  wrapper.unmount();
});

it("keeps the engine's problem out of sight while a book is only read, until the user listens", async () => {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  mock.emitEngine({ profileName: "Local Chatterbox", state: "error", detail: "The server stopped", wanted: false });
  await useReaderStore().open("book-1");
  await flushPromises();
  expect(useNavigationStore().view).toBe("reader");
  expect(wrapper.find("nav .engine-card").exists()).toBe(false);
  expect(document.body.querySelector('[role="alert"]')).toBeNull();
  expect(wrapper.find('[aria-label="Player"]').exists()).toBe(false);
  useNavigationStore().leaveReader();
  await flushPromises();
  expect(wrapper.find("nav .engine-card").exists()).toBe(false);

  await usePlayerStore().play("book-1", 0, null);
  mock.emitEngine({ profileName: "Local Chatterbox", state: "error", detail: "The server stopped" });
  useNavigationStore().leaveReader();
  await flushPromises();
  expect(wrapper.get("nav .engine-card").text()).toContain("Local Chatterbox has a problem");
  wrapper.unmount();
});
