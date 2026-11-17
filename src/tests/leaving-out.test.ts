import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import ItemView from "@/views/ItemView.vue";
import ReaderView from "@/views/ReaderView.vue";
import { useLibraryStore } from "@/stores/library";
import { useReaderStore } from "@/stores/reader";
import { usePlayerStore } from "@/stores/player";
import { openMenu } from "@/tests/helpers";

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

async function itemPage(bookId = "book-1") {
  await useLibraryStore().start();
  const wrapper = mount(ItemView, { props: { bookId }, attachTo: document.body });
  await flushPromises();
  return wrapper;
}
const rows = (wrapper: Awaited<ReturnType<typeof itemPage>>) => wrapper.findAll('[aria-label="Contents"] li');

it("leaves the front and back matter out in one click, and says which", async () => {
  const leave = vi.spyOn(mock, "setLeftOut");
  const wrapper = await itemPage();
  expect(wrapper.find('[aria-label^="Narrate chapter"]').exists()).toBe(false);
  await wrapper.get('[aria-label="Choose what is narrated"]').trigger("click");
  expect(wrapper.findAll('[aria-label^="Narrate chapter"]')).toHaveLength(4);
  const offer = wrapper.get('[aria-label="Leave out front and back matter"]');
  expect(wrapper.text()).toContain("About the author");

  await offer.trigger("click");
  await flushPromises();
  expect(leave).toHaveBeenCalledWith({ bookId: "book-1", chapters: [3] });
  expect(wrapper.get('[role="status"].book-contents__done').text()).toContain("Left out: About the author");
  const last = rows(wrapper)[3];
  expect(last.classes()).toContain("book-contents__item--left-out");
  expect(last.text()).toContain("Not narrated");
  expect((last.get('[aria-label="Narrate chapter 4: About the author"]').element as HTMLInputElement).checked).toBe(
    false,
  );
  expect(wrapper.find('[aria-label="Leave out front and back matter"]').exists()).toBe(false);
  expect(wrapper.text()).toContain("1 chapter left out");
  wrapper.unmount();
});

it("a chapter is left out or narrated again by its box", async () => {
  const leave = vi.spyOn(mock, "setLeftOut");
  const wrapper = await itemPage();
  await wrapper.get('[aria-label="Choose what is narrated"]').trigger("click");
  await wrapper.get('[aria-label="Narrate chapter 1: An ordinary discovery"]').setValue(false);
  await flushPromises();
  expect(leave).toHaveBeenLastCalledWith({ bookId: "book-1", chapters: [0] });
  expect(rows(wrapper)[0].text()).toContain("Not narrated");
  await wrapper.get('[aria-label="Narrate chapter 1: An ordinary discovery"]').setValue(true);
  await flushPromises();
  expect(leave).toHaveBeenLastCalledWith({ bookId: "book-1", chapters: [] });
  expect(rows(wrapper)[0].text()).not.toContain("Not narrated");
  expect(wrapper.text()).not.toContain("chapter left out");

  await wrapper.get('[aria-label="Done choosing what is narrated"]').trigger("click");
  expect(wrapper.find('[aria-label^="Narrate chapter"]').exists()).toBe(false);
  wrapper.unmount();
});

it("marks a chapter left out in the reader's chapter menu", async () => {
  await mock.setLeftOut({ bookId: "book-1", chapters: [3] });
  await useLibraryStore().start();
  await useReaderStore().open("book-1");
  await usePlayerStore().start();
  const wrapper = mount(ReaderView, { attachTo: document.body });
  await flushPromises();
  await wrapper.get('[aria-label$="choose a chapter"]').trigger("click");
  await flushPromises();
  const items = [...openMenu()!.querySelectorAll<HTMLElement>('[role^="menuitem"]')];
  expect(items[3].getAttribute("aria-label")).toBe("Chapter 4: About the author, not narrated");
  expect(items[3].textContent).toContain("Not narrated");
  expect(items[2].textContent).not.toContain("Not narrated");
  wrapper.unmount();
});
