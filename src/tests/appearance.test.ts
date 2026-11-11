import { beforeEach, afterEach, expect, it, vi } from "vitest";
import themeSource from "../styles/theme.css?raw";
import rustConfig from "../../crates/config/src/lib.rs?raw";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "../api/mock";
import { TOKENS, themeCss } from "../theme";
import { usePreferencesStore } from "../stores/preferences";
import SettingsDialog from "../components/settings/SettingsDialog.vue";

import { choose } from "./helpers";
const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("../api", () => ({ get api() { return backend.api; } }));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => { setActivePinia(createPinia()); mock = createMockApi(); backend.api = mock; });
afterEach(() => {
  mock.dispose();
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.style.removeProperty("--read-scale");
  document.getElementById("aoide-user-theme")?.remove();
});

it("keeps the themable tokens identical in Rust, TypeScript and theme.css", () => {
  const rust = rustConfig;
  const list = rust.slice(rust.indexOf("pub const TOKENS"), rust.indexOf("];", rust.indexOf("pub const TOKENS")));
  expect([...list.matchAll(/"([a-z-]+)"/g)].map(m => m[1])).toEqual([...TOKENS]);

  const css = themeSource;
  const blocks = [css.slice(css.indexOf(":root {"), css.indexOf("}")), css.slice(css.indexOf(':root[data-theme="dark"]'))];
  for (const block of blocks) {
    const declared = new Set([...block.matchAll(/--([a-z-]+):/g)].map(m => m[1]));
    for (const token of TOKENS) expect(declared, `--${token}`).toContain(token);
  }
});

it("turns a theme file into CSS that overrides only the given tokens, per variant", () => {
  expect(themeCss(undefined)).toBe("");
  const css = themeCss({ name: "dusk", light: { accent: "#123456" }, dark: { accent: "#ABCDEF", ground: "#000" }, problems: [] });
  expect(css).toContain(":root { --accent: #123456; }");
  expect(css).toContain('@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { --accent: #ABCDEF; --ground: #000; } }');
  expect(css).toContain(':root[data-theme="dark"] { --accent: #ABCDEF; --ground: #000; }');
  expect(css).not.toContain("undefined");
});

it("applies the saved mode, text size and theme to the document and follows live config changes", async () => {
  mock.emitConfig({ appearance: { mode: "dark", theme: "dusk", textSize: 4, readerMode: "page" } });
  const preferences = usePreferencesStore();
  await preferences.start();
  const root = document.documentElement;
  expect(root.getAttribute("data-theme")).toBe("dark");
  expect(Number(root.style.getPropertyValue("--read-scale"))).toBeGreaterThan(1);
  expect(document.getElementById("aoide-user-theme")?.textContent).toContain("--accent");

  mock.emitConfig({ appearance: { mode: "auto", theme: "default", textSize: 2, readerMode: "lines" } });
  await flushPromises();
  expect(root.hasAttribute("data-theme")).toBe(false);
  expect(root.style.getPropertyValue("--read-scale")).toBe("1");
  expect(document.getElementById("aoide-user-theme")?.textContent ?? "").toBe("");
  expect(preferences.appearance.readerMode).toBe("lines");
});

it("changes appearance from Settings and saves it to the config file", async () => {
  const save = vi.spyOn(mock, "saveAppearance");
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  await wrapper.get('[aria-label="Dark"]').trigger("click");
  await flushPromises();
  expect(save).toHaveBeenLastCalledWith(expect.objectContaining({ mode: "dark" }));
  expect(document.documentElement.getAttribute("data-theme")).toBe("dark");

  await choose(wrapper, "Colour theme", "dusk");
  await flushPromises();
  expect(save).toHaveBeenLastCalledWith(expect.objectContaining({ mode: "dark", theme: "dusk" }));
  expect(wrapper.text()).toContain("~/.config/aoide");
  wrapper.unmount();
});

it("shows problems in settings.toml and theme files", async () => {
  mock.emitConfig({ problems: ["settings.toml is not valid TOML: line 3"], themes: [{ name: "dusk", light: {}, dark: {}, problems: ["[light] sparkle: not a colour token Aoide knows"] }] });
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  expect(wrapper.text()).toContain("line 3");
  expect(wrapper.text()).toContain("not a colour token");
  wrapper.unmount();
});
