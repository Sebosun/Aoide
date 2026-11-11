import { isTauri } from "@tauri-apps/api/core";
import { tauriApi } from "./tauri";
import { createMockApi, slowed } from "./mock";
import type { AoideApi } from "./types";

/** Browser preview: `?delay=800` makes every request take that long, to see loading states. */
const delay = isTauri() ? 0 : Number(new URLSearchParams(location.search).get("delay")) || 0;
export const api: AoideApi = isTauri() ? tauriApi : slowed(createMockApi(), delay);
