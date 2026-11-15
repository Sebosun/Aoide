import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api } from "@/api";
import type { Candidate, CandidateStatus, DiscoveryImportProgress, ScanProgress } from "@/api/types";
import { useLibraryStore } from "@/stores/library";

export const useDiscoveryStore = defineStore("discovery", () => {
  const candidates = ref<Candidate[]>([]),
    progress = ref<ScanProgress | null>(null);
  /** What the screen is doing, one thing at a time, with how far it got. */
  type Activity =
    | { kind: "idle" }
    | { kind: "scanning" }
    | { kind: "fetching"; done: number; total: number }
    | { kind: "importing"; progress: DiscoveryImportProgress | null };
  const activity = ref<Activity>({ kind: "idle" });
  const scanning = computed(() => activity.value.kind === "scanning");
  const fetching = computed(() => {
    const a = activity.value;
    return a.kind === "fetching" ? { done: a.done, total: a.total } : null;
  });
  const importing = computed(() => (activity.value.kind === "importing" ? activity.value.progress : null));
  const error = ref(""),
    filter = ref<CandidateStatus>("new");
  /** Runs `task` as the screen's one activity (nothing, when another is under way); what it
   *  throws becomes the error, unless `quiet` says it is not one. */
  async function run(start: Activity, task: () => Promise<void>, quiet: (e: unknown) => boolean = () => false) {
    if (activity.value.kind !== "idle") return;
    activity.value = start;
    error.value = "";
    try {
      await task();
    } catch (e) {
      if (!quiet(e)) error.value = String(e);
    } finally {
      activity.value = { kind: "idle" };
    }
  }
  const selected = ref<Set<string>>(new Set()),
    applyDetails = ref(true),
    pairAudiobooks = ref(true);
  const counts = computed(() => {
    const c: Record<CandidateStatus, number> = { new: 0, inLibrary: 0, removed: 0, unreadable: 0 };
    for (const candidate of candidates.value) c[candidate.status]++;
    return c;
  });
  const visible = computed(() => candidates.value.filter((c) => c.status === filter.value));
  const needingDetails = computed(() =>
    candidates.value.filter((c) => c.status === "new" && !c.suggestion).map((c) => c.id),
  );
  let startup: Promise<void> | undefined;
  const unlisteners: (() => void)[] = [];
  function start() {
    return (startup ??= (async () => {
      try {
        unlisteners.push(
          await api.onScanProgress((p) => {
            progress.value = p;
          }),
        );
        unlisteners.push(
          await api.onDiscoveryImport((p) => {
            if (activity.value.kind === "importing") activity.value = { kind: "importing", progress: p };
          }),
        );
        unlisteners.push(
          await api.onDiscoveryDetails(() => {
            const a = activity.value;
            if (a.kind === "fetching") activity.value = { ...a, done: a.done + 1 };
          }),
        );
        candidates.value = await api.discoveredBooks();
      } catch (e) {
        error.value = String(e);
      }
    })());
  }
  const scan = () =>
    run(
      { kind: "scanning" },
      async () => {
        progress.value = null;
        candidates.value = await api.scanForBooks();
        selected.value = new Set();
      },
      (e) => String(e).includes("cancelled"),
    );
  async function fetchDetails(ids = needingDetails.value) {
    if (!ids.length) return;
    await run({ kind: "fetching", done: 0, total: ids.length }, async () => {
      await api.fetchDiscoveredDetails(ids);
      candidates.value = await api.discoveredBooks();
    });
  }
  async function importBooks(ids: string[]) {
    if (!ids.length) return;
    const choices = ids.map((id) => {
      const candidate = candidates.value.find((c) => c.id === id);
      return {
        id,
        applySuggestion: applyDetails.value && Boolean(candidate?.suggestion),
        pairAudiobook: pairAudiobooks.value && Boolean(candidate?.audiobook.length),
      };
    });
    await run({ kind: "importing", progress: null }, async () => {
      const results = await api.importDiscovered(choices);
      const failed = results.filter((r) => r.error);
      if (failed.length) error.value = failed.map((r) => `${r.path}: ${r.error}`).join("\n");
      candidates.value = await api.discoveredBooks();
      selected.value = new Set([...selected.value].filter((id) => !ids.includes(id)));
      await useLibraryStore().refresh();
      await useLibraryStore().lookForAudiobooks(results.flatMap((r) => (r.bookId && !r.error ? [r.bookId] : [])));
    });
  }
  function toggle(id: string, on: boolean) {
    const next = new Set(selected.value);
    if (on) next.add(id);
    else next.delete(id);
    selected.value = next;
  }
  function dispose() {
    unlisteners.splice(0).forEach((fn) => fn());
    startup = undefined;
  }
  return {
    candidates,
    progress,
    importing,
    scanning,
    fetching,
    error,
    filter,
    selected,
    applyDetails,
    pairAudiobooks,
    counts,
    visible,
    needingDetails,
    start,
    scan,
    cancel: () => api.cancelScan(),
    fetchDetails,
    importBooks,
    toggle,
    dispose,
  };
});
