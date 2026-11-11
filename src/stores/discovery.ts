import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api } from "../api";
import type { Candidate, CandidateStatus, DiscoveryImportProgress, ScanProgress } from "../api/types";
import { useLibraryStore } from "./library";

export const useDiscoveryStore = defineStore("discovery", () => {
  const candidates = ref<Candidate[]>([]), progress = ref<ScanProgress | null>(null), importing = ref<DiscoveryImportProgress | null>(null);
  const scanning = ref(false), fetching = ref<{ done: number; total: number } | null>(null), error = ref(""), filter = ref<CandidateStatus>("new");
  const selected = ref<Set<string>>(new Set()), applyDetails = ref(true);
  const counts = computed(() => {
    const c: Record<CandidateStatus, number> = { new: 0, inLibrary: 0, removed: 0, unreadable: 0 };
    for (const candidate of candidates.value) c[candidate.status]++;
    return c;
  });
  const visible = computed(() => candidates.value.filter(c => c.status === filter.value));
  const needingDetails = computed(() => candidates.value.filter(c => c.status === "new" && !c.suggestion).map(c => c.id));
  let startup: Promise<void> | undefined;
  const unlisteners: (() => void)[] = [];
  function start() {
    return startup ??= (async () => {
      try {
        unlisteners.push(await api.onScanProgress(p => { progress.value = p; }));
        unlisteners.push(await api.onDiscoveryImport(p => { importing.value = p; }));
        unlisteners.push(await api.onDiscoveryDetails(() => { if (fetching.value) fetching.value = { ...fetching.value, done: fetching.value.done + 1 }; }));
        candidates.value = await api.discoveredBooks();
      } catch (e) { error.value = String(e); }
    })();
  }
  async function scan() {
    error.value = ""; scanning.value = true; progress.value = null;
    try { candidates.value = await api.scanForBooks(); selected.value = new Set(); }
    catch (e) { if (!String(e).includes("cancelled")) error.value = String(e); }
    finally { scanning.value = false; }
  }
  async function fetchDetails(ids = needingDetails.value) {
    if (!ids.length) return;
    error.value = ""; fetching.value = { done: 0, total: ids.length };
    try { await api.fetchDiscoveredDetails(ids); candidates.value = await api.discoveredBooks(); }
    catch (e) { error.value = String(e); } finally { fetching.value = null; }
  }
  async function importBooks(ids: string[]) {
    if (!ids.length) return;
    error.value = "";
    const choices = ids.map(id => ({ id, applySuggestion: applyDetails.value && Boolean(candidates.value.find(c => c.id === id)?.suggestion) }));
    try {
      const results = await api.importDiscovered(choices);
      const failed = results.filter(r => r.error);
      if (failed.length) error.value = failed.map(r => `${r.path}: ${r.error}`).join("\n");
      candidates.value = await api.discoveredBooks();
      selected.value = new Set([...selected.value].filter(id => !ids.includes(id)));
      await useLibraryStore().refresh();
    } catch (e) { error.value = String(e); } finally { importing.value = null; }
  }
  function toggle(id: string, on: boolean) { const next = new Set(selected.value); if (on) next.add(id); else next.delete(id); selected.value = next; }
  function dispose() { unlisteners.splice(0).forEach(fn => fn()); startup = undefined; }
  return { candidates, progress, importing, scanning, fetching, error, filter, selected, applyDetails, counts, visible, needingDetails, start, scan, cancel: () => api.cancelScan(), fetchDetails, importBooks, toggle, dispose };
});
