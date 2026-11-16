/** The browser preview's stand-in core replays the real one. Each scenario in
 *  `fixtures/scenarios.json` was run on the core (src-tauri/src/core/fixtures.rs
 *  `scenarios_for_the_ui_mock`): every call the window could make, and what the core answered.
 *  Here each call is made again on the mock, which must answer alike. */
import { afterEach, describe, expect, it } from "vitest";
import { createMockApi } from "@/api/mock";
import { ApiError } from "@/api/errors";
import type { PlayerState, RecordingView, SyncStage } from "@/api/types";
import scenarios from "@/tests/fixtures/scenarios.json";
import stages from "@/tests/fixtures/stages.json";
import home from "@/tests/fixtures/home.json";
import player from "@/tests/fixtures/player.json";

let mock: ReturnType<typeof createMockApi> | null = null;
afterEach(() => mock?.dispose());

/** The value at `path`, read as the core's recorder reads it (fixtures.rs `pick`). */
function pick(value: unknown, path: string): unknown {
  let at = value;
  // Dots inside `[…]` (a web address) do not split.
  for (const part of path.match(/(?:\[[^\]]*\]|[^.])+/g) ?? []) {
    const field = /^\[(\w+)=(.*)\]$/.exec(part);
    if (part.startsWith("has=")) at = Array.isArray(at) && at.includes(part.slice(4));
    else if (field)
      at = Array.isArray(at)
        ? (at.find((item: Record<string, unknown>) => String(item[field[1]]) === field[2]) ?? null)
        : null;
    else if (part === "length" && Array.isArray(at)) at = at.length;
    else at = (at as Record<string, unknown> | null)?.[part] ?? null;
  }
  return at === undefined ? null : at;
}

/** Equal as JSON, numbers to within rounding (the core's fractions go through f32). */
/** Equal as JSON, whatever the order of an object's keys; numbers to within 1e-6. */
function same(actual: unknown, expected: unknown): boolean {
  if (typeof actual === "number" && typeof expected === "number") return Math.abs(actual - expected) < 1e-6;
  if (Array.isArray(actual) && Array.isArray(expected))
    return actual.length === expected.length && actual.every((a, i) => same(a, expected[i]));
  if (actual && expected && typeof actual === "object" && typeof expected === "object") {
    const [a, e] = [actual as Record<string, unknown>, expected as Record<string, unknown>];
    const keys = (o: Record<string, unknown>) => Object.keys(o).filter((k) => o[k] !== undefined);
    return keys(a).length === keys(e).length && keys(e).every((k) => same(a[k], e[k]));
  }
  return JSON.stringify(actual) === JSON.stringify(expected);
}

type Step = {
  call: string;
  /** Named as the window sends them; none for a command that takes none. */
  args: Record<string, unknown>;
  expect: Record<string, unknown>;
  error: string | null;
  /** The event the window is sent after it, and what its last payload holds. */
  event?: { name: string; expect: Record<string, unknown> };
};

describe("the mock answers as the core did", () => {
  for (const scenario of scenarios as { name: string; start: string; steps: Step[] }[])
    it(scenario.name, async () => {
      mock = createMockApi(scenario.start);
      const api = mock as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>;
      // The last payload of each event the scenario looks at.
      const last = new Map<string, unknown>();
      for (const name of new Set(scenario.steps.flatMap((s) => (s.event ? [s.event.name] : []))))
        await (api[name] as unknown as (h: (p: unknown) => void) => Promise<unknown>)((p) => last.set(name, p));
      for (const [n, step] of scenario.steps.entries()) {
        const what = `step ${n + 1}: ${step.call}(${JSON.stringify(step.args)})`;
        const call = Object.keys(step.args).length ? api[step.call](step.args) : api[step.call]();
        const answer = await call.then(
          (value) => ({ value, error: null }),
          (e: unknown) => ({ value: null, error: e instanceof Error ? e.message : String(e) }),
        );
        expect(answer.error, what).toBe(step.error);
        for (const [path, expected] of Object.entries(step.expect)) {
          const actual = pick(answer.value, path);
          expect(same(actual, expected), `${what} at "${path}": ${JSON.stringify(actual)}`).toBe(true);
        }
        if (!step.event) continue;
        // The core's events may follow later (a fetch, a download): wait until the mock's do.
        const { name, expect: wanted } = step.event;
        const differ = () =>
          Object.entries(wanted).filter(([path, expected]) => !same(pick(last.get(name) ?? null, path), expected));
        for (let waited = 0; differ().length && waited < 5_000; waited += 25)
          await new Promise((resolve) => setTimeout(resolve, 25));
        for (const [path, expected] of differ())
          expect(pick(last.get(name) ?? null, path), `${what}, then ${name} at "${path}"`).toEqual(expected);
      }
    });
});

it("starts with the Home the core made for the preview's books", async () => {
  mock = createMockApi();
  expect(await mock.home()).toEqual(home);
});

it("lets a recording be listened to in the stages the core does", async () => {
  mock = createMockApi();
  const seen: RecordingView[] = [];
  await mock.onSyncState((r) => seen.push(r));
  for (const [stage, { listenable }] of Object.entries(stages.sync) as [SyncStage, { listenable: boolean[] }][]) {
    mock.setRecordingForTest("book-1", stage, null);
    const view = seen.at(-1)!;
    expect(view.listenable, stage).toBe(listenable[view.transcribedMs > 0 ? 1 : 0]);
  }
});

it("refuses with an error of a kind, as the core does", async () => {
  mock = createMockApi();
  const refused = await mock.saveArticle({ url: "harbour notes" }).catch((e: unknown) => e);
  expect(refused).toBeInstanceOf(ApiError);
  expect((refused as ApiError).kind).toBe("internal");
});

it("plays a book with the times left the core gave for its sentences, at the speed", async () => {
  mock = createMockApi();
  const seen: PlayerState[] = [];
  await mock.onPlayerState((s) => seen.push(s));
  await mock.play({ bookId: "book-0", chapter: 1, segmentId: "1:1:0" });
  await mock.pause();
  const core = player.at["book-0"]["1:1:0"];
  expect(seen.at(-1)).toMatchObject(core);
  await mock.setSpeed({ speed: 2 });
  expect(seen.at(-1)).toMatchObject({
    chapterLeftMs: Math.floor(core.chapterLeftMs / 2000) * 1000,
    bookLeftMs: Math.floor(core.bookLeftMs / 2000) * 1000,
    chapterMs: core.chapterMs,
  });
});
