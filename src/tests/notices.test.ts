import { beforeEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const tauri = vi.hoisted(() => ({
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
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
