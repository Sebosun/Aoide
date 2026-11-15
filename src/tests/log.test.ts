import { beforeEach, expect, it, vi } from "vitest";

const tauri = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: tauri.invoke, convertFileSrc: (p: string) => p, isTauri: () => true }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import { tauriApi } from "../api/tauri";
import { forget } from "../log";

beforeEach(() => { tauri.invoke.mockReset(); forget(); });
const logged = () => tauri.invoke.mock.calls.filter(([name]) => name === "log_ui").map(([, args]) => args.entry);

it("writes a failed call to the log once, and still fails the caller", async () => {
  tauri.invoke.mockImplementation(async (name: string) => { if (name === "list_books") throw "database is locked"; });
  await expect(tauriApi.listBooks()).rejects.toThrow("database is locked");
  await expect(tauriApi.listBooks()).rejects.toThrow("database is locked");
  expect(logged()).toEqual([expect.objectContaining({ level: "warn", event: "ipc.error", msg: "database is locked", command: "list_books", tookMs: expect.any(Number) })]);
});

it("notes a slow answer, but not from the long jobs that log their own time", async () => {
  vi.useFakeTimers();
  tauri.invoke.mockImplementation(() => new Promise(done => setTimeout(done, 3_000)));
  const answers = [tauriApi.listBooks().catch(() => {}), tauriApi.scanForBooks().catch(() => {})];
  await vi.advanceTimersByTimeAsync(3_000);
  await Promise.all(answers);
  vi.useRealTimers();
  expect(logged()).toEqual([expect.objectContaining({ level: "info", event: "ipc.slow", command: "list_books", tookMs: 3_000 })]);
});

it("leaves cancelled scans and its own failures out of the log", async () => {
  tauri.invoke.mockImplementation(async (name: string) => { throw name === "scan_for_books" ? "cancelled" : "no window"; });
  await expect(tauriApi.scanForBooks()).rejects.toThrow("cancelled");
  await expect(tauriApi.logUi({ level: "error", event: "ui.error", msg: "x" })).rejects.toThrow("no window");
  expect(logged()).toEqual([{ level: "error", event: "ui.error", msg: "x" }]);
});
