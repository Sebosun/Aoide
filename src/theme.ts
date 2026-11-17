import type { Appearance, ReaderFont, Theme } from "@/api/types";
import { applyZoom } from "@/zoom";
import { readingFont } from "@/fonts";

/** Colour tokens a theme file may override; mirrors aoide_config::TOKENS and theme.css. */
export const TOKENS = [
  "ground",
  "cover-tint",
  "page",
  "ink",
  "reading",
  "dim",
  "faint",
  "line",
  "glass",
  "hero",
  "wash",
  "accent",
  "on-accent",
  "ok",
  "warn",
  "shadow",
  "mark",
] as const;

/** Reading-text scale for each text-size step (2 is the designed size). */
export const TEXT_SCALES = [0.84, 0.92, 1, 1.1, 1.22] as const;
/** Reading line height for each line-spacing step (2 is the designed spacing). */
export const LINE_HEIGHTS = [1.45, 1.56, 1.68, 1.82, 1.98] as const;
/** Reading column width in px for each text-width step (2 is the designed width). */
export const TEXT_WIDTHS = [520, 580, 640, 720, 800] as const;
/** The font stack of each reader font (fonts.css, theme.css). */
export const READER_FONTS: Record<ReaderFont, string> = {
  serif: "var(--font-read)",
  sans: "var(--font-ui)",
  legible: '"Atkinson Hyperlegible Next", var(--font-ui)',
  system: "system-ui, sans-serif",
};

/** The entry of `steps` for `step`, rounded and kept in range; the designed one (2) for a
 *  value that is not a number. */
function stepOf<T>(steps: readonly T[], step: number): T {
  const at = Number.isFinite(step) ? Math.round(step) : 2;
  return steps[Math.min(steps.length - 1, Math.max(0, at))];
}

const declarations = (tokens: Record<string, string>) =>
  Object.entries(tokens)
    .filter(([key]) => (TOKENS as readonly string[]).includes(key))
    .map(([key, value]) => `--${key}: ${value};`)
    .join(" ");

/** CSS for a theme file, using the selectors of theme.css so it overrides them when injected
 *  after it. Values were validated by the core (colours only). */
export function themeCss(theme: Theme | undefined): string {
  if (!theme) return "";
  const light = declarations(theme.light),
    dark = declarations(theme.dark);
  return [
    light && `:root { ${light} }`,
    dark && `@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { ${dark} } }`,
    dark && `:root[data-theme="dark"] { ${dark} }`,
  ]
    .filter(Boolean)
    .join("\n");
}

/** Applies mode, the reader's typography, interface zoom and the chosen theme to the document. */
export function applyAppearance(appearance: Appearance, themes: Theme[], root: HTMLElement = document.documentElement) {
  if (appearance.mode === "auto") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", appearance.mode);
  root.style.setProperty("--read-scale", String(stepOf(TEXT_SCALES, appearance.textSize)));
  root.style.setProperty("--read-line", String(stepOf(LINE_HEIGHTS, appearance.lineSpacing)));
  root.style.setProperty("--read-width", `${stepOf(TEXT_WIDTHS, appearance.textWidth)}px`);
  const font = appearance.readerFont in READER_FONTS ? appearance.readerFont : "serif";
  root.style.setProperty("--read-font", READER_FONTS[font]);
  // Before the first book opens, so its text is laid out once, in the font it is read in.
  void readingFont(font);
  root.style.setProperty("--read-align", appearance.justify ? "justify" : "start");
  root.style.setProperty("--read-hyphens", appearance.hyphenate ? "auto" : "manual");
  applyZoom(appearance.zoom, root);
  let style = document.getElementById("aoide-user-theme");
  if (!style) {
    style = document.createElement("style");
    style.id = "aoide-user-theme";
    document.head.append(style);
  }
  style.textContent = themeCss(themes.find((t) => t.name === appearance.theme));
}
