import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { defineComponent, h, ref } from "vue";
import { mount, flushPromises } from "@vue/test-utils";
import UiSelect from "../components/ui/UiSelect.vue";
import UiMenu from "../components/ui/UiMenu.vue";
import type { MenuEntry } from "../components/ui/types";
import UiHost from "../components/ui/UiHost.vue";
import { useUiStore } from "../stores/ui";
import { choose, optionLabels } from "./helpers";

beforeEach(() => { setActivePinia(createPinia()); });
afterEach(() => { vi.useRealTimers(); document.body.innerHTML = ""; });

const FRUIT = [
  { value: "a", label: "Apple" },
  { value: "b", label: "Banana" },
  { value: "c", label: "Cherry", disabled: true },
  { value: "d", label: "Damson" },
  { value: "e", label: "Date" },
];
function fruit(initial = "b") {
  const value = ref(initial);
  const Host = defineComponent(() => () => h("div", [h(UiSelect, { modelValue: value.value, "onUpdate:modelValue": (v: string | number) => { value.value = String(v); }, options: FRUIT, label: "Fruit" }), h("button", "after")]));
  return { value, wrapper: mount(Host, { attachTo: document.body }) };
}
const listbox = () => document.body.querySelector<HTMLElement>('[role="listbox"]');
const active = () => document.getElementById(listbox()?.getAttribute("aria-activedescendant") ?? "")?.textContent?.trim();
const key = (key: string) => { listbox()!.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true })); return flushPromises(); };

it("shows the chosen option and opens a listbox on the selected one", async () => {
  const { wrapper } = fruit();
  const button = wrapper.get('[aria-label="Fruit"]');
  expect(button.attributes("aria-haspopup")).toBe("listbox");
  expect(button.attributes("aria-expanded")).toBe("false");
  expect(button.text()).toBe("Banana");
  await button.trigger("click");
  await flushPromises();
  expect(button.attributes("aria-expanded")).toBe("true");
  expect(optionLabels()).toEqual(["Apple", "Banana", "Cherry", "Damson", "Date"]);
  expect(listbox()!.querySelector('[aria-selected="true"]')?.textContent?.trim()).toBe("Banana");
  expect(document.activeElement).toBe(listbox());
  expect(active()).toBe("Banana");
  wrapper.unmount();
});

it("moves with the arrows, Home and End, skips disabled options, and picks with Enter", async () => {
  const { wrapper, value } = fruit();
  const button = wrapper.get('[aria-label="Fruit"]');
  await button.trigger("click");
  await flushPromises();
  await key("ArrowDown");
  expect(active()).toBe("Damson");
  await key("End");
  expect(active()).toBe("Date");
  await key("Home");
  expect(active()).toBe("Apple");
  await key("ArrowUp");
  expect(active()).toBe("Apple");
  await key("Enter");
  expect(value.value).toBe("a");
  expect(listbox()).toBeNull();
  expect(document.activeElement).toBe(button.element);
  expect(button.text()).toBe("Apple");
  wrapper.unmount();
});

it("jumps by typing, and Escape closes without changing the value", async () => {
  const { wrapper, value } = fruit();
  const button = wrapper.get('[aria-label="Fruit"]');
  await button.trigger("keydown", { key: "ArrowDown" });
  await flushPromises();
  expect(listbox()).not.toBeNull();
  await key("d");
  expect(active()).toBe("Damson");
  await key("a");
  await key("t");
  expect(active()).toBe("Date");
  await key("Escape");
  expect(listbox()).toBeNull();
  expect(value.value).toBe("b");
  expect(document.activeElement).toBe(button.element);
  wrapper.unmount();
});

it("picks an option with a click, and ignores disabled ones", async () => {
  const { wrapper, value } = fruit();
  await wrapper.get('[aria-label="Fruit"]').trigger("click");
  await flushPromises();
  [...document.body.querySelectorAll<HTMLElement>('[role="option"]')].find(o => o.textContent?.includes("Cherry"))!.click();
  await flushPromises();
  expect(value.value).toBe("b");
  await choose(wrapper, "Fruit", "Date");
  expect(value.value).toBe("e");
  wrapper.unmount();
});

it("asks for confirmation in a modal and resolves with the answer", async () => {
  const host = mount(UiHost, { attachTo: document.body });
  const ui = useUiStore();
  const opener = document.createElement("button");
  document.body.append(opener);
  opener.focus();
  const answer = ui.confirm({ title: "Remove The Lantern Keeper?", body: "The EPUB file stays on disk.", confirmLabel: "Remove", danger: true });
  await flushPromises();
  const dialog = host.get('[role="alertdialog"]');
  expect(dialog.attributes("aria-modal")).toBe("true");
  expect(dialog.text()).toContain("Remove The Lantern Keeper?");
  expect(dialog.text()).toContain("The EPUB file stays on disk.");
  expect(document.activeElement?.textContent?.trim()).toBe("Cancel");
  await dialog.get('[data-confirm="yes"]').trigger("click");
  await expect(answer).resolves.toBe(true);
  await flushPromises();
  expect(host.find('[role="alertdialog"]').exists()).toBe(false);
  expect(document.activeElement).toBe(opener);

  const second = ui.confirm({ title: "Delete audio?", confirmLabel: "Delete" });
  await flushPromises();
  await host.get('[role="alertdialog"]').trigger("keydown", { key: "Escape" });
  await expect(second).resolves.toBe(false);
  host.unmount();
});

it("shows short messages as toasts that fade after a few seconds", async () => {
  vi.useFakeTimers();
  const host = mount(UiHost, { attachTo: document.body });
  useUiStore().toast("Bookmarked");
  await flushPromises();
  expect(host.get('[role="status"]').text()).toContain("Bookmarked");
  vi.advanceTimersByTime(4000);
  await flushPromises();
  expect(host.get('[role="status"]').text()).not.toContain("Bookmarked");
  host.unmount();
});

function menu() {
  const picked: string[] = [], checked = ref(false);
  const Host = defineComponent(() => () => h(UiMenu, {
    label: "Actions",
    items: [
      { id: "open", label: "Open", action: () => picked.push("open") },
      { id: "locked", label: "Locked", disabled: true, action: () => picked.push("locked") },
      { kind: "separator" },
      { kind: "heading", label: "Shelves" },
      { id: "night", label: "Night reading", checked: checked.value, action: () => { checked.value = !checked.value; picked.push("night"); } },
      { kind: "separator" },
      { id: "remove", label: "Remove", ariaLabel: "Remove the book", danger: true, action: () => picked.push("remove") },
      { kind: "note", label: "Or drop files here." },
    ] satisfies MenuEntry[],
  }, { default: ({ toggle, trigger }: { toggle: () => void; trigger: Record<string, unknown> }) => h("button", { ...trigger, "aria-label": "More", onClick: toggle }, "More") }));
  const wrapper = mount(Host, { attachTo: document.body });
  return { wrapper, picked, button: wrapper.get('[aria-label="More"]') };
}
const menuList = () => document.body.querySelector<HTMLElement>('[role="menu"]');
const menuKey = (key: string) => { document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true })); return flushPromises(); };

it("opens a menu from its button with items, a checkable item, groups and a note", async () => {
  const { wrapper, picked, button } = menu();
  expect(button.attributes("aria-haspopup")).toBe("menu");
  expect(button.attributes("aria-expanded")).toBe("false");
  expect(menuList()).toBeNull();
  await button.trigger("click");
  await flushPromises();
  expect(button.attributes("aria-expanded")).toBe("true");
  expect(menuList()!.getAttribute("aria-label")).toBe("Actions");
  expect([...menuList()!.querySelectorAll('[role="menuitem"]')].map(i => i.textContent?.trim())).toEqual(["Open", "Locked", "Remove"]);
  expect(menuList()!.querySelector('[role="group"][aria-label="Shelves"] [role="menuitemcheckbox"]')!.getAttribute("aria-checked")).toBe("false");
  expect(menuList()!.querySelector('[aria-label="Remove the book"]')).not.toBeNull();
  expect(menuList()!.textContent).toContain("Or drop files here.");

  menuList()!.querySelector<HTMLElement>('[role="menuitemcheckbox"]')!.click();
  await flushPromises();
  expect(picked).toEqual(["night"]);
  expect(menuList()).toBeNull();
  expect(document.activeElement).toBe(button.element);
  await button.trigger("click");
  await flushPromises();
  expect(menuList()!.querySelector('[role="menuitemcheckbox"]')!.getAttribute("aria-checked")).toBe("true");
  wrapper.unmount();
});

it("moves through a menu with the arrow keys, skipping disabled items, and closes on Escape or a click outside", async () => {
  const { wrapper, picked, button } = menu();
  await button.trigger("click");
  await flushPromises();
  expect(document.activeElement?.textContent?.trim()).toBe("Open");
  await menuKey("ArrowDown");
  expect(document.activeElement?.textContent?.trim()).toBe("Night reading");
  await menuKey("ArrowUp");
  await menuKey("ArrowUp");
  expect(document.activeElement?.textContent?.trim()).toBe("Remove");
  await menuKey("Escape");
  expect(menuList()).toBeNull();
  expect(document.activeElement).toBe(button.element);

  await button.trigger("click");
  await flushPromises();
  document.body.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  await flushPromises();
  expect(menuList()).toBeNull();
  expect(picked).toEqual([]);
  wrapper.unmount();
});
