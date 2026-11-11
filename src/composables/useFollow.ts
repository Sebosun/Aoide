import { onUnmounted, ref, type Ref } from "vue";

const CURRENT = ".chapter-text__segment--current";

/** Keeps the spoken sentence in the upper half of a scrolling container. Scrolling by the
 *  reader (wheel, touch, keys, scrollbar; Space plays and pauses instead of scrolling) stops following until `backToPlayhead`. */
export function useFollow(container: Ref<HTMLElement | undefined>, active: () => boolean) {
  const following = ref(true);
  let automatic = false, timer: ReturnType<typeof setTimeout> | undefined;
  const reduced = () => typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  function scrollToCurrent(force: boolean) {
    const box = container.value, element = box?.querySelector<HTMLElement>(CURRENT);
    if (!box || !element) return;
    const view = box.getBoundingClientRect(), rect = element.getBoundingClientRect();
    const inBand = rect.top >= view.top + view.height * 0.08 && rect.bottom < view.top + view.height * 0.6;
    if (inBand && !force) return;
    automatic = true;
    clearTimeout(timer);
    timer = setTimeout(() => { automatic = false; }, 1200);
    box.scrollTo?.({ top: Math.max(0, box.scrollTop + rect.top - view.top - view.height * 0.25), behavior: reduced() ? "auto" : "smooth" });
  }
  /** Called when the spoken sentence changes. */
  function follow() { if (following.value) scrollToCurrent(false); }
  function backToPlayhead() { following.value = true; scrollToCurrent(true); }
  function manual() { if (active()) { following.value = false; automatic = false; } }
  const listeners = {
    onScroll: () => { if (!automatic) manual(); },
    onScrollend: () => { automatic = false; },
    onWheel: manual,
    onTouchmove: manual,
    onKeydown: (event: KeyboardEvent) => {
      if ((event.target as HTMLElement)?.matches?.('input, select, textarea, button, [role="button"], [role="slider"]')) return;
      if (["ArrowDown", "ArrowUp", "PageDown", "PageUp", "Home", "End"].includes(event.key)) manual();
    },
  };
  onUnmounted(() => clearTimeout(timer));
  return { following, follow, backToPlayhead, listeners };
}
