import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import App from "@/App.vue";
import { useNavigationStore } from "@/stores/navigation";
import { useReaderStore } from "@/stores/reader";

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
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
});
afterEach(() => {
  vi.useRealTimers();
  mock.dispose();
  document.body.innerHTML = "";
});

async function app() {
  useNavigationStore().view = "library";
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}
async function search(wrapper: ReturnType<typeof mount>, words: string) {
  await wrapper.get('[aria-label="Search library"]').setValue(words);
  await vi.advanceTimersByTimeAsync(400);
  await flushPromises();
}

it("finds passages in the library's text and opens the reader there without playing", async () => {
  const play = vi.spyOn(mock, "play");
  const wrapper = await app();
  await search(wrapper, "Sparrow");
  const text = wrapper.get('[aria-label="In the text"]');
  const passage = text.get("button.text-search__passage");
  expect(passage.text()).toContain("a sparrow was rehearsing");
  expect(passage.findAll("mark").map((m) => m.text())).toEqual(["sparrow"]);
  // Titled by the item it is in.
  expect(text.text()).toContain("A Map of Quiet Places");
  await passage.trigger("click");
  await flushPromises();
  expect(useNavigationStore().view).toBe("reader");
  const reader = useReaderStore();
  expect(reader.book?.id).toBe("book-1");
  expect(reader.place).toEqual({ chapter: 0, segmentId: "0:2:0" });
  expect(wrapper.find('mark[data-highlight-id="-1"]').text()).toBe("sparrow");
  expect(play).not.toHaveBeenCalled();
  wrapper.unmount();
});

it("says when the text holds none of it, and searches no text when turned off", async () => {
  const wrapper = await app();
  await search(wrapper, "zeppelin");
  expect(wrapper.get('[aria-label="In the text"]').text()).toContain("Nothing in the text");
  await mock.updateAppSettings({ patch: { searchText: false } });
  const searchLibrary = vi.spyOn(mock, "searchLibrary");
  wrapper.unmount();
  const again = await app();
  await search(again, "sparrow");
  expect(again.find('[aria-label="In the text"]').exists()).toBe(false);
  expect(searchLibrary).not.toHaveBeenCalled();
  again.unmount();
});

it("goes to the library's search with Ctrl Shift F from anywhere", async () => {
  useNavigationStore().view = "home";
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  const event = new KeyboardEvent("keydown", {
    key: "F",
    ctrlKey: true,
    shiftKey: true,
    bubbles: true,
    cancelable: true,
  });
  document.body.dispatchEvent(event);
  await flushPromises();
  expect(event.defaultPrevented).toBe(true);
  expect(useNavigationStore().view).toBe("library");
  expect((document.activeElement as HTMLElement).getAttribute("aria-label")).toBe("Search library");
  wrapper.unmount();
});
