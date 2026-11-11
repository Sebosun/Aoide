import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";

/** Interface sizes that Ctrl + and Ctrl − step through; 1 is 100%. */
export const ZOOM_STEPS = [0.67, 0.8, 0.9, 1, 1.1, 1.25, 1.5] as const;
const MIN = ZOOM_STEPS[0], MAX = ZOOM_STEPS[ZOOM_STEPS.length - 1];

/** A usable zoom from any saved value (settings.toml can be edited by hand). */
export const clampZoom = (zoom: number) => Number.isFinite(zoom) ? Math.min(MAX, Math.max(MIN, zoom)) : 1;

/** The next size up (`direction` 1) or down (-1) from `zoom`; the ends stay where they are. */
export function stepZoom(zoom: number, direction: 1 | -1): number {
  const current = clampZoom(zoom), near = 0.001;
  if (direction > 0) return ZOOM_STEPS.find(step => step > current + near) ?? MAX;
  return [...ZOOM_STEPS].reverse().find(step => step < current - near) ?? MIN;
}

export const zoomLabel = (zoom: number) => `${Math.round(clampZoom(zoom) * 100)}%`;

let applied: number | null = null;
/** Sizes the whole interface: the webview's own zoom in the desktop app, CSS zoom on the root
 *  element in the browser preview. */
export function applyZoom(zoom: number, root: HTMLElement = document.documentElement) {
  const next = clampZoom(zoom);
  if (!isTauri()) { root.style.setProperty("zoom", String(next)); return; }
  if (next === applied) return;
  applied = next;
  void getCurrentWebview().setZoom(next).catch(() => { applied = null; });
}
