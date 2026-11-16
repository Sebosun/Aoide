import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import App from "@/App.vue";
import FindAudiobooksView from "@/views/FindAudiobooksView.vue";
import SettingsDialog from "@/components/settings/SettingsDialog.vue";
import { useLibraryStore } from "@/stores/library";
import { useNavigationStore } from "@/stores/navigation";
import { useUiStore } from "@/stores/ui";
import { pickMenuItem } from "@/tests/helpers";

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

const titles = (wrapper: ReturnType<typeof mount>) => wrapper.findAll(".find-audiobooks__card h3").map((h) => h.text());
async function scanned() {
  await useLibraryStore().start();
  const wrapper = mount(FindAudiobooksView, { attachTo: document.body });
  await flushPromises();
  await wrapper.get('[aria-label="Scan for audiobooks"]').trigger("click");
  await flushPromises();
  return wrapper;
}

it("is reached from the Add dialog", async () => {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  await wrapper.get('nav [aria-label="Add"]').trigger("click");
  await flushPromises();
  const find = [...document.body.querySelectorAll<HTMLElement>('[role="dialog"] button')].find((b) =>
    b.textContent?.includes("Find audiobooks"),
  )!;
  find.click();
  await flushPromises();
  expect(useNavigationStore().view).toBe("audiobooks");
  expect(wrapper.find(".find-audiobooks").exists()).toBe(true);
  wrapper.unmount();
});

it("lists the audiobooks found for each book, closest first, and pairs the ones the user picks", async () => {
  const pair = vi.spyOn(mock, "pairAudiobooks");
  const wrapper = await scanned();
  expect(titles(wrapper)).toEqual(["A Map of Quiet Places", "An Unfinished Atlas"]);
  const map = wrapper.findAll(".find-audiobooks__card")[0];
  expect(map.get(".find-audiobooks__eyebrow").text()).toBe("Same name");
  expect(map.findAll(".find-audiobooks__candidate").map((c) => c.text())).toEqual([
    "Ellis Reed - A Map of Quiet PlacesFolder · 15 files · 1.2 GB",
    "A Map of Quiet Places (Unabridged).m4b1 file · 597.0 MB",
  ]);
  expect(map.get(".find-audiobooks__candidate").attributes("title")).toContain(
    "~/Audiobooks/Ellis Reed - A Map of Quiet Places/15 - A Map of Quiet Places.m4b",
  );
  const atlas = wrapper.findAll(".find-audiobooks__card")[1];
  expect(atlas.get(".find-audiobooks__eyebrow").text()).toBe("Similar name");
  expect(atlas.get(".find-audiobooks__candidate").text()).toBe("Unfinished Atlas 1.mp32 files · 410.0 MB");

  await map.get('[aria-label="Pair A Map of Quiet Places (Unabridged).m4b with A Map of Quiet Places"]').setValue(true);
  await map.get('[aria-label="Select A Map of Quiet Places"]').setValue(true);
  await wrapper.get('[aria-label="Pair selected audiobooks"]').trigger("click");
  await flushPromises();

  expect(pair).toHaveBeenCalledWith({ choices: [{ bookId: "book-1", candidate: 1 }] });
  expect(titles(wrapper)).toEqual(["An Unfinished Atlas"]);
  await wrapper.get('[aria-label="Show paired books"]').trigger("click");
  expect(titles(wrapper)).toEqual(["A Map of Quiet Places"]);
  expect(wrapper.text()).toContain("A Map of Quiet Places (Unabridged).m4b");
  wrapper.unmount();
});

it("pairs every book's closest audiobook at once", async () => {
  const pair = vi.spyOn(mock, "pairAudiobooks");
  const wrapper = await scanned();
  await wrapper.get('[aria-label="Pair all found audiobooks"]').trigger("click");
  await flushPromises();
  expect(pair).toHaveBeenCalledWith({
    choices: [
      { bookId: "book-1", candidate: 0 },
      { bookId: "book-3", candidate: 0 },
    ],
  });
  expect(wrapper.get('[aria-label="Show paired books"]').text()).toContain("2");
  wrapper.unmount();
});

it("with verifying on, says Aoide checks the matches itself and lists them as being checked", async () => {
  await mock.updateAppSettings({ patch: { verifyAudiobooks: true } });
  const pair = vi.spyOn(mock, "pairAudiobooks");
  const wrapper = await scanned();
  expect(wrapper.text()).toContain("Aoide pairs each book's closest audiobook");
  expect(wrapper.get('[aria-label="Show books being checked"]').text()).toContain("2");
  expect(pair).not.toHaveBeenCalled();
  await wrapper.get('[aria-label="Show books being checked"]').trigger("click");
  expect(wrapper.findAll(".find-audiobooks__card")[0].text()).toContain("Checking Ellis Reed - A Map of Quiet Places");
  wrapper.unmount();
});

it("opens Find audiobooks after adding books whose audiobooks wait for the user, and not otherwise", async () => {
  await useLibraryStore().start();
  vi.spyOn(mock, "importFiles").mockResolvedValue([{ path: "~/Downloads/map.epub", bookId: "book-1", error: null }]);
  const look = vi.spyOn(mock, "lookForAudiobooks");
  await useLibraryStore().importFiles(["~/Downloads/map.epub"]);
  expect(look).toHaveBeenCalledWith({ ids: ["book-1"] });
  expect(useNavigationStore().view).toBe("audiobooks");

  useNavigationStore().view = "library";
  look.mockResolvedValueOnce(0);
  await useLibraryStore().importFiles(["~/Downloads/map.epub"]);
  expect(useNavigationStore().view).toBe("library");
});

it("turns verifying on from Settings → Recordings", async () => {
  useNavigationStore().openSettings("recordings");
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  await wrapper.get('[aria-label="Let Aoide verify audiobook matches"]').setValue(true);
  await flushPromises();
  expect((await mock.appSettings()).verifyAudiobooks).toBe(true);
  wrapper.unmount();
});

it("lists audiobooks that match no book, and adds one on its own", async () => {
  const wrapper = await scanned();
  const section = wrapper.get('[aria-labelledby="find-unmatched"]');
  expect(section.text()).toContain("Ines Morrow - Winter Radio");
  expect(section.text()).toContain("Folder · 2 files");
  await section.get('[aria-label="Add Ines Morrow - Winter Radio on its own"]').trigger("click");
  await flushPromises();
  expect(useLibraryStore().books.find((b) => b.id === "book-5")?.kind).toBe("audiobook");
  expect(useUiStore().toasts.map((t) => t.text)).toContain("Added “Winter Radio” as an audiobook");
  expect(wrapper.find('[aria-labelledby="find-unmatched"]').exists(), "added, so no longer listed").toBe(false);
  wrapper.unmount();
});
