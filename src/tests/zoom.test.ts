import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "../api/mock";
import App from "../App.vue";
import SettingsDialog from "../components/settings/SettingsDialog.vue";
import { usePreferencesStore } from "../stores/preferences";
import { useUiStore } from "../stores/ui";
import { stepZoom } from "../zoom";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("../api", () => ({ get api() { return backend.api; } }));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => { setActivePinia(createPinia()); mock = createMockApi(); backend.api = mock; });
afterEach(() => { mock.dispose(); document.documentElement.style.removeProperty("zoom"); document.body.innerHTML = ""; });

const shown = () => document.documentElement.style.getPropertyValue("zoom");
const press = (key: string, init: KeyboardEventInit = { ctrlKey: true }) => {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init });
  (document.activeElement ?? document.body).dispatchEvent(event);
  return event;
};

it("steps through fixed sizes and stops at the ends, also from a hand-written value", () => {
  expect(stepZoom(1, 1)).toBe(1.1);
  expect(stepZoom(1, -1)).toBe(0.9);
  expect(stepZoom(1.5, 1)).toBe(1.5);
  expect(stepZoom(0.67, -1)).toBe(0.67);
  expect(stepZoom(1.2, 1)).toBe(1.25);
  expect(stepZoom(1.2, -1)).toBe(1.1);
});

it("applies the saved interface size on start and when the config file changes, within limits", async () => {
  mock.emitConfig({ appearance: { zoom: 1.25 } });
  await usePreferencesStore().start();
  expect(shown()).toBe("1.25");
  mock.emitConfig({ appearance: { zoom: 9 } });
  await flushPromises();
  expect(shown()).toBe("1.5");
  mock.emitConfig({ appearance: { zoom: 1 } });
  await flushPromises();
  expect(shown()).toBe("1");
});

it("zooms with Ctrl and plus, minus or zero, saves the size and says it", async () => {
  const save = vi.spyOn(mock, "saveAppearance");
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  expect(press("=").defaultPrevented).toBe(true);
  await flushPromises();
  expect(shown()).toBe("1.1");
  expect(save).toHaveBeenLastCalledWith(expect.objectContaining({ zoom: 1.1 }));
  press("+", { ctrlKey: true, shiftKey: true });
  await flushPromises();
  expect(shown()).toBe("1.25");
  // One message, replaced as the size changes.
  expect(useUiStore().toasts.map(t => t.text)).toEqual(["Zoom 125%"]);
  press("-");
  press("-");
  press("-");
  await flushPromises();
  expect(shown()).toBe("0.9");
  press("0");
  await flushPromises();
  expect(shown()).toBe("1");
  expect(save).toHaveBeenLastCalledWith(expect.objectContaining({ zoom: 1 }));
  // Without Ctrl the keys are left alone.
  expect(press("=", {}).defaultPrevented).toBe(false);
  expect(shown()).toBe("1");
  wrapper.unmount();
});

it("zooms with Ctrl and the wheel, one step per notch", async () => {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  const wheel = (deltaY: number, ctrlKey = true) => { const event = new WheelEvent("wheel", { deltaY, ctrlKey, bubbles: true, cancelable: true }); document.body.dispatchEvent(event); return event; };
  expect(wheel(-120).defaultPrevented).toBe(true);
  await flushPromises();
  expect(shown()).toBe("1.1");
  wheel(120);
  wheel(120);
  await flushPromises();
  expect(shown()).toBe("0.9");
  expect(wheel(120, false).defaultPrevented).toBe(false);
  expect(shown()).toBe("0.9");
  wrapper.unmount();
});

it("changes the interface size from Settings", async () => {
  const save = vi.spyOn(mock, "saveAppearance");
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  const row = wrapper.get('[role="group"][aria-label="Interface size"]');
  expect(row.text()).toContain("100%");
  await row.get('[aria-label="Larger interface"]').trigger("click");
  await flushPromises();
  expect(row.text()).toContain("110%");
  expect(save).toHaveBeenLastCalledWith(expect.objectContaining({ zoom: 1.1 }));
  await row.get('[aria-label="Smaller interface"]').trigger("click");
  await row.get('[aria-label="Smaller interface"]').trigger("click");
  await flushPromises();
  expect(row.text()).toContain("90%");
  await row.get('[aria-label="Reset interface size to 100%"]').trigger("click");
  await flushPromises();
  expect(shown()).toBe("1");
  expect(row.get('[aria-label="Reset interface size to 100%"]').attributes("disabled")).toBeDefined();
  wrapper.unmount();
});
