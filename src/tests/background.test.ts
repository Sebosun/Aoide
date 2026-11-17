import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import App from "@/App.vue";
import { useNavigationStore } from "@/stores/navigation";

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

async function app() {
  useNavigationStore().view = "library";
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}
const question = () => document.body.querySelector<HTMLElement>('[aria-label="Keep working in the background?"]');
const press = async (label: string) => {
  question()!.querySelector<HTMLElement>(`[aria-label="${label}"]`)!.click();
  await flushPromises();
};

it("asks whether to keep working when the window is closed while working, and remembers", async () => {
  const choose = vi.spyOn(mock, "chooseClose");
  const wrapper = await app();
  expect(question()).toBeNull();
  mock.emitCloseAsked();
  await flushPromises();
  expect(question()?.textContent).toContain("carry on with the window closed");
  const remember = question()!.querySelector<HTMLInputElement>('[aria-label="Remember my choice"]')!;
  remember.click();
  await press("Keep working");
  expect(choose).toHaveBeenLastCalledWith({ keep: true, remember: true });
  expect(question()).toBeNull();

  mock.emitCloseAsked();
  await flushPromises();
  await press("Quit");
  expect(choose).toHaveBeenLastCalledWith({ keep: false, remember: false });
  wrapper.unmount();
});
