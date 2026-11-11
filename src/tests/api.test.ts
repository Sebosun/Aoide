import { afterEach, expect, it, vi } from "vitest";
import { createMockApi } from "../api/mock";
import type { PlayerState } from "../api/types";
import { tauriApi } from "../api/tauri";

const ipc = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), convertFileSrc: vi.fn((path: string) => `asset:${path}`) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, convertFileSrc: ipc.convertFileSrc }));
vi.mock("@tauri-apps/api/event", () => ({ listen: ipc.listen }));
afterEach(() => { vi.useRealTimers(); vi.clearAllMocks(); });

it("maps Rust cover paths to display URLs without leaking the wire field", async () => {
  const mock = createMockApi();
  const [book] = await mock.listBooks();
  const { coverUrl: _, ...rest } = book;
  ipc.invoke.mockResolvedValue([{ ...rest, coverPath: "/books/cover.jpg" }, { ...rest, coverPath: null }]);
  const result = await tauriApi.listBooks();
  expect(ipc.invoke).toHaveBeenCalledWith("list_books");
  expect(result[0].coverUrl).toBe("asset:/books/cover.jpg");
  expect(result[1].coverUrl).toBeNull();
  expect(result[0]).not.toHaveProperty("coverPath");
});

it("passes camelCase playback arguments and returns event unlisteners", async () => {
  ipc.invoke.mockResolvedValue(undefined);
  await tauriApi.play("book-0", 1, "1:2:0");
  expect(ipc.invoke).toHaveBeenCalledWith("play", { bookId: "book-0", chapter: 1, segmentId: "1:2:0" });
  const off = vi.fn();
  ipc.listen.mockResolvedValue(off);
  const handler = vi.fn();
  const unlisten = await tauriApi.onPlayerState(handler);
  const [event, callback] = ipc.listen.mock.calls[0];
  expect(event).toBe("player://state");
  callback({ payload: { status: "playing" } });
  expect(handler).toHaveBeenCalledWith({ status: "playing" });
  unlisten();
  expect(off).toHaveBeenCalledOnce();
});

it("simulates words, segment and chapter transitions, and respects pause, speed, stop and unlisten", async () => {
  vi.useFakeTimers();
  const mock = createMockApi();
  const states: PlayerState[] = [];
  const off = await mock.onPlayerState(s => states.push(s));
  try {
    await mock.play("book-0", 0, "0:0:0");
    await vi.advanceTimersByTimeAsync(320);
    expect(states.at(-1)?.wordIndex).toBe(1);
    await mock.pause();
    const count = states.length;
    await vi.advanceTimersByTimeAsync(2000);
    expect(states).toHaveLength(count);
    await mock.setSpeed(2);
    await mock.resume();
    await vi.advanceTimersByTimeAsync(320);
    expect(states.at(-1)?.segmentId).toBe("0:1:0");
    await vi.advanceTimersByTimeAsync(10000);
    expect(states.some(s => s.chapter === 1)).toBe(true);
    await mock.stop();
    const stoppedCount = states.length;
    await vi.advanceTimersByTimeAsync(1000);
    expect(states).toHaveLength(stoppedCount);
    off();
    await mock.play("book-1", null, null);
    expect(states).toHaveLength(stoppedCount);
  } finally { mock.dispose(); }
});
