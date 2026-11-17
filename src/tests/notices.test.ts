import { beforeEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const tauri = vi.hoisted(() => ({
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
  /** What the core held for the window before it listened. */
  held: [] as string[],
  /** The files the core could not open meanwhile, told when the window asks. */
  failed: [] as string[],
  calls: [] as string[],
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (name: string) => {
    tauri.calls.push(name);
    if (name !== "take_open_requests") return undefined;
    // Asked for what it held, the core tells the files it could not open meanwhile.
    for (const message of tauri.failed.splice(0)) tauri.handlers.get("core://problem")?.({ payload: { message } });
    return tauri.held.splice(0);
  }),
  convertFileSrc: (p: string) => p,
  isTauri: () => true,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, h: (event: { payload: unknown }) => void) => {
    tauri.handlers.set(name, h);
    return () => {};
  }),
}));

import { tauriApi } from "@/api/tauri";
import { useNavigationStore } from "@/stores/navigation";
import { useUiStore } from "@/stores/ui";
import { listenForOpenItems, listenToCore } from "@/opening";

beforeEach(() => {
  setActivePinia(createPinia());
  tauri.handlers.clear();
  tauri.held = [];
  tauri.failed = [];
  tauri.calls = [];
});

it("clicking a desktop notification opens its item's page, closing the Add dialog", async () => {
  const navigation = useNavigationStore();
  navigation.openAdd();
  const off = await listenForOpenItems(tauriApi);
  tauri.handlers.get("item://open")!({ payload: "book-3" });
  expect(navigation.view).toBe("item");
  expect(navigation.bookId).toBe("book-3");
  expect(navigation.addOpen).toBe(false);
  off();
});

it("a book opened from the file manager while the window started opens once the window listens", async () => {
  const navigation = useNavigationStore();
  tauri.held = ["book-2", "book-4"];
  const off = await listenForOpenItems(tauriApi);
  // The listener is attached before the core is asked, so nothing falls between the two.
  expect(tauri.handlers.has("item://open")).toBe(true);
  expect(tauri.calls).toEqual(["take_open_requests"]);
  expect(navigation.view).toBe("item");
  expect(navigation.bookId).toBe("book-4");
  off();
});

it("a book that could not be opened while the window started is shown as a problem once it listens", async () => {
  tauri.failed = ["Could not open Torn.epub: not a book"];
  const off = await listenToCore(tauriApi);
  expect(useUiStore().toasts.map((t) => [t.text, t.tone])).toEqual([["Could not open Torn.epub: not a book", "error"]]);
  off();
});
