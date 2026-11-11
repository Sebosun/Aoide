import { computed, ref, watch } from "vue";
import { api } from "../api";
import { chapterWords, formatDuration, segmentAt, speechMs, wordsLeft } from "../estimate";
import { useLibraryStore } from "../stores/library";
import { usePlayerStore } from "../stores/player";
import { useReaderStore } from "../stores/reader";
import { useUiStore } from "../stores/ui";

export const SPEEDS = [0.75, 1, 1.25, 1.5, 1.75, 2, 2.5, 3];
export const SLEEP_MINUTES = [5, 15, 30, 45, 60, 90];

/** Transport state and actions shared by the dock, the Lines transport and the mini player. */
export function usePlayerControls() {
  const player = usePlayerStore(), reader = useReaderStore(), library = useLibraryStore();
  const active = computed(() => player.state.status === "playing" || player.state.status === "buffering");
  /** The book the transport acts on: the playing one, else the open one. */
  const bookId = computed(() => player.state.bookId ?? reader.book?.id ?? null);
  const book = computed(() => {
    const id = bookId.value;
    if (!id) return null;
    const summary = library.books.find(b => b.id === id);
    const view = player.playingBook?.id === id ? player.playingBook : reader.book?.id === id ? reader.book : null;
    return { id, title: view?.title ?? summary?.title ?? "Now listening", authors: view?.authors ?? summary?.authors ?? [], coverUrl: summary?.coverUrl ?? null, chapters: view?.chapters ?? [] };
  });
  const chapterTitle = computed(() => book.value?.chapters.find(c => c.index === player.state.chapter)?.title ?? (player.state.bookId ? `Chapter ${player.state.chapter + 1}` : ""));
  /** The playing chapter's text, when it has been loaded. */
  const playingChapter = computed(() => player.state.bookId ? reader.cache[`${player.state.bookId}:${player.state.chapter}`] : undefined);
  const timeLeftMs = computed(() => {
    const chapter = playingChapter.value;
    return chapter ? speechMs(wordsLeft(chapter, player.state.segmentId, player.state.wordIndex), player.state.speed) : null;
  });
  const timeLeft = computed(() => timeLeftMs.value === null ? "" : `${formatDuration(timeLeftMs.value)} left`);
  /** Generated audio ahead of the playhead, as a share of the chapter (estimated). */
  const ahead = computed(() => {
    const chapter = playingChapter.value;
    if (!chapter || player.generation.bookId !== player.state.bookId) return 0;
    const total = speechMs(chapterWords(chapter), 1);
    return total ? Math.min(1 - player.state.chapterFraction, player.generation.bufferedAheadMs / total) : 0;
  });
  async function toggle() {
    if (active.value) await player.pause();
    else if (player.state.status === "paused" && (!reader.book || player.state.bookId === reader.book.id)) await player.resume();
    else if (reader.book) await player.play(reader.book.id, null, null);
    else if (player.state.bookId) await player.play(player.state.bookId, null, null);
  }
  const sleepChoice = ref(player.state.sleep ? (player.state.sleep.endOfChapter ? "chapter" : "") : "off");
  watch(() => player.state.sleep, sleep => { if (!sleep) sleepChoice.value = "off"; });
  function setSleep(value: string) {
    sleepChoice.value = value;
    if (value === "off") void player.setSleepTimer(null, false);
    else if (value === "chapter") void player.setSleepTimer(null, true);
    else void player.setSleepTimer(Number(value), false);
  }
  const sleepLabel = computed(() => {
    const sleep = player.state.sleep;
    if (!sleep) return "";
    if (sleep.remainingMs === null) return "End of chapter";
    const seconds = Math.ceil(sleep.remainingMs / 1000);
    return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
  });
  async function bookmark() {
    const id = player.state.bookId ?? reader.book?.id; if (!id) return;
    const ui = useUiStore();
    try { await api.addBookmark(id, ""); ui.toast("Bookmarked"); } catch (e) { ui.toast(String(e), "error"); }
  }
  /** Shown under the scrubber only while it matters. */
  const status = computed(() => (player.state.status === "buffering" ? "Buffering…" : ""));
  /** The buffer and preparation numbers, for the scrubber's tooltip. */
  const detail = computed(() => {
    const parts: string[] = [];
    if (player.generation.bookId) parts.push(`${(player.generation.bufferedAheadMs / 1000).toFixed(1)} s buffered · ${player.generation.pending} pending`);
    const preparing = player.generation.preparing;
    if (preparing) parts.push(`Preparing ${library.books.find(b => b.id === preparing.bookId)?.title ?? "a book"} · ${preparing.done.toLocaleString()} / ${preparing.total.toLocaleString()}`);
    return parts.join(" · ");
  });
  const error = computed(() => player.generation.error || player.state.error || player.error || "");
  const percent = computed(() => Math.round(player.state.chapterFraction * 100));
  /** Scrubber: play from the sentence at a share of the playing chapter. */
  function seek(fraction: number) {
    const chapter = playingChapter.value, id = player.state.bookId;
    const segment = chapter && segmentAt(chapter, fraction);
    if (id && segment) void player.play(id, player.state.chapter, segment);
  }
  /** Scrubber and arrow keys: the next or previous spoken sentence. Fetches the playing
   *  chapter's text when no screen has loaded it yet. */
  async function step(delta: number) {
    const id = player.state.bookId;
    if (!id) return;
    const chapter = playingChapter.value ?? await reader.chapterText(id, player.state.chapter).catch(() => undefined);
    const list = chapter?.blocks.flatMap(b => b.segments).filter(s => s.speakable) ?? [];
    if (!list.length || player.state.bookId !== id) return;
    const at = list.findIndex(s => s.id === player.state.segmentId);
    const next = list[Math.min(list.length - 1, Math.max(0, at + delta))];
    if (next) void player.play(id, player.state.chapter, next.id);
  }
  return { player, active, bookId, book, chapterTitle, playingChapter, timeLeftMs, timeLeft, ahead, percent, toggle, seek, step, sleepChoice, setSleep, sleepLabel, bookmark, status, detail, error };
}
