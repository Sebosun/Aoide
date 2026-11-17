import { onMounted, onUnmounted, ref, watch } from "vue";
import { OVERLAYS } from "@/composables/useShortcuts";

/** How long the pointer and the keys are still before the controls fade. */
export const QUIET_MS = 3_000;
/** Keys that only scroll the text: pressing them is reading, and leaves the controls faded. */
const SCROLLING = new Set(["ArrowUp", "ArrowDown", "PageUp", "PageDown", "Home", "End"]);
/** What the pointer may rest on without the controls fading. */
const CONTROLS =
  'button, a[href], input, select, textarea, summary, [role="button"], [role="slider"], [role="tab"], [role="menuitem"], [role="option"]';

/** A quiet reader: while `reading()` (the text read silently), true once the pointer has not
 *  moved and no key but a scrolling one was pressed for `QUIET_MS`, unless the pointer rests on
 *  a control or something is open over the reader (`OVERLAYS`). Moving the pointer or pressing
 *  another key makes it false again at once. The view fades its controls while it is true. */
export function useQuietReader(reading: () => boolean) {
  const quiet = ref(false);
  let timer: ReturnType<typeof setTimeout> | undefined,
    resting: Element | null = null;
  function settle() {
    timer = undefined;
    if (reading() && !resting?.closest(CONTROLS) && !document.querySelector(OVERLAYS)) quiet.value = true;
  }
  function wake() {
    quiet.value = false;
    clearTimeout(timer);
    timer = reading() ? setTimeout(settle, QUIET_MS) : undefined;
  }
  const onPointer = (event: Event) => {
    resting = event.target instanceof Element ? event.target : null;
    wake();
  };
  const onKey = (event: KeyboardEvent) => {
    if (!SCROLLING.has(event.key)) wake();
  };
  watch(reading, wake);
  onMounted(() => {
    window.addEventListener("pointermove", onPointer, { capture: true, passive: true });
    window.addEventListener("pointerdown", onPointer, { capture: true, passive: true });
    window.addEventListener("keydown", onKey, true);
    wake();
  });
  onUnmounted(() => {
    clearTimeout(timer);
    window.removeEventListener("pointermove", onPointer, { capture: true });
    window.removeEventListener("pointerdown", onPointer, { capture: true });
    window.removeEventListener("keydown", onKey, true);
  });
  return quiet;
}
