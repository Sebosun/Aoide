import { onMounted, onUnmounted, watch, type Ref } from "vue";
import { api } from "../api";
import { useLibraryStore } from "../stores/library";
import { useReaderStore } from "../stores/reader";

const SPOKEN = ".chapter-text__segment:not(.chapter-text__segment--quiet)";

/** While `active` (the book is read silently), keeps the core told where the reader is: the
 *  first sentence whose top edge is on screen, or the chapter's last sentence once it is
 *  scrolled to the end. Reports at most once a second, and only a change. On opening a book
 *  it scrolls to the saved place. Call `schedule` on each scroll. */
export function useReadingPlace(container: Ref<HTMLElement | undefined>, active: () => boolean) {
  const reader = useReaderStore(),
    library = useLibraryStore();
  let timer: ReturnType<typeof setTimeout> | undefined,
    lastAt = -Infinity,
    restored: string | null = null,
    restoring = false;

  function top(): string | null {
    const box = container.value;
    if (!box) return null;
    const segments = [...box.querySelectorAll<HTMLElement>(SPOKEN)];
    const scrollable = box.scrollHeight > box.clientHeight;
    if (scrollable && box.scrollTop + box.clientHeight >= box.scrollHeight - 2)
      return segments.at(-1)?.dataset.segmentId ?? null;
    const view = box.getBoundingClientRect();
    const first = segments.find((segment) => {
      const rect = segment.getBoundingClientRect();
      return rect.top >= view.top && rect.top < view.bottom;
    });
    return first?.dataset.segmentId ?? null;
  }
  function report() {
    timer = undefined;
    const book = reader.book?.id,
      chapter = reader.currentChapter;
    if (!active() || !book) return;
    const segmentId = top();
    if (!segmentId || (reader.place?.chapter === chapter && reader.place.segmentId === segmentId)) return;
    lastAt = Date.now();
    reader.place = { chapter, segmentId };
    api.setReadingPlace(book, chapter, segmentId).then(
      (progress) => {
        if (progress) library.setProgress(book, progress);
      },
      () => {
        // Logged by the API layer; the next report tries again.
      },
    );
  }
  function schedule() {
    if (timer || restoring || !active()) return;
    timer = setTimeout(report, Math.max(0, lastAt + 1000 - Date.now()));
  }
  // Another chapter picked while reading: the reader is at its start now.
  watch(
    () => reader.currentChapter,
    () => schedule(),
    { flush: "post" },
  );
  // A book opened to be read: show its saved place.
  function restore() {
    const id = reader.book?.id,
      chapter = reader.chapter,
      box = container.value;
    if (!id || !chapter || !box || restored === id || !active()) return;
    restored = id;
    const place = reader.place;
    if (place?.chapter !== chapter.index) return;
    const element = [...box.querySelectorAll<HTMLElement>("[data-segment-id]")].find(
      (element) => element.dataset.segmentId === place.segmentId,
    );
    if (!element) return;
    // The reading font may still be loading: text set in it grows, and WebKit does not keep
    // the scroll anchored, so scroll once it is in, and report nothing before.
    restoring = true;
    const show = () => {
      element.scrollIntoView?.({ block: "start" });
      // The scroll this makes is not the reader's: a place near the chapter's end lands at the
      // bottom, which would otherwise count as having read to the end.
      setTimeout(() => {
        restoring = false;
      }, 400);
    };
    const fonts = typeof document.fonts?.ready?.then === "function" ? document.fonts.ready : null;
    if (fonts) void fonts.then(show, show);
    else show();
  }
  watch(() => [reader.book?.id, reader.chapter, container.value], restore, { flush: "post" });
  onMounted(restore);
  onUnmounted(() => {
    clearTimeout(timer);
    restoring = false;
  });
  return { schedule };
}
