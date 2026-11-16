import { afterEach, expect, it, vi } from "vitest";
import { createMockApi } from "@/api/mock";
import type { PlayerState } from "@/api/types";
import { tauriApi } from "@/api/tauri";

const ipc = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  convertFileSrc: vi.fn((path: string) => `asset:${path}`),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, convertFileSrc: ipc.convertFileSrc }));
vi.mock("@tauri-apps/api/event", () => ({ listen: ipc.listen }));
afterEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
});

it("maps Rust cover paths to display URLs without leaking the wire field", async () => {
  const mock = createMockApi();
  const [book] = await mock.listBooks();
  const { coverUrl: _, ...rest } = book;
  ipc.invoke.mockResolvedValue([
    { ...rest, coverPath: "/books/cover.jpg" },
    { ...rest, coverPath: null },
  ]);
  const result = await tauriApi.listBooks();
  expect(ipc.invoke).toHaveBeenCalledWith("list_books");
  expect(result[0].coverUrl).toBe("asset:/books/cover.jpg");
  expect(result[1].coverUrl).toBeNull();
  expect(result[0]).not.toHaveProperty("coverPath");
});

it("passes camelCase playback arguments and returns event unlisteners", async () => {
  ipc.invoke.mockResolvedValue(undefined);
  await tauriApi.play({ bookId: "book-0", chapter: 1, segmentId: "1:2:0" });
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
  const off = await mock.onPlayerState((s) => states.push(s));
  try {
    await mock.play({ bookId: "book-0", chapter: 0, segmentId: "0:0:0" });
    await vi.advanceTimersByTimeAsync(320);
    expect(states.at(-1)?.wordIndex).toBe(1);
    await mock.pause();
    const count = states.length;
    await vi.advanceTimersByTimeAsync(2000);
    expect(states).toHaveLength(count);
    await mock.setSpeed({ speed: 2 });
    await mock.resume();
    await vi.advanceTimersByTimeAsync(320);
    expect(states.at(-1)?.segmentId).toBe("0:1:0");
    await vi.advanceTimersByTimeAsync(10000);
    expect(states.some((s) => s.chapter === 1)).toBe(true);
    await mock.stop();
    const stoppedCount = states.length;
    await vi.advanceTimersByTimeAsync(1000);
    expect(states).toHaveLength(stoppedCount);
    off();
    await mock.play({ bookId: "book-1", chapter: null, segmentId: null });
    expect(states).toHaveLength(stoppedCount);
  } finally {
    mock.dispose();
  }
});

it("the mock's browser download, as the core's: refused while off, then progress, done, counted and removable", async () => {
  const off = createMockApi("?browser=off");
  expect((await off.browserStatus()).binary).toMatchObject({ kind: "found", version: null });
  await expect(off.downloadBrowser()).rejects.toThrow("The browser is off.");

  vi.useFakeTimers();
  const mock = createMockApi("?browser=missing");
  expect((await mock.browserStatus()).binary).toEqual({ kind: "missing" });
  const stages: string[] = [];
  await mock.onBrowserDownload((d) => stages.push(d.stage));
  const done = mock.downloadBrowser();
  await vi.advanceTimersByTimeAsync(1000);
  await done;
  expect([...new Set(stages)]).toEqual(["downloading", "checking", "unpacking", "done"]);
  const status = await mock.browserStatus();
  expect(status.binary).toMatchObject({ kind: "found", downloaded: true });
  expect((await mock.storageReport()).browserBytes).toBeGreaterThan(0);
  await mock.removeBrowser();
  expect((await mock.browserStatus()).binary).toEqual({ kind: "missing" });
  expect((await mock.storageReport()).browserBytes).toBe(0);
});

it("the mock's sign-in, as the core's: waiting, fetched once the user says so, the session kept on the site", async () => {
  vi.useFakeTimers();
  const mock = createMockApi("?article=needsyou-signin&browser=ready");
  const stages: string[] = [];
  await mock.onSignInState((v) => stages.push(v.stage.kind));
  const opened = await mock.startSignIn({ bookId: "book-6" });
  expect(opened).toMatchObject({ site: "quay.example", reason: "signIn", staySignedIn: true });
  await expect(mock.startSignIn({ bookId: "book-6" })).rejects.toThrow("already open");
  await vi.advanceTimersByTimeAsync(300);
  await mock.finishSignIn();
  await vi.advanceTimersByTimeAsync(500);
  expect(stages).toEqual(["waiting", "fetching", "done"]);
  expect(await mock.signIn()).toMatchObject({ result: { kind: "ready" } });
  expect(await mock.article({ bookId: "book-6" })).toMatchObject({ stage: { kind: "ready" }, via: "browser" });
  const site = (await mock.sites()).find((s) => s.host === "quay.example");
  expect(site?.session.kind).toBe("signedIn");

  const preview = createMockApi("?signin=fetching");
  expect((await preview.signIn())?.stage.kind).toBe("fetching");
  const off = createMockApi("?article=needsyou-signin");
  await expect(off.startSignIn({ bookId: "book-6" })).rejects.toThrow("The browser is off.");
});
