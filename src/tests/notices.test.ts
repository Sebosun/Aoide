import { beforeEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const tauri = vi.hoisted(() => ({
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
  /** What the core held for the window before it listened. */
  held: [] as string[],
  calls: [] as string[],
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (name: string) => {
    tauri.calls.push(name);
    return name === "take_open_requests" ? tauri.held.splice(0) : undefined;
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
import { listenForOpenItems } from "@/opening";

beforeEach(() => {
  setActivePinia(createPinia());
  tauri.handlers.clear();
  tauri.held = [];
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
