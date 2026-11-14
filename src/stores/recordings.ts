import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api } from "../api";
import type { Narration, RecordingView, SyncAction } from "../api/types";

/** Books' recordings, kept current from "sync://state", and the pairing dialog and sync report
 *  that are open over any screen. */
export const useRecordingsStore = defineStore("recordings", () => {
  const byBook = ref<Record<string, RecordingView>>({});
  const error = ref("");
  /** Audio dropped or chosen, waiting to be paired with a book. */
  const pairing = ref<{ paths: string[]; bookId: string | null } | null>(null);
  /** The book whose sync report is open. */
  const report = ref<string | null>(null);
  /** Recordings still being synced or needing attention, oldest first. */
  const syncing = computed(() => Object.values(byBook.value).filter(r => r.stage !== "ready"));
  let startup: Promise<void> | undefined, unlisten: (() => void) | undefined;
  const put = (recording: RecordingView) => { byBook.value = { ...byBook.value, [recording.bookId]: recording }; };
  function start() {
    return startup ??= (async () => {
      try {
        unlisten = await api.onSyncState(put);
        for (const recording of await api.recordings()) put(recording);
      } catch (e) { error.value = String(e); }
    })();
  }
  async function load(bookId: string) {
    const recording = await api.recording(bookId);
    if (recording) put(recording);
    else { const { [bookId]: _, ...rest } = byBook.value; byBook.value = rest; }
  }
  async function attach(bookId: string, paths: string[]) { const recording = await api.attachRecording(bookId, paths); put(recording); return recording; }
  async function act(bookId: string, action: SyncAction) { await api.syncAction(bookId, action); await load(bookId); }
  async function remove(bookId: string) { await api.removeRecording(bookId); await load(bookId); }
  async function setNarration(bookId: string, narration: Narration) { await api.setNarration(bookId, narration); }
  function openPairing(paths: string[], bookId: string | null = null) { pairing.value = { paths, bookId }; }
  function dispose() { unlisten?.(); startup = undefined; }
  return { byBook, error, pairing, report, syncing, start, load, attach, act, remove, setNarration, openPairing, closePairing: () => { pairing.value = null; }, openReport: (bookId: string) => { report.value = bookId; }, closeReport: () => { report.value = null; }, dispose };
});
