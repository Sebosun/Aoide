import { nextTick, onUnmounted, ref, watch, type Ref } from "vue";

/** CSS zoom on the root element (the browser preview's interface zoom): rectangles are measured
 *  in window pixels, lengths set on the popup are multiplied by this. 1 in the desktop app,
 *  where the webview itself zooms. */
export const rootZoom = () => Number(getComputedStyle(document.documentElement).getPropertyValue("zoom")) || 1;
/** Anything with a rectangle on screen: an element, a range of selected text, a point. */
export interface Anchor {
  getBoundingClientRect(): DOMRect;
}
/** Places a popup (teleported to the body, `position: fixed`) next to its anchor: below it, or
 *  above when there is more room there (with `prefer: "above"`, above whenever it fits); kept
 *  inside the window, and following the anchor when the page scrolls or the window resizes.
 *  The popup is at least as wide as the anchor unless `matchWidth` is false. */
export function usePopover(
  anchor: Ref<Anchor | undefined>,
  popup: Ref<HTMLElement | undefined>,
  open: Ref<boolean>,
  options: { align?: "start" | "end"; maxHeight?: number; prefer?: "below" | "above"; matchWidth?: boolean } = {},
) {
  const style = ref<Record<string, string>>({});
  const above = ref(false);
  function place() {
    const at = anchor.value;
    if (!at || !open.value) return;
    const r = at.getBoundingClientRect(),
      vw = window.innerWidth,
      vh = window.innerHeight,
      gap = 6,
      margin = 8,
      zoom = rootZoom();
    const px = (length: number) => `${Math.round(length / zoom)}px`;
    const want = options.maxHeight ?? 320;
    const below = vh - r.bottom - gap - margin,
      over = r.top - gap - margin;
    const natural = Math.min((popup.value?.scrollHeight || want) * zoom, want);
    above.value = options.prefer === "above" ? over >= natural || over > below : below < natural && over > below;
    const width = (popup.value?.offsetWidth ?? 0) * zoom;
    const left = Math.max(margin, Math.min(options.align === "end" ? r.right - width : r.left, vw - margin - width));
    style.value = {
      ...(options.matchWidth === false ? {} : { minWidth: px(r.width) }),
      maxHeight: px(Math.max(96, Math.min(want, above.value ? over : below))),
      left: px(left),
      ...(above.value ? { bottom: px(vh - r.top + gap) } : { top: px(r.bottom + gap) }),
    };
  }
  function stop() {
    window.removeEventListener("resize", place);
    window.removeEventListener("scroll", place, true);
  }
  watch(open, async (value) => {
    stop();
    if (!value) return;
    place();
    await nextTick();
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
  });
  onUnmounted(stop);
  return { style, above, place };
}
