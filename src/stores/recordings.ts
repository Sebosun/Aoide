import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api } from "@/api";
import type { Narration, RecordingView, SyncAction } from "@/api/types";
import { useReaderStore } from "@/stores/reader";
import { useLibraryStore } from "@/stores/library";
import { useUiStore } from "@/stores/ui";
import { useNavigationStore } from "@/stores/navigation";

/** Books' recordings, kept current from "sync://state", and the pairing dialog and sync report
 *  that are open over any screen. */
export const useRecordingsStore = defineStore("recordings", () => {
  const byBook = ref<Record<string, RecordingView>>({});
  const error = ref("");
  /** Audio dropped or chosen, waiting to be paired with a book. */
  const pairing = ref<{ paths: string[]; bookId: string | null } | null>(null);
  /** The book whose sync report is open. */
  const report = ref<string | null>(null);
  /** Recordings still being synced or needing attention, oldest first (an audiobook on its own
   *  has nothing to sync with). */
  const syncing = computed(() =>
    Object.values(byBook.value).filter((r) => r.stage !== "ready" && r.stage !== "standalone"),
  );
  let startup: Promise<void> | undefined, unlisten: (() => void) | undefined;
  const put = (recording: RecordingView) => {
    const before = byBook.value[recording.bookId]?.stage;
    byBook.value = { ...byBook.value, [recording.bookId]: recording };
    if (before !== recording.stage && (before === "ready" || recording.stage === "ready"))
      void useReaderStore().refreshBook(recording.bookId);
  };
  function start() {
    return (startup ??= (async () => {
      try {
        unlisten = await api.onSyncState(put);
        for (const recording of await api.recordings()) put(recording);
      } catch (e) {
        error.value = String(e);
      }
    })());
  }
  async function load(bookId: string) {
    const recording = await api.recording(bookId);
    if (recording) put(recording);
    else if (byBook.value[bookId]) {
      const { [bookId]: _, ...rest } = byBook.value;
      byBook.value = rest;
      void useReaderStore().refreshBook(bookId);
    }
  }
  async function attach(bookId: string, paths: string[], asListed = false) {
    const recording = await api.attachRecording(bookId, paths, asListed);
    put(recording);
    return recording;
  }
  async function act(bookId: string, action: SyncAction) {
    await api.syncAction(bookId, action);
    await load(bookId);
  }
  async function remove(bookId: string) {
    await api.removeRecording(bookId);
    await load(bookId);
  }
  async function setNarration(bookId: string, narration: Narration) {
    await api.setNarration(bookId, narration);
    await load(bookId);
    await useReaderStore().refreshBook(bookId);
  }
  function openPairing(paths: string[], bookId: string | null = null) {
    pairing.value = { paths, bookId };
  }
  /** Audio dropped or chosen: paired with `bookId` (a book's page or reader is open); else added
   *  on its own, unless its name comes close to a book's, when the pairing dialog offers both. */
  async function receive(paths: string[], bookId: string | null) {
    if (bookId) return openPairing(paths, bookId);
    const suggested = await api.inspectRecording(paths).then(
      (found) => found.suggestedBook,
      () => null,
    );
    if (suggested) openPairing(paths, null);
    else await addOnItsOwn(paths);
  }
  /** Adds an audiobook with no book as an item of its own; a toast names it, or says why not.
   *  Returns its id. */
  async function addOnItsOwn(paths: string[]): Promise<string | null> {
    const ui = useUiStore(),
      library = useLibraryStore();
    try {
      const added = await api.addAudiobook(paths);
      if (!added.bookId) {
        ui.toast(added.error ?? "Could not add the audiobook.", "error");
        return null;
      }
      await library.refresh();
      const title = library.books.find((b) => b.id === added.bookId)?.title ?? "The audiobook";
      ui.toast(`Added “${title}” as an audiobook`);
      void load(added.bookId);
      return added.bookId;
    } catch (e) {
      ui.toast(String(e), "error");
      return null;
    }
  }
  /** "Add the EPUB…" on an audiobook on its own: the chosen EPUB joins the library and takes
   *  the audiobook's recording, which syncs with it; its page opens. */
  async function addText(audiobookId: string) {
    const ui = useUiStore(),
      library = useLibraryStore();
    try {
      const paths = await api.chooseBooks();
      if (!paths?.length) return;
      const [imported] = await api.importFiles(paths.slice(0, 1));
      if (!imported?.bookId) {
        ui.toast(imported?.error ?? "Could not add the EPUB.", "error");
        return;
      }
      await api.mergeAudiobook(audiobookId, imported.bookId);
      await library.refresh();
      await load(imported.bookId);
      const title = library.books.find((b) => b.id === imported.bookId)?.title ?? "The book";
      ui.toast(`“${title}” has its text; the recording syncs with it`);
      useNavigationStore().openItem(imported.bookId);
    } catch (e) {
      ui.toast(String(e), "error");
    }
  }
  function dispose() {
    unlisten?.();
    startup = undefined;
  }
  return {
    byBook,
    error,
    pairing,
    report,
    syncing,
    start,
    load,
    attach,
    act,
    remove,
    setNarration,
    openPairing,
    receive,
    addOnItsOwn,
    addText,
    closePairing: () => {
      pairing.value = null;
    },
    openReport: (bookId: string) => {
      report.value = bookId;
    },
    closeReport: () => {
      report.value = null;
    },
    dispose,
  };
});
