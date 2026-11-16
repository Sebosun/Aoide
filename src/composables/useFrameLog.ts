import { onMounted, onUnmounted, watch } from "vue";
import { api } from "@/api";
import type { UiLog } from "@/api/contract";
import { beginShowing, canCount, cancelShowing, countFrames, endShowing, frameLine } from "@/frames";

/** A scroll has ended once no scroll event came for this long. */
const SETTLED_MS = 300;

/** Logs how smoothly the window drew (`ui.frames`): each scroll of any scroller, and each change
 *  of `view` (or opening, begun by the reader store) until `shown` is called once the new view
 *  has come in. */
export function useFrameLog(view: () => string) {
  const send = (entry: UiLog) => void api.logUi({ entry }).catch(() => {});
  let scrolling: ReturnType<typeof countFrames> | null = null,
    settle: ReturnType<typeof setTimeout> | undefined;
  function scrolled() {
    if (!canCount()) return;
    scrolling ??= countFrames();
    clearTimeout(settle);
    settle = setTimeout(() => {
      const report = scrolling?.stop();
      scrolling = null;
      if (report) send(frameLine("scroll", view(), report));
    }, SETTLED_MS);
  }
  watch(view, () => beginShowing("view"), { flush: "sync" });
  function shown() {
    const done = endShowing();
    if (done) send(frameLine(done.motion, view(), done.report));
  }
  // Scroll events do not bubble, but a capturing listener on the document sees every scroller's.
  onMounted(() => document.addEventListener("scroll", scrolled, { capture: true, passive: true }));
  onUnmounted(() => {
    document.removeEventListener("scroll", scrolled, { capture: true });
    clearTimeout(settle);
    scrolling?.stop();
    cancelShowing();
  });
  return { shown };
}
