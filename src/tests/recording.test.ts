import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { createMockApi } from "../api/mock";
import type { RecordingView, SyncStage } from "../api/types";
import { recordingStatus } from "../recording";
import { useRecordingsStore } from "../stores/recordings";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("../api", () => ({ get api() { return backend.api; } }));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => { setActivePinia(createPinia()); mock = createMockApi(); backend.api = mock; });
afterEach(() => { mock.dispose(); document.body.innerHTML = ""; });

const view = (stage: SyncStage, more: Partial<RecordingView> = {}): RecordingView => ({
  bookId: "book-0", stage, error: null, files: [], totalMs: 10 * 3_600_000, chapters: 20, transcribedMs: 0,
  remainingMs: null, position: null, summary: null, accepted: false, modelId: "base.en", ...more,
});

it("words each stage and offers only what applies to it", () => {
  expect(recordingStatus(view("checking"))).toMatchObject({ label: "Checking the audio", actions: ["remove"], playable: false });
  expect(recordingStatus(view("waiting", { position: 2 }))).toMatchObject({ label: "Waiting to sync", detail: "2 recordings ahead", actions: ["pause", "remove"] });
  expect(recordingStatus(view("transcribing", { transcribedMs: 2.5 * 3_600_000, remainingMs: 17 * 60_000 }))).toMatchObject({ label: "Syncing", detail: "25% · about 17 min left", percent: 25, actions: ["listen", "pause", "remove"], listenable: true });
  expect(recordingStatus(view("needsModel"))).toMatchObject({ actions: ["download", "remove"] });
  expect(recordingStatus(view("ready", { summary: { words: 1000, matchedWords: 940, timedWords: 995 } }))).toMatchObject({ label: "Synced", detail: "94% of the text found in the recording", actions: ["report", "export", "resync", "remove"], playable: true });
  expect(recordingStatus(view("poorMatch", { summary: { words: 1000, matchedWords: 500, timedWords: 600 } }))).toMatchObject({ label: "Needs review", actions: ["report", "accept", "remove"] });
  expect(recordingStatus(view("sampleMismatch")).actions).toEqual(["accept", "remove"]);
  expect(recordingStatus(view("paused", { transcribedMs: 3_600_000 }))).toMatchObject({ percent: 10, actions: ["listen", "resume", "remove"] });
  expect(recordingStatus(view("transcribing", { transcribedMs: 0 }))).toMatchObject({ actions: ["pause", "remove"], listenable: false });
  expect(recordingStatus(view("failed", { error: "no audio" }))).toMatchObject({ detail: "no audio", actions: ["retry", "remove"] });
  expect(recordingStatus(view("missing")).actions).toEqual(["remove"]);
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
