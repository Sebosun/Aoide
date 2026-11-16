import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api } from "@/api";
import { ApiError } from "@/api/errors";
import type { AudiobookMatch, AudiobookSuggestion, MatchStage, ScanProgress } from "@/api/types";
import { useRecordingsStore } from "@/stores/recordings";

/** Find audiobooks' tabs: the core's match stages, as the screen groups them. */
export type MatchGroup = "found" | "checking" | "paired" | "unverified";
const groupOf: Record<MatchStage["kind"], MatchGroup> = {
  found: "found",
  verifying: "checking",
  paired: "paired",
  noneVerified: "unverified",
};

/** Audiobooks found for the library's books ("Find audiobooks"). The core keeps the matches and
 *  their stages; this keeps the screen's choices: the tab, the selected books, and which
 *  candidate each book would be paired with. */
export const useAudiobooksStore = defineStore("audiobooks", () => {
  const matches = ref<AudiobookMatch[]>([]),
    progress = ref<ScanProgress | null>(null);
  /** What the last search found that matches no book, to add on its own. */
  const unmatched = ref<AudiobookSuggestion[]>([]);
  async function loadUnmatched() {
    unmatched.value = await api.unmatchedAudiobooks();
  }
  /** What the screen is doing: one thing at a time. */
  const activity = ref<"idle" | "scanning" | "pairing" | "adding">("idle");
  const scanning = computed(() => activity.value === "scanning"),
    pairing = computed(() => activity.value === "pairing"),
    adding = computed(() => activity.value === "adding");
  const error = ref(""),
    filter = ref<MatchGroup>("found");
  /** Runs `task` as the screen's one activity (nothing, when another is under way); what it
   *  throws becomes the error, unless `quiet` says it is not one. */
  async function run(
    kind: Exclude<typeof activity.value, "idle">,
    task: () => Promise<void>,
    quiet: (e: unknown) => boolean = () => false,
  ) {
    if (activity.value !== "idle") return;
    activity.value = kind;
    error.value = "";
    try {
      await task();
    } catch (e) {
      if (!quiet(e)) error.value = String(e);
    } finally {
      activity.value = "idle";
    }
  }
  const selected = ref<Set<string>>(new Set());
  /** The candidate picked for a book, when not its closest. */
  const picked = ref<Record<string, number>>({});
  const counts = computed(() => {
    const c: Record<MatchGroup, number> = { found: 0, checking: 0, paired: 0, unverified: 0 };
    for (const match of matches.value) c[groupOf[match.stage.kind]]++;
    return c;
  });
  const visible = computed(() => matches.value.filter((m) => groupOf[m.stage.kind] === filter.value));
  /** Books the user can pair from the current tab. */
  const pairable = computed(() => filter.value === "found" || filter.value === "unverified");
  const candidateOf = (bookId: string) => picked.value[bookId] ?? 0;
  function put(next: AudiobookMatch[]) {
    const before = new Map(matches.value.map((m) => [m.bookId, m.stage]));
    matches.value = next;
    // A recording paired or unpaired by Aoide (while verifying) is news to the recordings store.
    const recordings = useRecordingsStore();
    for (const m of next)
      if (before.has(m.bookId) && before.get(m.bookId)!.kind !== m.stage.kind) void recordings.load(m.bookId);
    selected.value = new Set(
      [...selected.value].filter((id) => next.some((m) => m.bookId === id && groupOf[m.stage.kind] === filter.value)),
    );
  }
  let startup: Promise<void> | undefined;
  const unlisteners: (() => void)[] = [];
  function start() {
    return (startup ??= (async () => {
      try {
        unlisteners.push(
          await api.onAudiobookScan((p) => {
            progress.value = p;
          }),
        );
        unlisteners.push(await api.onAudiobookMatches(put));
        put(await api.audiobookMatches());
        await loadUnmatched();
      } catch (e) {
        error.value = String(e);
      }
    })());
  }
  const scan = () =>
    run(
      "scanning",
      async () => {
        progress.value = null;
        put(await api.scanForAudiobooks());
        await loadUnmatched();
      },
      (e) => e instanceof ApiError && e.kind === "cancelled",
    );
  async function pair(bookIds: string[]) {
    if (!bookIds.length) return;
    await run("pairing", async () => {
      const outcomes = await api.pairAudiobooks({
        choices: bookIds.map((bookId) => ({ bookId, candidate: candidateOf(bookId) })),
      });
      const failed = outcomes.filter((o) => o.error);
      if (failed.length) error.value = failed.map((o) => o.error).join("\n");
      put(await api.audiobookMatches());
      const recordings = useRecordingsStore();
      await Promise.all(bookIds.map((id) => recordings.load(id)));
    });
  }
  /** Adds an audiobook that matches no book as an item of its own. */
  const addUnmatched = (audiobook: AudiobookSuggestion) =>
    run("adding", async () => {
      await useRecordingsStore().addOnItsOwn(audiobook.paths);
      await loadUnmatched();
    });
  function toggle(id: string, on: boolean) {
    const next = new Set(selected.value);
    if (on) next.add(id);
    else next.delete(id);
    selected.value = next;
  }
  function pick(bookId: string, candidate: number) {
    picked.value = { ...picked.value, [bookId]: candidate };
  }
  function show(group: MatchGroup) {
    filter.value = group;
    selected.value = new Set();
  }
  function dispose() {
    unlisteners.splice(0).forEach((fn) => fn());
    startup = undefined;
  }
  return {
    activity,
    matches,
    unmatched,
    adding,
    addUnmatched,
    progress,
    scanning,
    pairing,
    error,
    filter,
    selected,
    counts,
    visible,
    pairable,
    candidateOf,
    start,
    scan,
    cancel: () => api.cancelAudiobookScan(),
    pair,
    toggle,
    pick,
    show,
    dispose,
  };
});
