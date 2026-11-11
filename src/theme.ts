import type { Appearance, Theme } from "./api/types";
import { applyZoom } from "./zoom";

/** Colour tokens a theme file may override; mirrors aoide_config::TOKENS and theme.css. */
export const TOKENS = ["ground", "cover-tint", "page", "ink", "reading", "dim", "faint", "line", "glass", "hero", "wash", "accent", "on-accent", "ok", "warn", "shadow"] as const;

/** Reading-text scale for each text-size step (2 is the designed size). */
export const TEXT_SCALES = [0.84, 0.92, 1, 1.1, 1.22] as const;

const declarations = (tokens: Record<string, string>) =>
  Object.entries(tokens).filter(([key]) => (TOKENS as readonly string[]).includes(key)).map(([key, value]) => `--${key}: ${value};`).join(" ");

/** CSS for a theme file, using the selectors of theme.css so it overrides them when injected
 *  after it. Values were validated by the core (colours only). */
export function themeCss(theme: Theme | undefined): string {
  if (!theme) return "";
  const light = declarations(theme.light), dark = declarations(theme.dark);
  return [
    light && `:root { ${light} }`,
    dark && `@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { ${dark} } }`,
    dark && `:root[data-theme="dark"] { ${dark} }`,
  ].filter(Boolean).join("\n");
}

/** Applies mode, text size, interface zoom and the chosen theme to the document. */
export function applyAppearance(appearance: Appearance, themes: Theme[], root: HTMLElement = document.documentElement) {
  if (appearance.mode === "auto") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", appearance.mode);
  const step = Math.min(TEXT_SCALES.length - 1, Math.max(0, Math.round(appearance.textSize)));
  root.style.setProperty("--read-scale", String(TEXT_SCALES[step]));
  applyZoom(appearance.zoom, root);
  let style = document.getElementById("aoide-user-theme");
  if (!style) {
    style = document.createElement("style");
    style.id = "aoide-user-theme";
    document.head.append(style);
  }
  style.textContent = themeCss(themes.find(t => t.name === appearance.theme));
}
