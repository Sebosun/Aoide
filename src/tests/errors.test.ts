import { beforeEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const tauri = vi.hoisted(() => ({
  invoke: vi.fn(),
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: tauri.invoke,
  convertFileSrc: (p: string) => p,
  isTauri: () => true,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, h: (event: { payload: unknown }) => void) => {
    tauri.handlers.set(name, h);
    return () => {};
  }),
}));

import { ApiError } from "../api/errors";
import { tauriApi } from "../api/tauri";
import { useRecordingsStore } from "../stores/recordings";
import { useUiStore } from "../stores/ui";
import { listenForProblems } from "../problems";

beforeEach(() => {
  setActivePinia(createPinia());
  tauri.invoke.mockReset();
  tauri.handlers.clear();
});

it("a rejected command reaches the store as an ApiError with its kind and message", async () => {
  tauri.invoke.mockImplementation(async (name: string) => {
    if (name === "attach_recording") throw { kind: "notFound", what: "book", message: "book was not found" };
  });
  const failure = await useRecordingsStore()
    .attach("book-9", ["/audio/a.m4b"])
    .catch((e) => e);
  expect(failure).toBeInstanceOf(ApiError);
  expect(failure.kind).toBe("notFound");
  expect(failure.error).toEqual({ kind: "notFound", what: "book", message: "book was not found" });
  expect(String(failure)).toBe("book was not found");
});

it("an error Tauri gives as text is an internal ApiError with that text", async () => {
  tauri.invoke.mockRejectedValue("command scan_for_books not found");
  const failure = await tauriApi.scanForBooks().catch((e) => e);
  expect(failure).toBeInstanceOf(ApiError);
  expect(failure.kind).toBe("internal");
  expect(String(failure)).toBe("command scan_for_books not found");
});

it("shows a problem the core reports as an error toast", async () => {
  const off = await listenForProblems(tauriApi);
  tauri.handlers.get("core://problem")!({
    payload: { message: "Could not save your place: disk full", error: { kind: "io", message: "disk full" } },
  });
  expect(useUiStore().toasts).toEqual([
    expect.objectContaining({ text: "Could not save your place: disk full", tone: "error" }),
  ]);
  off();
});
