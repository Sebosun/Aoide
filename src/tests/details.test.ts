import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "../api/mock";
import App from "../App.vue";
import { useNavigationStore } from "../stores/navigation";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("../api", () => ({ get api() { return backend.api; } }));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => { setActivePinia(createPinia()); mock = createMockApi(); backend.api = mock; });
afterEach(() => { mock.dispose(); document.body.innerHTML = ""; });

async function app() {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}
const page = (wrapper: ReturnType<typeof mount>) => wrapper.get('[aria-label="About A Map of Quiet Places"]');

it("opens an unread book on its details page, without playing", async () => {
  const play = vi.spyOn(mock, "play");
  const wrapper = await app();
  await wrapper.get('[aria-label="Open A Map of Quiet Places"]').trigger("click");
  await flushPromises();
  expect(useNavigationStore().view).toBe("book");
  const about = page(wrapper);
  expect(about.get("h1").text()).toBe("A Map of Quiet Places");
  expect(about.text()).toContain("Ellis Reed");
  expect(about.text()).toContain("An original story used in the browser preview.");
  const facts = Object.fromEntries(about.findAll(".book-page__fact").map(f => [f.get("dt").text(), f.get("dd").text()]));
  expect(facts.Chapters).toBe("3");
  expect(Number(facts.Words.replace(/\D/g, ""))).toBeGreaterThan(50);
  expect(facts["Listening time"]).toMatch(/at 1×$/);
  expect(facts.Language).toBe("en");
  expect(facts.Audio).toBe("Not prepared yet");
  expect(about.findAll('[aria-label="Contents"] li').map(li => li.text())).toEqual(["1An ordinary discovery", "2The long way home", "3A place to begin"]);
  expect(about.text()).toContain("Extraction notes");
  expect(play).not.toHaveBeenCalled();
  wrapper.unmount();
});

it("starts listening from the beginning, or from a chosen chapter", async () => {
  const play = vi.spyOn(mock, "play");
  const wrapper = await app();
  await wrapper.get('[aria-label="Open A Map of Quiet Places"]').trigger("click");
  await flushPromises();
  await page(wrapper).get('[aria-label="Start listening"]').trigger("click");
  await flushPromises();
  expect(play).toHaveBeenCalledWith("book-1", null, null);
  expect(useNavigationStore().view).toBe("reader");
  expect(wrapper.find('[aria-label="Chapter text"]').exists()).toBe(true);

  useNavigationStore().openBook("book-1");
  await flushPromises();
  await page(wrapper).get('[aria-label="Contents"] [aria-label="Play chapter 2: The long way home"]').trigger("click");
  await flushPromises();
  expect(play).toHaveBeenLastCalledWith("book-1", 1, "1:0:0");
  expect(useNavigationStore().view).toBe("reader");
  wrapper.unmount();
});

it("opens details from the card menu, and goes back to the library", async () => {
  const wrapper = await app();
  await wrapper.get('[aria-label="More for The Lantern Keeper"]').trigger("click");
  [...document.body.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(m => m.textContent?.trim() === "Details")!.click();
  await flushPromises();
  const about = wrapper.get('[aria-label="About The Lantern Keeper"]');
  expect(about.get('[aria-label="Resume listening"]').text()).toContain("Resume");
  expect(about.text()).toContain("12% listened");
  await about.get('[aria-label="Back to library"]').trigger("click");
  expect(useNavigationStore().view).toBe("library");
  wrapper.unmount();
});
