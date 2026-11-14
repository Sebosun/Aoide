import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "../api/mock";
import App from "../App.vue";
import SettingsDialog from "../components/settings/SettingsDialog.vue";
import BookToolsSheet from "../components/book/BookToolsSheet.vue";
import { useReaderStore } from "../stores/reader";
import { useNavigationStore } from "../stores/navigation";
import { choose } from "./helpers";
import UiHost from "../components/ui/UiHost.vue";
import { useUiStore } from "../stores/ui";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("../api", () => ({ get api() { return backend.api; } }));
let mock: ReturnType<typeof createMockApi>;
const play = vi.fn(() => Promise.resolve());
beforeEach(() => {
  setActivePinia(createPinia()); mock = createMockApi(); backend.api = mock;
  play.mockClear();
  window.HTMLMediaElement.prototype.play = play;
  window.HTMLMediaElement.prototype.pause = vi.fn();
  URL.createObjectURL = vi.fn(() => "blob:preview");
  URL.revokeObjectURL = vi.fn();
});
afterEach(() => { mock.dispose(); document.body.innerHTML = ""; });

const tabNames = (wrapper: ReturnType<typeof mount>) => wrapper.findAll('[role="tab"]').map(t => t.text());
const selected = (wrapper: ReturnType<typeof mount>) => wrapper.get('[role="tab"][aria-selected="true"]');

it("opens settings from the gear as a modal with a tab per section, and Escape closes it", async () => {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  const gear = wrapper.get('header [aria-label="Settings"]');
  await gear.trigger("click");
  await flushPromises();
  const dialog = wrapper.get('[role="dialog"][aria-label="Settings"]');
  expect(dialog.attributes("aria-modal")).toBe("true");
  expect(dialog.get('[role="tablist"]').attributes("aria-orientation")).toBe("vertical");
  expect(tabNames(wrapper)).toEqual(["Appearance", "Speech engine", "Recordings", "Library", "Word highlighting", "Storage", "About"]);
  expect(selected(wrapper).text()).toBe("Appearance");
  expect(document.activeElement).toBe(selected(wrapper).element);
  expect(wrapper.findAll(".book-card").length).toBe(4);

  await dialog.trigger("keydown", { key: "Escape" });
  await flushPromises();
  expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
  expect(document.activeElement).toBe(gear.element);
  wrapper.unmount();
});

it("moves between tabs with the arrow keys, Home and End, and remembers the last one", async () => {
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  await selected(wrapper).trigger("keydown", { key: "ArrowDown" });
  expect(selected(wrapper).text()).toBe("Speech engine");
  expect(wrapper.get('[role="tabpanel"]').attributes("aria-labelledby")).toBe(selected(wrapper).attributes("id"));
  expect(wrapper.find('[aria-label="Engine type"]').exists()).toBe(true);
  await selected(wrapper).trigger("keydown", { key: "End" });
  expect(selected(wrapper).text()).toBe("About");
  await selected(wrapper).trigger("keydown", { key: "ArrowDown" });
  expect(selected(wrapper).text()).toBe("Appearance");
  await selected(wrapper).trigger("keydown", { key: "ArrowUp" });
  expect(selected(wrapper).text()).toBe("About");
  expect(useNavigationStore().settingsTab).toBe("about");
  wrapper.unmount();
});

it("keeps the appearance tab short and explains config and theme files under About", async () => {
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  const panel = () => wrapper.get('[role="tabpanel"]');
  expect(panel().text()).toContain("~/.config/aoide/settings.toml");
  expect(panel().text()).not.toContain("cover-tint");
  await wrapper.findAll('[role="tab"]').find(t => t.text() === "About")!.trigger("click");
  expect(panel().text()).toContain("~/.config/aoide");
  expect(panel().get("details").text()).toContain("cover-tint");
  wrapper.unmount();
});

it("previews the chosen voice with the unsaved engine settings", async () => {
  const preview = vi.spyOn(mock, "previewVoice");
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  await wrapper.findAll('[role="tab"]').find(t => t.text() === "Speech engine")!.trigger("click");
  await flushPromises();
  await choose(wrapper, "Default voice", "Olivia");
  await wrapper.get('[aria-label="Temperature"]').setValue("1.2");
  await wrapper.get('[aria-label="Preview voice"]').trigger("click");
  await flushPromises();
  expect(preview).toHaveBeenCalledWith(expect.objectContaining({ kind: "chatterbox", voiceId: "Olivia.wav", params: expect.objectContaining({ temperature: 1.2 }) }), "Olivia.wav", {}, null);
  expect(play).toHaveBeenCalled();
  expect(wrapper.find('[aria-label="Stop preview"]').exists()).toBe(true);
  await wrapper.get('[aria-label="Stop preview"]').trigger("click");
  expect(wrapper.find('[aria-label="Preview voice"]').exists()).toBe(true);
  wrapper.unmount();
});

it("previews a book's own voice from book tools", async () => {
  await useReaderStore().open("book-0");
  const preview = vi.spyOn(mock, "previewVoice");
  const wrapper = mount(BookToolsSheet, { attachTo: document.body });
  await wrapper.get('[role="tab"][aria-controls="book-tools-voice"]').trigger("click");
  await flushPromises();
  await choose(wrapper, "Voice for this book", "Adrian");
  await wrapper.get('[aria-label="Preview voice"]').trigger("click");
  await flushPromises();
  expect(preview).toHaveBeenCalledWith(expect.objectContaining({ kind: "chatterbox" }), "Adrian.wav", {}, "en");
  expect(play).toHaveBeenCalled();
  wrapper.unmount();
});

it("shows why a preview failed", async () => {
  vi.spyOn(mock, "previewVoice").mockRejectedValue("engine is unavailable: connection refused");
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  await wrapper.findAll('[role="tab"]').find(t => t.text() === "Speech engine")!.trigger("click");
  await flushPromises();
  await wrapper.get('[aria-label="Preview voice"]').trigger("click");
  await flushPromises();
  expect(wrapper.get('[role="alert"]').text()).toContain("connection refused");
  expect(play).not.toHaveBeenCalled();
  wrapper.unmount();
});

async function storageTab() {
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  await wrapper.findAll('[role="tab"]').find(t => t.text() === "Storage")!.trigger("click");
  await flushPromises();
  return wrapper;
}

it("shows what uses disk space, per kind and per book", async () => {
  const wrapper = await storageTab();
  const usage = wrapper.get('[aria-label="Disk use"]');
  for (const part of ["Generated audio", "Books", "Covers", "Database"]) expect(usage.text()).toContain(part);
  expect(usage.text()).toContain("2.3 MB");
  const rows = wrapper.findAll('[aria-label="Audio by book"] tbody tr');
  expect(rows).toHaveLength(1);
  expect(rows[0].text()).toContain("The Lantern Keeper");
  expect(rows[0].text()).toContain("2.3 MB");
  wrapper.unmount();
});

it("counts recordings' sync data and recognition models once there are any", async () => {
  let wrapper = await storageTab();
  expect(wrapper.get('[aria-label="Disk use"]').text()).not.toContain("Speech recognition");
  wrapper.unmount();
  mock.setRecordingForTest("book-0", "ready");
  wrapper = await storageTab();
  const usage = wrapper.get('[aria-label="Disk use"]').text();
  expect(usage).toContain("Speech recognition");
  expect(usage).toContain("Audiobook sync");
  wrapper.unmount();
});

it("cleans up unused files and deletes a book's audio after confirming", async () => {
  const clean = vi.spyOn(mock, "cleanUnusedFiles"), regenerate = vi.spyOn(mock, "regenerateBook");
  const wrapper = await storageTab();
  const host = mount(UiHost, { attachTo: document.body });
  const unused = wrapper.get('[aria-label="Unused files"]');
  expect(unused.text()).toContain("3 files");
  await unused.get("button").trigger("click");
  await flushPromises();
  expect(clean).toHaveBeenCalled();
  expect(useUiStore().toasts.map(t => t.text)).toContain("Freed 12.0 MB");
  expect(wrapper.get('[aria-label="Unused files"]').text()).toContain("Nothing to clean up");

  await wrapper.get('[aria-label="Delete audio of The Lantern Keeper"]').trigger("click");
  await flushPromises();
  await host.get('[data-confirm="yes"]').trigger("click");
  await flushPromises();
  expect(regenerate).toHaveBeenCalledWith("book-0");
  expect(wrapper.findAll('[aria-label="Audio by book"] tbody tr')).toHaveLength(0);
  wrapper.unmount();
  host.unmount();
});

it("deletes the audio of finished books, but not of the book that is playing", async () => {
  const regenerate = vi.spyOn(mock, "regenerateBook");
  await mock.play("book-0", 0, null);
  await mock.pause();
  mock.setProgressForTest("book-0", 1);
  mock.setProgressForTest("book-1", 1);
  mock.setAudioForTest("book-1", 5 * 1024 ** 2);
  const wrapper = await storageTab();
  const host = mount(UiHost, { attachTo: document.body });
  const finished = wrapper.get('[aria-label="Audio of finished books"]');
  expect(finished.text()).toContain("1 book");
  expect(finished.text()).toContain("5.0 MB");
  await finished.get("button").trigger("click");
  await flushPromises();
  await host.get('[data-confirm="yes"]').trigger("click");
  await flushPromises();
  expect(regenerate.mock.calls).toEqual([["book-1"]]);
  wrapper.unmount();
  host.unmount();
});
