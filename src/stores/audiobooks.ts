import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api } from "../api";
import type { AudiobookMatch, MatchStage, ScanProgress } from "../api/types";
import { useRecordingsStore } from "./recordings";

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
  const scanning = ref(false),
    pairing = ref(false),
    error = ref(""),
    filter = ref<MatchGroup>("found");
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
      } catch (e) {
        error.value = String(e);
      }
    })());
  }
  async function scan() {
    error.value = "";
    scanning.value = true;
    progress.value = null;
    try {
      put(await api.scanForAudiobooks());
    } catch (e) {
      if (!String(e).includes("cancelled")) error.value = String(e);
    } finally {
      scanning.value = false;
    }
  }
  async function pair(bookIds: string[]) {
    if (!bookIds.length) return;
    error.value = "";
    pairing.value = true;
    try {
      const outcomes = await api.pairAudiobooks(bookIds.map((bookId) => ({ bookId, candidate: candidateOf(bookId) })));
      const failed = outcomes.filter((o) => o.error);
      if (failed.length) error.value = failed.map((o) => o.error).join("\n");
      put(await api.audiobookMatches());
      const recordings = useRecordingsStore();
      await Promise.all(bookIds.map((id) => recordings.load(id)));
    } catch (e) {
      error.value = String(e);
    } finally {
      pairing.value = false;
    }
  }
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
    matches,
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
