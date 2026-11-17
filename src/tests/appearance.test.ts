import { beforeEach, afterEach, expect, it, vi } from "vitest";
import themeSource from "@/styles/theme.css?raw";
import rustConfig from "../../crates/config/src/lib.rs?raw";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import { LINE_HEIGHTS, TEXT_WIDTHS, TOKENS, applyAppearance, themeCss } from "@/theme";
import type { Appearance } from "@/api/types";
import { usePreferencesStore } from "@/stores/preferences";
import { useLibraryStore } from "@/stores/library";
import SettingsDialog from "@/components/settings/SettingsDialog.vue";

import { choose } from "@/tests/helpers";
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
  document.documentElement.removeAttribute("data-theme");
  for (const property of [
    "--read-scale",
    "--read-line",
    "--read-width",
    "--read-font",
    "--read-align",
    "--read-hyphens",
  ])
    document.documentElement.style.removeProperty(property);
  document.getElementById("aoide-user-theme")?.remove();
});

it("keeps the themable tokens identical in Rust, TypeScript and theme.css", () => {
  const rust = rustConfig;
  const list = rust.slice(rust.indexOf("pub const TOKENS"), rust.indexOf("];", rust.indexOf("pub const TOKENS")));
  expect([...list.matchAll(/"([a-z-]+)"/g)].map((m) => m[1])).toEqual([...TOKENS]);

  const css = themeSource;
  const blocks = [
    css.slice(css.indexOf(":root {"), css.indexOf("}")),
    css.slice(css.indexOf(':root[data-theme="dark"]')),
  ];
  for (const block of blocks) {
    const declared = new Set([...block.matchAll(/--([a-z-]+):/g)].map((m) => m[1]));
    for (const token of TOKENS) expect(declared, `--${token}`).toContain(token);
  }
});

it("turns a theme file into CSS that overrides only the given tokens, per variant", () => {
  expect(themeCss(undefined)).toBe("");
  const css = themeCss({
    name: "dusk",
    title: "dusk",
    builtin: false,
    light: { accent: "#123456" },
    dark: { accent: "#ABCDEF", ground: "#000" },
    problems: [],
  });
  expect(css).toContain(":root { --accent: #123456; }");
  expect(css).toContain(
    '@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { --accent: #ABCDEF; --ground: #000; } }',
  );
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

const designed: Appearance = {
  mode: "auto",
  theme: "default",
  textSize: 2,
  readerMode: "page",
  zoom: 1,
  timeLeft: "chapter",
  readerFont: "serif",
  lineSpacing: 2,
  textWidth: 2,
  justify: false,
  hyphenate: false,
};

it("gives the reading text today's look at the designed steps", () => {
  const root = document.documentElement;
  applyAppearance(designed, []);
  expect(root.style.getPropertyValue("--read-line")).toBe("1.68");
  expect(root.style.getPropertyValue("--read-width")).toBe("640px");
  expect(root.style.getPropertyValue("--read-font")).toBe("var(--font-read)");
  expect(root.style.getPropertyValue("--read-align")).toBe("start");
  expect(root.style.getPropertyValue("--read-hyphens")).toBe("manual");
});

it("sets the reader's font, spacing, width, justification and hyphenation, keeping steps in range", () => {
  const root = document.documentElement;
  applyAppearance(
    { ...designed, readerFont: "legible", lineSpacing: 9, textWidth: -3, justify: true, hyphenate: true },
    [],
  );
  expect(root.style.getPropertyValue("--read-line")).toBe(String(LINE_HEIGHTS[LINE_HEIGHTS.length - 1]));
  expect(root.style.getPropertyValue("--read-width")).toBe(`${TEXT_WIDTHS[0]}px`);
  expect(root.style.getPropertyValue("--read-font")).toContain("Atkinson Hyperlegible Next");
  expect(root.style.getPropertyValue("--read-align")).toBe("justify");
  expect(root.style.getPropertyValue("--read-hyphens")).toBe("auto");

  // Spacing and width grow step by step.
  expect([...LINE_HEIGHTS]).toEqual([...LINE_HEIGHTS].sort((a, b) => a - b));
  expect([...TEXT_WIDTHS]).toEqual([...TEXT_WIDTHS].sort((a, b) => a - b));
  applyAppearance({ ...designed, readerFont: "sans" }, []);
  expect(root.style.getPropertyValue("--read-font")).toBe("var(--font-ui)");
  applyAppearance({ ...designed, readerFont: "system" }, []);
  expect(root.style.getPropertyValue("--read-font")).toContain("system-ui");
});

it("changes appearance from Settings and saves it to the config file", async () => {
  const save = vi.spyOn(mock, "saveAppearance");
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  await wrapper
    .get('[aria-label="Light or dark"]')
    .findAll("button")
    .find((b) => b.text() === "Dark")!
    .trigger("click");
  await flushPromises();
  expect(save).toHaveBeenLastCalledWith({ appearance: expect.objectContaining({ mode: "dark" }) });
  expect(document.documentElement.getAttribute("data-theme")).toBe("dark");

  await wrapper
    .get('[role="radiogroup"][aria-label="Colour theme"]')
    .findAll("label")
    .find((l) => l.text() === "dusk")!
    .get("input")
    .setValue(true);
  await flushPromises();
  expect(save).toHaveBeenLastCalledWith({ appearance: expect.objectContaining({ mode: "dark", theme: "dusk" }) });
  expect(wrapper.text()).toContain("~/.config/aoide");

  // The reader's typography, as in the reader's Text.
  await wrapper
    .get('[aria-label="Reader font"]')
    .findAll("button")
    .find((b) => b.text() === "Sans")!
    .trigger("click");
  await wrapper.get('[aria-label="Wider text"]').trigger("click");
  await flushPromises();
  expect(save).toHaveBeenLastCalledWith({ appearance: expect.objectContaining({ readerFont: "sans", textWidth: 3 }) });
  expect(document.documentElement.style.getPropertyValue("--read-width")).toBe("720px");
  wrapper.unmount();
});

it("offers the themes that come with Aoide, each with a swatch in its own colours", async () => {
  mock.emitConfig({ appearance: { mode: "dark", theme: "default", textSize: 2, readerMode: "page", zoom: 1 } });
  const save = vi.spyOn(mock, "saveAppearance");
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  const picker = wrapper.get('[role="radiogroup"][aria-label="Colour theme"]');
  const choices = () => picker.findAll("label");
  expect(choices().map((c) => c.text())).toEqual([
    "Aoide",
    "Catppuccin",
    "Dracula",
    "Everforest",
    "Gruvbox",
    "Nord",
    "Rosé Pine",
    "Tokyo Night",
    "dusk",
  ]);
  const choice = (title: string) => choices().find((c) => c.text() === title)!;
  expect((choice("Aoide").get("input").element as HTMLInputElement).checked).toBe(true);
  // Dark mode: the dark variant's page and accent.
  const swatch = choice("Tokyo Night").get(".theme-picker__swatch").attributes("style");
  expect(swatch).toContain("--swatch-page: #1a1b26");
  expect(swatch).toContain("--swatch-accent: #7aa2f7");
  await choice("Tokyo Night").get("input").setValue(true);
  await flushPromises();
  expect(save).toHaveBeenLastCalledWith({ appearance: expect.objectContaining({ theme: "tokyo-night" }) });
  expect((choice("Tokyo Night").get("input").element as HTMLInputElement).checked).toBe(true);
  wrapper.unmount();
});

it("shows problems in settings.toml and theme files", async () => {
  mock.emitConfig({
    problems: ["settings.toml is not valid TOML: line 3"],
    themes: [
      {
        name: "dusk",
        title: "dusk",
        builtin: false,
        light: {},
        dark: {},
        problems: ["[light] sparkle: not a colour token Aoide knows"],
      },
    ],
  });
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  expect(wrapper.text()).toContain("line 3");
  expect(wrapper.text()).toContain("not a colour token");
  wrapper.unmount();
});

it("saves the reading speed used for time left to read", async () => {
  const save = vi.spyOn(mock, "updateAppSettings");
  await useLibraryStore().start();
  const wrapper = mount(SettingsDialog, { attachTo: document.body });
  await flushPromises();
  const speed = wrapper.get('[aria-label="Reading speed in words a minute"]');
  expect((speed.element as HTMLInputElement).value).toBe("230");
  await speed.setValue("300");
  await speed.trigger("change");
  await flushPromises();
  expect(save).toHaveBeenLastCalledWith({ patch: { readingWpm: 300 } });
  expect(useLibraryStore().readingWpm).toBe(300);
  wrapper.unmount();
});
