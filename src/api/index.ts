import { isTauri } from "@tauri-apps/api/core";
import { tauriApi } from "@/api/tauri";
import type { AoideApi } from "@/api/types";

/** In the app, the core; in a browser, the preview's stand-in (`@/api/mock`), loaded only there so
 *  the app does not carry it. */
export const api: AoideApi = isTauri() ? tauriApi : await previewApi();

/** Browser preview: `?delay=800` makes every request take that long, to see loading states. */
async function previewApi() {
  const { createMockApi, slowed } = await import("@/api/mock");
  const delay = Number(new URLSearchParams(location.search).get("delay")) || 0;
  return slowed(createMockApi(), delay);
}
