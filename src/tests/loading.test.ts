import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "../api/mock";
import LibraryView from "../views/LibraryView.vue";
import BookDetailsView from "../views/BookDetailsView.vue";
import StatsView from "../views/StatsView.vue";
import { useLibraryStore } from "../stores/library";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("../api", () => ({ get api() { return backend.api; } }));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => { setActivePinia(createPinia()); mock = createMockApi(); backend.api = mock; });
afterEach(() => { mock.dispose(); document.body.innerHTML = ""; });

/** Holds back one API call until `release()`. */
function hold<K extends "listBooks" | "bookOverview" | "listeningStats">(method: K) {
  const real = mock[method].bind(mock) as (...args: unknown[]) => Promise<unknown>;
  let release = () => {};
  const gate = new Promise<void>(resolve => { release = resolve; });
  vi.spyOn(mock, method).mockImplementation((async (...args: unknown[]) => { await gate; return real(...args); }) as never);
  return async () => { release(); await flushPromises(); };
}
const placeholder = (wrapper: ReturnType<typeof mount>, label: string) => wrapper.find(`[role="status"][aria-label="${label}"]`);

it("shows placeholder cards while the library loads, not the empty library message", async () => {
  const release = hold("listBooks");
  void useLibraryStore().start();
  const wrapper = mount(LibraryView, { attachTo: document.body });
  await flushPromises();
  expect(placeholder(wrapper, "Loading your library").exists()).toBe(true);
  expect(wrapper.text()).not.toContain("Your library is empty");
  expect(wrapper.findAll(".book-card").length).toBe(0);
  await release();
  expect(placeholder(wrapper, "Loading your library").exists()).toBe(false);
  expect(wrapper.findAll(".book-card").length).toBe(4);
  wrapper.unmount();
});

it("says the library is empty once it has loaded with no books", async () => {
  vi.spyOn(mock, "listBooks").mockResolvedValue([]);
  await useLibraryStore().start();
  const wrapper = mount(LibraryView, { attachTo: document.body });
  await flushPromises();
  expect(wrapper.text()).toContain("Your library is empty");
  expect(placeholder(wrapper, "Loading your library").exists()).toBe(false);
  wrapper.unmount();
});

it("shows the book's title at once and placeholders for its details until they load", async () => {
  await useLibraryStore().start();
  const release = hold("bookOverview");
  const wrapper = mount(BookDetailsView, { props: { bookId: "book-1" }, attachTo: document.body });
  await flushPromises();
  expect(wrapper.get("h1").text()).toBe("A Map of Quiet Places");
  expect(placeholder(wrapper, "Loading book details").exists()).toBe(true);
  expect(wrapper.find('[aria-label="Contents"]').exists()).toBe(false);
  await release();
  expect(placeholder(wrapper, "Loading book details").exists()).toBe(false);
  expect(wrapper.find('[aria-label="Contents"]').exists()).toBe(true);
  wrapper.unmount();
});

it("shows placeholders while listening stats load, and the error if they fail", async () => {
  const release = hold("listeningStats");
  const wrapper = mount(StatsView, { attachTo: document.body });
  await flushPromises();
  expect(placeholder(wrapper, "Loading listening stats").exists()).toBe(true);
  await release();
  expect(placeholder(wrapper, "Loading listening stats").exists()).toBe(false);
  expect(wrapper.get('[aria-label="Listened"]').text()).toContain("Listened");
  wrapper.unmount();

  vi.spyOn(mock, "listeningStats").mockRejectedValue(new Error("stats unavailable"));
  const failed = mount(StatsView, { attachTo: document.body });
  await flushPromises();
  expect(placeholder(failed, "Loading listening stats").exists()).toBe(false);
  expect(failed.get('[role="alert"]').text()).toContain("stats unavailable");
  failed.unmount();
});
