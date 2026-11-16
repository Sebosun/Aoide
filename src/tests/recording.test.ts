import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import type { RecordingView, SyncStage } from "@/api/types";
import { recordingStatus } from "@/recording";
import { useRecordingsStore } from "@/stores/recordings";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("@/api", () => ({
  get api() {
    return backend.api;
  },
}));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => {
  setActivePinia(createPinia());
  mock = createMockApi();
  backend.api = mock;
});
afterEach(() => {
  mock.dispose();
  document.body.innerHTML = "";
});

const view = (stage: SyncStage, more: Partial<RecordingView> = {}): RecordingView => ({
  bookId: "book-0",
  stage,
  error: null,
  files: [],
  totalMs: 10 * 3_600_000,
  chapters: 20,
  transcribedMs: 0,
  remainingMs: null,
  position: null,
  summary: null,
  accepted: false,
  modelId: "base.en",
  listenable: stage === "ready",
  narrates: false,
  actions: [],
  ...more,
});

it("words each stage", () => {
  expect(recordingStatus(view("checking"))).toMatchObject({ label: "Checking the audio", playable: false });
  expect(recordingStatus(view("waiting", { position: 2 }))).toMatchObject({
    label: "Waiting to sync",
    detail: "2 recordings ahead",
  });
  expect(
    recordingStatus(
      view("transcribing", { transcribedMs: 2.5 * 3_600_000, remainingMs: 17 * 60_000, listenable: true }),
    ),
  ).toMatchObject({ label: "Syncing", detail: "25% · about 17 min left", percent: 25, listenable: true });
  expect(
    recordingStatus(view("ready", { summary: { words: 1000, matchedWords: 940, timedWords: 995 } })),
  ).toMatchObject({ label: "Synced", detail: "94% of the text found in the recording", playable: true });
  expect(
    recordingStatus(view("poorMatch", { summary: { words: 1000, matchedWords: 500, timedWords: 600 } })),
  ).toMatchObject({ label: "Needs review" });
  expect(recordingStatus(view("paused", { transcribedMs: 3_600_000, listenable: true }))).toMatchObject({
    percent: 10,
  });
  expect(recordingStatus(view("transcribing", { transcribedMs: 0 }))).toMatchObject({ listenable: false });
  expect(recordingStatus(view("failed", { error: "no audio" }))).toMatchObject({ detail: "no audio" });
});

it("offers what the core says can be done, in its order", () => {
  expect(recordingStatus(view("failed", { actions: ["retry", "resync", "remove"] })).actions).toEqual([
    "retry",
    "resync",
    "remove",
  ]);
});

it("offers what the core's stage machine accepts when a sync is paused and resumed", async () => {
  const store = useRecordingsStore();
  await store.start();
  mock.setRecordingForTest("book-0", "paused");
  await flushPromises();
  expect(recordingStatus(store.byBook["book-0"]).actions).toContain("resume");
  await mock.syncAction("book-0", "resume");
  // The core puts a resumed recording back in line (core/syncing.rs), not straight to work.
  expect(store.byBook["book-0"].stage).toBe("waiting");
});

it("keeps each book's recording up to date from sync events", async () => {
  const store = useRecordingsStore();
  await store.start();
  expect(store.byBook["book-0"]).toBeUndefined();
  vi.useFakeTimers();
  await store.attach("book-0", ["~/Audiobooks/lantern.m4b"]);
  expect(store.byBook["book-0"].stage).toBe("checking");
  for (let i = 0; i < 20 && store.byBook["book-0"].stage !== "ready"; i++) await vi.advanceTimersByTimeAsync(400);
  vi.useRealTimers();
  expect(store.byBook["book-0"].stage).toBe("ready");
  expect((await mock.bookSettings("book-0")).narration).toBe("recording");
  expect(store.syncing).toEqual([]);
});
