import { computed, getCurrentScope, onScopeDispose, ref, watch } from "vue";
import { api } from "@/api";
import { endsAt, formatDuration, segmentAt } from "@/estimate";
import { useLibraryStore } from "@/stores/library";
import { usePreferencesStore } from "@/stores/preferences";
import { usePlayerStore } from "@/stores/player";
import { useReaderStore } from "@/stores/reader";
import { useUiStore } from "@/stores/ui";

export const SPEEDS = [0.75, 1, 1.25, 1.5, 1.75, 2, 2.5, 3];
export const SLEEP_MINUTES = [5, 15, 30, 45, 60, 90];
/** An arrow key on an audiobook's scrubber moves this far. */
const STEP_MS = 10_000;

/** Transport state and actions shared by the dock, the Lines transport and the mini player. */
export function usePlayerControls() {
  const player = usePlayerStore(),
    reader = useReaderStore(),
    library = useLibraryStore();
  const active = computed(() => player.state.status === "playing" || player.state.status === "buffering");
  /** The book the transport acts on: the playing one, else the open one. */
  const bookId = computed(() => player.state.bookId ?? reader.book?.id ?? null);
  const book = computed(() => {
    const id = bookId.value;
    if (!id) return null;
    const summary = library.books.find((b) => b.id === id);
    const view = player.playingBook?.id === id ? player.playingBook : reader.book?.id === id ? reader.book : null;
    return {
      id,
      title: view?.title ?? summary?.title ?? "Now listening",
      authors: view?.authors ?? summary?.authors ?? [],
      coverUrl: summary?.coverUrl ?? null,
      chapters: view?.chapters ?? [],
    };
  });
  const chapterTitle = computed(
    () =>
      book.value?.chapters.find((c) => c.index === player.state.chapter)?.title ??
      (player.state.bookId ? `Chapter ${player.state.chapter + 1}` : ""),
  );
  /** The playing chapter's text, when it has been loaded. */
  const playingChapter = computed(() =>
    player.state.bookId ? reader.cache[`${player.state.bookId}:${player.state.chapter}`] : undefined,
  );
  const preferences = usePreferencesStore();
  /** The book's time left is shown: chosen, and known (the rest may wait for the sync). */
  const showsBook = computed(() => preferences.appearance.timeLeft === "book" && player.state.bookLeftMs !== null);
  /** Listening time left in the playing chapter or book, as the core counts it. */
  const timeLeftMs = computed(() => (showsBook.value ? player.state.bookLeftMs : player.state.chapterLeftMs));
  const timeLeft = computed(() =>
    timeLeftMs.value === null ? "" : `${formatDuration(timeLeftMs.value)} left${showsBook.value ? " in book" : ""}`,
  );
  /** Ticks to read the clock again: the times left hold still while paused, and the end times
   *  move on all the same. */
  const tick = ref(0);
  if (getCurrentScope()) {
    const clock = setInterval(() => tick.value++, 15_000);
    onScopeDispose(() => clearInterval(clock));
  }
  /** The time left's tooltip: when the chapter and the book end by the clock, and what a click
   *  shows instead. */
  const timeLeftTitle = computed(() => {
    const { chapterLeftMs, bookLeftMs } = player.state;
    if (chapterLeftMs === null) return "";
    void tick.value;
    const at = new Date();
    const ends = [`Chapter ends ${endsAt(chapterLeftMs, at)}`];
    if (bookLeftMs !== null) ends.push(`book ends ${endsAt(bookLeftMs, at)}`);
    return [...ends, `click for the time left in the ${showsBook.value ? "chapter" : "book"}`].join(" · ");
  });
  /** Shows the other time left, and keeps the choice. */
  function toggleTimeLeft() {
    void preferences.update({ timeLeft: preferences.appearance.timeLeft === "book" ? "chapter" : "book" });
  }
  /** Generated audio ahead of the playhead, as a share of the chapter's playing length. */
  const ahead = computed(() => {
    const total = player.state.chapterMs;
    if (!total || player.generation.bookId !== player.state.bookId) return 0;
    return Math.min(1 - player.state.chapterFraction, player.generation.bufferedAheadMs / total);
  });
  async function toggle() {
    if (active.value) await player.pause();
    else if (player.state.status === "paused" && (!reader.book || player.state.bookId === reader.book.id))
      await player.resume();
    else if (reader.book) await player.play(reader.book.id, null, null);
    else if (player.state.bookId) await player.play(player.state.bookId, null, null);
  }
  const sleepChoice = ref(player.state.sleep ? (player.state.sleep.endOfChapter ? "chapter" : "") : "off");
  watch(
    () => player.state.sleep,
    (sleep) => {
      if (!sleep) sleepChoice.value = "off";
    },
  );
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
    const id = player.state.bookId ?? reader.book?.id;
    if (!id) return;
    const ui = useUiStore();
    try {
      await api.addBookmark({ bookId: id, note: "" });
      ui.toast("Bookmarked");
    } catch (e) {
      ui.toast(String(e), "error");
    }
  }
  /** The book's recording is playing, not a speech engine. */
  const recording = computed(() => Boolean(player.state.bookId) && player.state.source === "recording");
  /** The speech engine, when it cannot speak right now (not when it is ready or busy speaking). */
  const engineStatus = computed(() => {
    const engine = player.engine;
    if (recording.value || engine.state === "ready" || engine.state === "busy") return "";
    return engine.state === "error"
      ? `${engine.profileName} has a problem${engine.detail ? `: ${engine.detail}` : ""}`
      : `${engine.profileName} is ${engine.state}`;
  });
  /** Shown under the scrubber only while it matters. */
  const status = computed(() =>
    player.state.waitingForSync
      ? "Waiting for the sync to reach here"
      : player.state.status === "buffering"
        ? "Buffering…"
        : player.state.offText
          ? "Not in the text"
          : engineStatus.value,
  );
  /** The buffer and preparation numbers, for the scrubber's tooltip. */
  const detail = computed(() => {
    const parts: string[] = [];
    if (player.generation.bookId)
      parts.push(
        `${(player.generation.bufferedAheadMs / 1000).toFixed(1)} s buffered · ${player.generation.pending} pending`,
      );
    const preparing = player.generation.preparing;
    if (preparing)
      parts.push(
        `Preparing ${library.books.find((b) => b.id === preparing.bookId)?.title ?? "a book"} · ${preparing.done.toLocaleString()} / ${preparing.total.toLocaleString()}`,
      );
    return parts.join(" · ");
  });
  /** What went wrong. The speech engine's trouble is left out while the recording plays: it
   *  is not reading anything then. */
  const error = computed(
    () => (recording.value ? "" : player.generation.error) || player.state.error || player.error || "",
  );
  const percent = computed(() => Math.round(player.state.chapterFraction * 100));
  /** An audiobook on its own is playing, on its timeline. */
  const timeline = computed(() => Boolean(player.state.bookId) && player.state.positionMs !== null);
  /** Scrubber: play from the sentence at a share of the playing chapter (on an audiobook's
   *  timeline, from that share of the part). */
  function seek(fraction: number) {
    const chapter = playingChapter.value,
      id = player.state.bookId;
    if (id && timeline.value) {
      const { positionMs, partPositionMs } = player.state;
      const start = (positionMs ?? 0) - (partPositionMs ?? 0),
        length = book.value?.chapters.find((c) => c.index === player.state.chapter)?.durationMs ?? 0;
      void player.seek(id, Math.round(start + Math.min(1, Math.max(0, fraction)) * length));
      return;
    }
    const segment = chapter && segmentAt(chapter, fraction);
    if (id && segment) void player.play(id, player.state.chapter, segment);
  }
  /** Scrubber and arrow keys: the next or previous spoken sentence. Fetches the playing
   *  chapter's text when no screen has loaded it yet. */
  async function step(delta: number) {
    const id = player.state.bookId;
    if (!id) return;
    if (timeline.value) return player.skip(id, delta * STEP_MS);
    const chapter = playingChapter.value ?? (await reader.chapterText(id, player.state.chapter).catch(() => undefined));
    const list = chapter?.blocks.flatMap((b) => b.segments).filter((s) => s.speakable) ?? [];
    if (!list.length || player.state.bookId !== id) return;
    const at = list.findIndex((s) => s.id === player.state.segmentId);
    const next = list[Math.min(list.length - 1, Math.max(0, at + delta))];
    if (next) void player.play(id, player.state.chapter, next.id);
  }
  return {
    player,
    active,
    recording,
    timeline,
    bookId,
    book,
    chapterTitle,
    playingChapter,
    timeLeftMs,
    timeLeft,
    timeLeftTitle,
    toggleTimeLeft,
    ahead,
    percent,
    toggle,
    seek,
    step,
    sleepChoice,
    setSleep,
    sleepLabel,
    bookmark,
    status,
    engineStatus,
    detail,
    error,
  };
}
