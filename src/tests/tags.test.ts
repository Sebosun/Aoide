import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import LibraryView from "@/views/LibraryView.vue";
import SettingsDialog from "@/components/settings/SettingsDialog.vue";
import { useNavigationStore } from "@/stores/navigation";
import { useLibraryStore } from "@/stores/library";
import { usePlayerStore } from "@/stores/player";
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
afterEach(() => mock.dispose());

async function library() {
  await useLibraryStore().start();
  await usePlayerStore().start();
  const wrapper = mount(LibraryView, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}
const titles = (wrapper: ReturnType<typeof mount>) => wrapper.findAll(".book-card__title").map((h) => h.text());

it("shows only the items with a tag, then leaves them out, and remembers it", async () => {
  const save = vi.spyOn(mock, "updateAppSettings");
  const wrapper = await library();
  const bar = () => wrapper.get('[role="group"][aria-label="Filter by tag"]');
  const chip = (name: string) =>
    bar()
      .findAll("button")
      .find((b) => b.text().startsWith(name))!;
  // The tags on the items shown, the most used first, each with how many have it.
  expect(
    bar()
      .findAll("button")
      .map((b) => b.text()),
  ).toEqual(["Fiction 3", "Literary 2", "Cities 1", "Lighthouses 1", "Sea Stories 1", "Travel 1"]);

  await chip("Fiction").trigger("click");
  await flushPromises();
  expect(chip("Fiction").attributes("aria-label")).toBe("Fiction: only these are shown");
  expect(titles(wrapper)).toEqual(
    expect.arrayContaining(["The Lantern Keeper", "A Map of Quiet Places", "The Absent Orchard"]),
  );
  expect(titles(wrapper)).not.toContain("An Unfinished Atlas");
  expect(save).toHaveBeenLastCalledWith({ patch: { libraryTags: { include: ["Fiction"], exclude: [] } } });

  await chip("Fiction").trigger("click");
  await flushPromises();
  expect(chip("Fiction").attributes("aria-label")).toBe("Fiction: left out");
  expect(titles(wrapper)).toEqual(["An Unfinished Atlas"]);
  expect(save).toHaveBeenLastCalledWith({ patch: { libraryTags: { include: [], exclude: ["Fiction"] } } });
  wrapper.unmount();

  // Started again, the library still leaves them out.
  setActivePinia(createPinia());
  const again = await library();
  expect(titles(again)).toEqual(["An Unfinished Atlas"]);
  const clear = again
    .get('[role="group"][aria-label="Filter by tag"]')
    .findAll("button")
    .find((b) => b.text().startsWith("Fiction"))!;
  await clear.trigger("click");
  await flushPromises();
  expect(clear.attributes("aria-label")).toBe("Fiction: not filtered");
  expect(titles(again)).toHaveLength(4);
  again.unmount();
});

it("edits an item's tags from its card", async () => {
  const wrapper = await library();
  await wrapper.get('[aria-label="More for The Lantern Keeper"]').trigger("click");
  await flushPromises();
  await pickMenuItem("Tags…");
  const dialog = () =>
    document.body.querySelector<HTMLElement>('[role="dialog"][aria-label="Tags for The Lantern Keeper"]')!;
  expect(dialog()).not.toBeNull();
  const shown = () => [...dialog().querySelectorAll(".tags-editor__tag")].map((t) => t.textContent?.trim());
  expect(shown()).toEqual(["Fiction", "Lighthouses", "Sea Stories"]);

  dialog().querySelector<HTMLButtonElement>('[aria-label="Remove the tag Lighthouses"]')!.click();
  await flushPromises();
  const field = dialog().querySelector<HTMLInputElement>('[aria-label="Add a tag"]')!;
  // Tags in use are offered.
  expect([...dialog().querySelectorAll("datalist option")].map((o) => (o as HTMLOptionElement).value)).toContain(
    "Travel",
  );
  field.value = "To read";
  field.dispatchEvent(new Event("input"));
  field.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
  await flushPromises();
  expect(shown()).toEqual(["Fiction", "Sea Stories", "To read"]);
  expect(useLibraryStore().books.find((b) => b.title === "The Lantern Keeper")!.tags).toEqual([
    "Fiction",
    "Sea Stories",
    "To read",
  ]);
  wrapper.unmount();
});

it("turns on tagging with a local model in Settings and tags the untagged items", async () => {
  const save = vi.spyOn(mock, "updateAppSettings");
  const run = vi.spyOn(mock, "tagUntaggedWithModel");
  useNavigationStore().openSettings("library");
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  const section = () => wrapper.get('[aria-label="Tagging with a model"]');
  // Off at first: nothing else to set.
  expect(section().find('[aria-label="Model to tag with"]').exists()).toBe(false);
  await section().get('[aria-label="Tag items with a model on this computer"]').setValue(true);
  await flushPromises();
  expect(save).toHaveBeenLastCalledWith({ patch: { tagging: expect.objectContaining({ enabled: true }) } });
  const model = section().get('[aria-label="Model to tag with"]');
  await model.setValue("qwen2.5:3b");
  await model.trigger("change");
  await flushPromises();
  expect(save).toHaveBeenLastCalledWith({
    patch: {
      tagging: expect.objectContaining({ enabled: true, model: "qwen2.5:3b", baseUrl: "http://127.0.0.1:11434" }),
    },
  });
  // One item without tags.
  const atlas = (await mock.listBooks()).find((b) => b.title === "An Unfinished Atlas")!;
  await mock.setBookTags({ bookId: atlas.id, tags: [] });
  await section().get('[aria-label="Tag the items that have no tags"]').trigger("click");
  await flushPromises();
  expect(run).toHaveBeenCalled();
  expect(section().text()).toContain("Tagged 1 item");
  wrapper.unmount();
});

it("asks the model for an item's tags from its tag editor once tagging is on", async () => {
  const wrapper = await library();
  const open = async () => {
    await wrapper.get('[aria-label="More for An Unfinished Atlas"]').trigger("click");
    await flushPromises();
    await pickMenuItem("Tags…");
  };
  await open();
  const dialog = () =>
    document.body.querySelector<HTMLElement>('[role="dialog"][aria-label="Tags for An Unfinished Atlas"]')!;
  expect(dialog().querySelector('[aria-label="Suggest tags with the model"]')).toBeNull();
  dialog().querySelector<HTMLButtonElement>('[aria-label="Close tags"]')!.click();
  await flushPromises();

  await mock.updateAppSettings({
    patch: { tagging: { enabled: true, baseUrl: "http://127.0.0.1:11434", model: "m", network: "loopback" } },
  });
  await open();
  dialog().querySelector<HTMLButtonElement>('[aria-label="Suggest tags with the model"]')!.click();
  await flushPromises();
  const shown = [...dialog().querySelectorAll(".tags-editor__tag")].map((t) => t.textContent?.trim());
  expect(shown).toContain("Travel");
  expect(shown).toContain("Maps");
  wrapper.unmount();
});
