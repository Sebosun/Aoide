import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import App from "@/App.vue";
import SettingsDialog from "@/components/settings/SettingsDialog.vue";
import BookToolsSheet from "@/components/book/BookToolsSheet.vue";
import { useReaderStore } from "@/stores/reader";
import { useNavigationStore } from "@/stores/navigation";
import { choose } from "@/tests/helpers";
import UiHost from "@/components/ui/UiHost.vue";
import { useUiStore } from "@/stores/ui";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("@/api", () => ({
  get api() {
    return backend.api;
  },
}));
let mock: ReturnType<typeof createMockApi>;
const play = vi.fn(() => Promise.resolve());
beforeEach(() => {
  setActivePinia(createPinia());
  mock = createMockApi();
  backend.api = mock;
  play.mockClear();
  window.HTMLMediaElement.prototype.play = play;
  window.HTMLMediaElement.prototype.pause = vi.fn();
  URL.createObjectURL = vi.fn(() => "blob:preview");
  URL.revokeObjectURL = vi.fn();
});
afterEach(() => {
  mock.dispose();
  document.body.innerHTML = "";
});

const tabNames = (wrapper: ReturnType<typeof mount>) => wrapper.findAll('[role="tab"]').map((t) => t.text());
const selected = (wrapper: ReturnType<typeof mount>) => wrapper.get('[role="tab"][aria-selected="true"]');

it("opens settings from the gear as a modal with a tab per section, and Escape closes it", async () => {
  useNavigationStore().view = "library";
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  const gear = wrapper.get('nav [aria-label="Settings"]');
  await gear.trigger("click");
  await flushPromises();
  const dialog = wrapper.get('[role="dialog"][aria-label="Settings"]');
  expect(dialog.attributes("aria-modal")).toBe("true");
  expect(dialog.get('[role="tablist"]').attributes("aria-orientation")).toBe("vertical");
  expect(tabNames(wrapper)).toEqual([
    "Appearance",
    "Speech engine",
    "Recordings",
    "Library",
    "Feeds",
    "Sites",
    "Word highlighting",
    "Storage",
    "About",
  ]);
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
  await wrapper
    .findAll('[role="tab"]')
    .find((t) => t.text() === "About")!
    .trigger("click");
  expect(panel().text()).toContain("~/.config/aoide");
  expect(panel().get("details").text()).toContain("cover-tint");
  wrapper.unmount();
});

it("previews the chosen voice with the unsaved engine settings", async () => {
  const preview = vi.spyOn(mock, "previewVoice");
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  await wrapper
    .findAll('[role="tab"]')
    .find((t) => t.text() === "Speech engine")!
    .trigger("click");
  await flushPromises();
  await choose(wrapper, "Default voice", "Olivia");
  await wrapper.get('[aria-label="Temperature"]').setValue("1.2");
  await wrapper.get('[aria-label="Preview voice"]').trigger("click");
  await flushPromises();
  expect(preview).toHaveBeenCalledWith({
    profile: expect.objectContaining({
      kind: "chatterbox",
      voiceId: "Olivia.wav",
      params: expect.objectContaining({ temperature: 1.2 }),
    }),
    voiceId: "Olivia.wav",
    params: {},
    language: null,
  });
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
  expect(preview).toHaveBeenCalledWith({
    profile: expect.objectContaining({ kind: "chatterbox" }),
    voiceId: "Adrian.wav",
    params: {},
    language: "en",
  });
  expect(play).toHaveBeenCalled();
  wrapper.unmount();
});

it("shows why a preview failed", async () => {
  vi.spyOn(mock, "previewVoice").mockRejectedValue("engine is unavailable: connection refused");
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  await wrapper
    .findAll('[role="tab"]')
    .find((t) => t.text() === "Speech engine")!
    .trigger("click");
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
  await wrapper
    .findAll('[role="tab"]')
    .find((t) => t.text() === "Storage")!
    .trigger("click");
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
  const clean = vi.spyOn(mock, "cleanUnusedFiles"),
    regenerate = vi.spyOn(mock, "regenerateBook");
  const wrapper = await storageTab();
  const host = mount(UiHost, { attachTo: document.body });
  const unused = wrapper.get('[aria-label="Unused files"]');
  expect(unused.text()).toContain("3 files");
  await unused.get("button").trigger("click");
  await flushPromises();
  expect(clean).toHaveBeenCalled();
  expect(useUiStore().toasts.map((t) => t.text)).toContain("Freed 12.0 MB");
  expect(wrapper.get('[aria-label="Unused files"]').text()).toContain("Nothing to clean up");

  await wrapper.get('[aria-label="Delete audio of The Lantern Keeper"]').trigger("click");
  await flushPromises();
  await host.get('[data-confirm="yes"]').trigger("click");
  await flushPromises();
  expect(regenerate).toHaveBeenCalledWith({ bookId: "book-0" });
  expect(wrapper.findAll('[aria-label="Audio by book"] tbody tr')).toHaveLength(0);
  wrapper.unmount();
  host.unmount();
});

it("deletes the audio of finished books, but not of the book that is playing", async () => {
  const regenerate = vi.spyOn(mock, "regenerateBook");
  await mock.play({ bookId: "book-0", chapter: 0, segmentId: null });
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
  expect(regenerate.mock.calls).toEqual([[{ bookId: "book-1" }]]);
  wrapper.unmount();
  host.unmount();
});

it("uses a recognition server instead of the built-in model", async () => {
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  await wrapper
    .findAll('[role="tab"]')
    .find((t) => t.text() === "Recordings")!
    .trigger("click");
  await flushPromises();
  expect(wrapper.find('[aria-label="Speech-recognition models"]').exists()).toBe(true);
  expect(wrapper.find('[aria-label="Recognition server address"]').exists()).toBe(false);

  await choose(
    wrapper,
    "What recognises the narration",
    "A recognition serverOpenAI-style, such as faster-whisper-server",
  );
  expect(wrapper.find('[aria-label="Speech-recognition models"]').exists(), "no model is needed here").toBe(false);
  await wrapper.get('[aria-label="Recognition server address"]').setValue("http://127.0.0.1:8001");
  await wrapper.get('[aria-label="Recognition server model"]').setValue("Systran/faster-whisper-base.en");
  await wrapper.get("form.recording-settings").trigger("submit");
  await flushPromises();
  const saved = (await mock.appSettings()).recognition;
  expect(saved).toMatchObject({
    engine: "server",
    server: { baseUrl: "http://127.0.0.1:8001", model: "Systran/faster-whisper-base.en", network: "loopback" },
  });
  wrapper.unmount();
});

async function openFeedSettings() {
  useNavigationStore().openSettings("feeds");
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}

it("keeps how often feeds are checked and how long entries stay", async () => {
  let wrapper = await openFeedSettings();
  expect(selected(wrapper).text()).toBe("Feeds");
  await choose(wrapper, "How often feeds are checked", "Every 6 hours");
  const read = wrapper.get<HTMLInputElement>('input[aria-label="Days to keep read entries"]');
  expect(read.element.value).toBe("30");
  await read.setValue("14");
  await read.trigger("change");
  await flushPromises();
  expect((await mock.appSettings()).feeds).toEqual({ checkMinutes: 360, keepReadDays: 14, keepUnreadDays: 90 });
  wrapper.unmount();

  setActivePinia(createPinia());
  wrapper = await openFeedSettings();
  expect(wrapper.get('[aria-label="How often feeds are checked"]').text()).toBe("Every 6 hours");
  expect(wrapper.get<HTMLInputElement>('input[aria-label="Days to keep read entries"]').element.value).toBe("14");
  wrapper.unmount();
});

it("imports a subscription list and says what was added and skipped, and exports one", async () => {
  const before = (await mock.feeds()).length;
  const wrapper = await openFeedSettings();
  await wrapper
    .findAll("button")
    .find((b) => b.text() === "Import…")!
    .trigger("click");
  await flushPromises();
  expect(wrapper.get(".feed-settings__result").text()).toBe(
    "Added 2 feeds. 1 skipped: already subscribed, or not at a web address.",
  );
  expect((await mock.feeds()).length).toBe(before + 2);
  await wrapper
    .findAll("button")
    .find((b) => b.text() === "Export…")!
    .trigger("click");
  await flushPromises();
  expect(wrapper.get(".feed-settings__result").text()).toBe(`Exported ${before + 2} feeds.`);
  wrapper.unmount();
});

/** Settings on the Sites tab, with the mock's browser `browser` (off, missing or ready). */
async function sitesTab(browser = "off") {
  mock.dispose();
  mock = createMockApi(`?browser=${browser}`);
  backend.api = mock;
  useNavigationStore().openSettings("sites");
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}
const browserRow = (wrapper: ReturnType<typeof mount>) => wrapper.get('[aria-label="Browser for fetching"]');
const button = (root: { findAll: ReturnType<typeof mount>["findAll"] }, text: string) =>
  root.findAll("button").find((b) => b.text() === text);

it("is off by default, with the browser and sites shown but not usable, until turned on", async () => {
  const wrapper = await sitesTab("off");
  expect(selected(wrapper).text()).toBe("Sites");
  expect(wrapper.get('[aria-label="Use a browser for articles"]').text()).toBe("Off");
  // Found, but not run while off: no version.
  expect(browserRow(wrapper).text()).toContain("/usr/bin/chromium");
  expect(browserRow(wrapper).text()).not.toContain("154.0");
  expect(button(browserRow(wrapper), "Change…")!.attributes("disabled")).toBeDefined();
  expect(wrapper.get('[aria-label="Sign out of harbour.example"]').attributes("disabled")).toBeDefined();

  await choose(
    wrapper,
    "Use a browser for articles",
    "When plain fetching failsA page that comes back cut short or behind a check is opened in a browser on this computer",
  );
  expect((await mock.appSettings()).browser.use).toBe("fallback");
  expect(browserRow(wrapper).text()).toContain("Chromium 154.0.8037.97");
  expect(wrapper.get('[aria-label="Sign out of harbour.example"]').attributes("disabled")).toBeUndefined();
  wrapper.unmount();
});

it("downloads Chrome when none is found, showing progress, and removes it after confirming", async () => {
  const host = mount(UiHost, { attachTo: document.body });
  const wrapper = await sitesTab("missing");
  expect(browserRow(wrapper).text()).toContain("No Chrome or Chromium was found.");
  expect(browserRow(wrapper).text()).toContain("Chrome for Testing 157.0.8103.78 from Google");
  vi.useFakeTimers();
  await button(browserRow(wrapper), "Download Chrome")!.trigger("click");
  await vi.advanceTimersByTimeAsync(200);
  expect(browserRow(wrapper).text()).toContain("Downloading Chrome 157.0.8103.78");
  expect(button(browserRow(wrapper), "Cancel download")).toBeDefined();
  await vi.advanceTimersByTimeAsync(800);
  vi.useRealTimers();
  await flushPromises();
  expect(browserRow(wrapper).text()).toContain("Chrome, downloaded by Aoide · 157.0.8103.78");

  await flushPromises();
  // A native click: after the fake timers, `trigger` does not reach this button.
  (button(browserRow(wrapper), "Remove downloaded browser")!.element as HTMLButtonElement).click();
  await flushPromises();
  await host.get('[data-confirm="yes"]').trigger("click");
  await flushPromises();
  expect(browserRow(wrapper).text()).toContain("No Chrome or Chromium was found.");
  wrapper.unmount();
  host.unmount();
});

it("edits a site's data policy and route, signs out, forgets and removes all site data", async () => {
  const host = mount(UiHost, { attachTo: document.body });
  const wrapper = await sitesTab("ready");
  const setRule = vi.spyOn(mock, "setSiteRule");
  const rows = () => wrapper.findAll('[aria-label="Sites"] tbody tr').map((r) => r.text());
  expect(rows()[0]).toContain("harbour.example");
  expect(rows()[0]).toMatch(/Signed in/);
  expect(rows()[1]).toContain("Asked for a subscription");

  await choose(wrapper, "Site data for harbour.example", "Clear before each fetch");
  expect(setRule).toHaveBeenLastCalledWith({ host: "harbour.example", fetch: null, siteData: "clear_before_fetch" });
  await choose(wrapper, "How harbour.example is fetched", "Browser");
  expect(setRule).toHaveBeenLastCalledWith({
    host: "harbour.example",
    fetch: "browser",
    siteData: "clear_before_fetch",
  });
  await choose(wrapper, "How harbour.example is fetched", "Automatic");
  expect(setRule).toHaveBeenLastCalledWith({ host: "harbour.example", fetch: null, siteData: "clear_before_fetch" });

  await wrapper.get('[aria-label="Sign out of harbour.example"]').trigger("click");
  await flushPromises();
  expect(rows()[0]).toContain("No sign-in");
  await wrapper.get('[aria-label="Forget lantern.example"]').trigger("click");
  await flushPromises();
  await host.get('[data-confirm="yes"]').trigger("click");
  await flushPromises();
  expect(rows().some((r) => r.includes("lantern.example"))).toBe(false);

  const clear = vi.spyOn(mock, "clearBrowserData");
  await button(wrapper, "Remove all site data")!.trigger("click");
  await flushPromises();
  await host.get('[data-confirm="no"]').trigger("click");
  await flushPromises();
  expect(clear).not.toHaveBeenCalled();
  await button(wrapper, "Remove all site data")!.trigger("click");
  await flushPromises();
  await host.get('[data-confirm="yes"]').trigger("click");
  await flushPromises();
  expect(clear).toHaveBeenCalled();
  expect(useUiStore().toasts.map((t) => t.text)).toContain("Removed all site data.");
  wrapper.unmount();
  host.unmount();
});
