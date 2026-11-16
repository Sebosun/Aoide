import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import LibraryView from "@/views/LibraryView.vue";
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
