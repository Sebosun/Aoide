import { beforeEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const tauri = vi.hoisted(() => ({
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
  /** What the core held for the window before it listened. */
  held: [] as string[],
  /** The files the core could not open meanwhile, told when the window asks. */
  failed: [] as string[],
  /** The pages sent from the browser it saved meanwhile, told when the window asks. */
  saved: [] as unknown[],
  calls: [] as string[],
  args: [] as unknown[],
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (name: string, args?: unknown) => {
    tauri.calls.push(name);
    tauri.args.push(args);
    if (name !== "take_open_requests") return undefined;
    // Asked for what it held, the core tells the files it could not open and the pages it
    // saved meanwhile.
    for (const message of tauri.failed.splice(0)) tauri.handlers.get("core://problem")?.({ payload: { message } });
    for (const payload of tauri.saved.splice(0)) tauri.handlers.get("article://saved")?.({ payload });
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
  tauri.saved = [];
  tauri.calls = [];
  tauri.args = [];
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

it("a page saved from the browser is named in a toast, whose Undo removes it when it was new to the library", async () => {
  tauri.saved = [{ id: "article-1", title: "quay.example", newToLibrary: true }];
  const off = await listenToCore(tauriApi);
  const ui = useUiStore();
  expect(ui.toasts.map((t) => [t.text, t.action?.label])).toEqual([["Saved “quay.example”", "Undo"]]);

  await ui.toasts[0].action!.run();
  expect(tauri.calls.at(-1)).toBe("remove_book");
  expect(tauri.args.at(-1)).toEqual({ bookId: "article-1" });

  // One the library had already: nothing to undo.
  tauri.handlers.get("article://saved")!({
    payload: { id: "article-2", title: "The Quay Lights", newToLibrary: false },
  });
  expect(ui.toasts.at(-1)).toMatchObject({ text: "“The Quay Lights” is in the library already", action: undefined });
  off();
});
