import { onMounted, onUnmounted } from "vue";
import { api } from "../api";
import { useReaderStore } from "../stores/reader";

/** How often the reader tells the core it is being read, at most. */
export const REPORT_MS = 15_000;
/** Input this recent means someone is reading. */
export const IDLE_MS = 60_000;
const INPUT = ["scroll", "wheel", "keydown", "pointerdown", "pointermove"] as const;

/** Counts reading time: while the reader is open on a book read silently, the window has focus
 *  and there was a scroll, key or pointer event in the last minute, it tells the core every
 *  15 s at most (the core credits the time between reports, capped). Mount it with the
 *  reader. */
export function useReadingTime() {
  const reader = useReaderStore();
  let lastInput = -Infinity,
    lastSent = -Infinity,
    timer: ReturnType<typeof setInterval> | undefined;
  const reading = () =>
    Boolean(reader.book) && reader.shape === "read" && document.hasFocus() && Date.now() - lastInput < IDLE_MS;
  function send() {
    const id = reader.book?.id;
    if (!id || Date.now() - lastSent < REPORT_MS || !reading()) return;
    lastSent = Date.now();
    api.readingActivity(id).catch(() => {
      // Logged by the API layer; the next report counts the time since the last one.
    });
  }
  function input() {
    lastInput = Date.now();
    send();
  }
  onMounted(() => {
    for (const name of INPUT) window.addEventListener(name, input, { capture: true, passive: true });
    timer = setInterval(send, REPORT_MS);
  });
  onUnmounted(() => {
    for (const name of INPUT) window.removeEventListener(name, input, { capture: true });
    clearInterval(timer);
  });
}
