import { onMounted, onUnmounted } from "vue";
import { useNavigationStore } from "../stores/navigation";
import { usePreferencesStore } from "../stores/preferences";
import { useUiStore } from "../stores/ui";
import { stepZoom, zoomLabel } from "../zoom";
import { usePlayerControls } from "./usePlayerControls";

/** Pixels of Ctrl + wheel per zoom step: one notch of a mouse wheel, or a short pinch. */
const WHEEL_STEP = 40;
const TYPING = "input, textarea, select";
/** Things that use Space themselves, when the keyboard put the focus on them. */
const PRESSABLE = 'button, a[href], summary, [role="button"], [role="slider"], [role="tab"], [role="menuitem"], [role="menuitemcheckbox"], [role="option"], [role="listbox"]';
/** Things that use the arrow keys themselves. */
const ARROWS = '[role="slider"], [role="tablist"], [role="menu"], [role="listbox"]';

/** App-wide keys, listened for on the window (mounted once by App); src/shortcuts.ts lists them
 *  for Settings › About.
 *  - Space plays or pauses and ← / → move a sentence. Both are left alone while typing, on a
 *    control that uses the key (for Space: one reached with the keyboard), under a dialog, and
 *    when a component already handled the key.
 *  - Ctrl F searches the open book (Now Playing) or the library; Ctrl , opens Settings.
 *  - Ctrl + / Ctrl − / Ctrl 0 and Ctrl + wheel zoom, also while typing. */
export function useShortcuts() {
  const preferences = usePreferencesStore(), ui = useUiStore(), navigation = useNavigationStore();
  const controls = usePlayerControls();

  // Whether the keyboard moved the focus to where it is. A button that was clicked keeps the
  // focus, and Space should still play or pause then. (`:focus-visible` cannot tell: browsers
  // turn it on as soon as any key is pressed.)
  let lastInput: "pointer" | "key" = "key", keyboardFocus = true;
  const onPointerdown = () => { lastInput = "pointer"; };
  const onAnyKey = () => { lastInput = "key"; };
  const onFocusin = () => { keyboardFocus = lastInput === "key"; };

  function zoom(to: number) {
    if (to !== preferences.appearance.zoom) void preferences.update({ zoom: to });
    ui.toast(`Zoom ${zoomLabel(to)}`, "info", "zoom");
  }
  function withCtrl(event: KeyboardEvent) {
    const current = preferences.appearance.zoom, key = event.key.toLowerCase();
    if (key === "+" || key === "=") zoom(stepZoom(current, 1));
    else if (key === "-" || key === "_") zoom(stepZoom(current, -1));
    else if (key === "0") zoom(1);
    else if (key === "," || key === "f") {
      // Not over another dialog: Settings only toggles itself, search only reaches book tools.
      const dialog = document.querySelector('[aria-modal="true"]')?.getAttribute("aria-label") ?? null;
      const field = document.querySelector<HTMLElement>('[aria-label="Search library"]');
      if (key === "," && navigation.settingsOpen) navigation.closeSettings();
      else if (key === "," && !dialog) navigation.openSettings();
      else if (key === "f" && navigation.view === "reader" && (!dialog || dialog === "Book tools")) navigation.findInBook();
      else if (key === "f" && navigation.view === "library" && !dialog && field) field.focus();
      else return false;
    }
    else return false;
    return true;
  }
  function plain(event: KeyboardEvent) {
    const target = event.target instanceof Element ? event.target : null;
    if (target?.matches(TYPING) || (target as HTMLElement | null)?.isContentEditable) return false;
    if (document.querySelector('[aria-modal="true"]')) return false;
    const pressable = target?.closest(PRESSABLE);
    if (event.key === " " && controls.bookId.value && !(pressable && keyboardFocus)) { spaceHeld = true; void controls.toggle(); }
    else if ((event.key === "ArrowLeft" || event.key === "ArrowRight") && controls.player.state.bookId && !target?.closest(ARROWS)) void controls.step(event.key === "ArrowRight" ? 1 : -1);
    else return false;
    return true;
  }
  // A button acts on Space when the key comes up, so that is swallowed too.
  let spaceHeld = false;
  function onKeyup(event: KeyboardEvent) { if (event.key === " " && spaceHeld) { spaceHeld = false; event.preventDefault(); } }
  function onKeydown(event: KeyboardEvent) {
    if (event.defaultPrevented || event.altKey) return;
    const ctrl = event.ctrlKey || event.metaKey;
    if (ctrl ? withCtrl(event) : !event.shiftKey && plain(event)) event.preventDefault();
  }
  let travelled = 0, lastWheel = 0;
  function onWheel(event: WheelEvent) {
    if (!event.ctrlKey && !event.metaKey) return;
    event.preventDefault();
    if (event.timeStamp - lastWheel > 400) travelled = 0;
    lastWheel = event.timeStamp;
    travelled += event.deltaMode === WheelEvent.DOM_DELTA_PIXEL ? event.deltaY : Math.sign(event.deltaY) * WHEEL_STEP;
    if (Math.abs(travelled) < WHEEL_STEP) return;
    zoom(stepZoom(preferences.appearance.zoom, travelled < 0 ? 1 : -1));
    travelled = 0;
  }
  onMounted(() => {
    window.addEventListener("pointerdown", onPointerdown, true);
    window.addEventListener("keydown", onAnyKey, true);
    window.addEventListener("focusin", onFocusin, true);
    window.addEventListener("keydown", onKeydown);
    window.addEventListener("keyup", onKeyup);
    window.addEventListener("wheel", onWheel, { passive: false });
  });
  onUnmounted(() => {
    window.removeEventListener("pointerdown", onPointerdown, true);
    window.removeEventListener("keydown", onAnyKey, true);
    window.removeEventListener("focusin", onFocusin, true);
    window.removeEventListener("keydown", onKeydown);
    window.removeEventListener("keyup", onKeyup);
    window.removeEventListener("wheel", onWheel);
  });
}
