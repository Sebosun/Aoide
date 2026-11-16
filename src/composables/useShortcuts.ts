import { onMounted, onUnmounted } from "vue";
import { useNavigationStore } from "@/stores/navigation";
import { usePreferencesStore } from "@/stores/preferences";
import { useUiStore } from "@/stores/ui";
import { stepZoom, zoomLabel } from "@/zoom";
import { usePlayerControls } from "@/composables/usePlayerControls";
import { matchShortcut, type ShortcutId } from "@/shortcuts";

/** Pixels of Ctrl + wheel per zoom step: one notch of a mouse wheel, or a short pinch. */
const WHEEL_STEP = 40;
const TYPING = "input, textarea, select";
/** Things that use Space themselves, when the keyboard put the focus on them. */
const PRESSABLE =
  'button, a[href], summary, [role="button"], [role="slider"], [role="tab"], [role="menuitem"], [role="menuitemcheckbox"], [role="option"], [role="listbox"]';
/** Open things that Escape closes before it may leave the reader. */
const OVERLAYS = '[aria-modal="true"], [role="dialog"], [role="menu"], [role="listbox"], [aria-expanded="true"]';
/** Things that use the arrow keys themselves. */
const ARROWS = '[role="slider"], [role="tablist"], [role="menu"], [role="listbox"]';

/** The speeds the player plays at (the audio player keeps to them too). */
const MIN_SPEED = 0.5,
  MAX_SPEED = 3;
/** A tenth up or down from `value`, landing on whole tenths (1.25 goes to 1.3 or 1.2), within
 *  `min` and `max`. */
function stepTenth(value: number, direction: number, min: number, max: number) {
  const tenths = direction > 0 ? Math.floor(value * 10 + 1e-6) + 1 : Math.ceil(value * 10 - 1e-6) - 1;
  return Math.min(max, Math.max(min, tenths / 10));
}
const volumeLabel = (volume: number) => (volume > 0 ? `Volume ${Math.round(volume * 100)}%` : "Muted");

/** App-wide keys, listened for on the window (mounted once by App). Which keys there are comes
 *  from the table in src/shortcuts.ts, which Settings › About also lists; this says what each does.
 *  - Keys that do not work while typing are also left alone under a dialog.
 *  - Space plays or pauses and ← / → move a sentence. Both are left alone while typing, on a
 *    control that uses the key (for Space: one reached with the keyboard), under a dialog, and
 *    when a component already handled the key.
 *  - Shift ← / → skip a chapter; Ctrl ← / → move 30 s on an audiobook's timeline, else three
 *    sentences (and move by word in a text field, as usual).
 *  - [ and ] step the speed by 0.1×, \\ puts it back to 1×; Shift ↑ / ↓ step the volume by 10%
 *    (↑ and ↓ alone keep scrolling); M mutes. Each says the new value in a toast.
 *  - B bookmarks the playing (or open) book's place; T opens the player's sleep timer menu.
 *  - Escape leaves the reader for the screen it was opened from, when nothing is open over it.
 *  - Ctrl F searches the open book (in the reader) or the library; Ctrl , opens Settings;
 *    Ctrl N opens Add.
 *  - Ctrl + / Ctrl − / Ctrl 0 and Ctrl + wheel zoom, also while typing. */
export function useShortcuts() {
  const preferences = usePreferencesStore(),
    ui = useUiStore(),
    navigation = useNavigationStore();
  const controls = usePlayerControls();

  // Whether the keyboard moved the focus to where it is. A button that was clicked keeps the
  // focus, and Space should still play or pause then. (`:focus-visible` cannot tell: browsers
  // turn it on as soon as any key is pressed.)
  let lastInput: "pointer" | "key" = "key",
    keyboardFocus = true;
  const onPointerdown = () => {
    lastInput = "pointer";
  };
  const onAnyKey = () => {
    lastInput = "key";
  };
  const onFocusin = () => {
    keyboardFocus = lastInput === "key";
  };

  function zoom(to: number) {
    if (to !== preferences.appearance.zoom) void preferences.update({ zoom: to });
    ui.toast(`Zoom ${zoomLabel(to)}`, "info", "zoom");
  }
  function setSpeed(speed: number) {
    void controls.player.setSpeed(speed);
    ui.toast(`Speed ${speed}×`, "info", "speed");
  }
  function setVolume(volume: number) {
    void controls.player.setVolume(volume);
    ui.toast(volumeLabel(volume), "info", "volume");
  }
  let spaceHeld = false;
  const dialogOpen = () => document.querySelector('[aria-modal="true"]');
  /** What each shortcut does, given its combination's `arg` and the focused element. One that
   *  returns false did nothing, and leaves the key to the page. */
  const actions: Record<ShortcutId, (arg: number, target: Element | null) => boolean | void> = {
    playPause(_, target) {
      if (!controls.bookId.value || (target?.closest(PRESSABLE) && keyboardFocus)) return false;
      spaceHeld = true;
      void controls.toggle();
    },
    sentence(arg, target) {
      if (!controls.player.state.bookId || target?.closest(ARROWS)) return false;
      void controls.step(arg);
    },
    chapter(arg) {
      if (!controls.player.state.bookId) return false;
      void controls.player.skipChapter(arg);
    },
    jump(arg) {
      // `step` moves 10 s on an audiobook's timeline: three steps are 30 s, or three sentences.
      if (!controls.player.state.bookId) return false;
      void controls.step(arg * 3);
    },
    speed: (arg) => setSpeed(stepTenth(controls.player.state.speed, arg, MIN_SPEED, MAX_SPEED)),
    normalSpeed: () => setSpeed(1),
    volume: (arg) => setVolume(stepTenth(controls.player.state.volume, arg, 0, 1)),
    mute() {
      void controls.player.toggleMute();
      ui.toast(volumeLabel(controls.player.state.volume), "info", "volume");
    },
    bookmark() {
      if (!controls.bookId.value) return false;
      void controls.bookmark();
    },
    sleep() {
      if (!controls.bookId.value) return false;
      controls.player.askSleep();
    },
    find() {
      // Search only reaches book tools, never past another dialog.
      const dialog = dialogOpen()?.getAttribute("aria-label") ?? null;
      const field = document.querySelector<HTMLElement>('[aria-label="Search library"]');
      if (navigation.view === "reader" && (!dialog || dialog === "Book tools")) navigation.findInBook();
      else if (navigation.view === "library" && !dialog && field) field.focus();
      else return false;
    },
    add() {
      if (dialogOpen()) return false;
      navigation.openAdd();
    },
    settings() {
      // Settings toggles itself, and does not open over another dialog.
      if (navigation.settingsOpen) navigation.closeSettings();
      else if (!dialogOpen()) navigation.openSettings();
      else return false;
    },
    zoomIn: () => zoom(stepZoom(preferences.appearance.zoom, 1)),
    zoomOut: () => zoom(stepZoom(preferences.appearance.zoom, -1)),
    zoomReset: () => zoom(1),
    back() {
      if (navigation.view !== "reader" || document.querySelector(OVERLAYS)) return false;
      navigation.leaveReader();
    },
  };
  function onKeydown(event: KeyboardEvent) {
    if (event.defaultPrevented || event.altKey) return;
    const found = matchShortcut(event);
    if (!found) return;
    const target = event.target instanceof Element ? event.target : null;
    const typing = target?.matches(TYPING) || (target as HTMLElement | null)?.isContentEditable;
    if (!found.shortcut.typing && (typing || dialogOpen())) return;
    if (actions[found.shortcut.id](found.combo.arg ?? 0, target) !== false) event.preventDefault();
  }
  // A button acts on Space when the key comes up, so that is swallowed too.
  function onKeyup(event: KeyboardEvent) {
    if (event.key === " " && spaceHeld) {
      spaceHeld = false;
      event.preventDefault();
    }
  }
  let travelled = 0,
    lastWheel = 0;
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
  // Ctrl + wheel must cancel the wheel, so its listener cannot be passive; and while any
  // listener that may cancel is on, every scroll waits for the page's script before it moves.
  // So it is on only while Ctrl (or ⌘) is held: from its key going down, or from the pointer's
  // modifier state when it went down outside the window.
  let zoomWheel = false;
  const isCtrl = (event: KeyboardEvent) => event.key === "Control" || event.key === "Meta";
  function wheelZoom(on: boolean) {
    if (on === zoomWheel) return;
    zoomWheel = on;
    if (on) window.addEventListener("wheel", onWheel, { passive: false });
    else window.removeEventListener("wheel", onWheel);
  }
  const held = (event: KeyboardEvent | MouseEvent) => event.ctrlKey || event.metaKey;
  const onCtrlDown = (event: KeyboardEvent) => isCtrl(event) && wheelZoom(true);
  // The other Ctrl (or ⌘) may still be down.
  const onCtrlUp = (event: KeyboardEvent) => isCtrl(event) && wheelZoom(held(event));
  const onPointer = (event: MouseEvent) => wheelZoom(held(event));
  const onBlur = () => wheelZoom(false);
  onMounted(() => {
    window.addEventListener("pointerdown", onPointerdown, true);
    window.addEventListener("keydown", onAnyKey, true);
    window.addEventListener("focusin", onFocusin, true);
    window.addEventListener("keydown", onKeydown);
    window.addEventListener("keyup", onKeyup);
    window.addEventListener("keydown", onCtrlDown, true);
    window.addEventListener("keyup", onCtrlUp, true);
    window.addEventListener("pointermove", onPointer, { capture: true, passive: true });
    window.addEventListener("blur", onBlur);
  });
  onUnmounted(() => {
    window.removeEventListener("pointerdown", onPointerdown, true);
    window.removeEventListener("keydown", onAnyKey, true);
    window.removeEventListener("focusin", onFocusin, true);
    window.removeEventListener("keydown", onKeydown);
    window.removeEventListener("keyup", onKeyup);
    window.removeEventListener("keydown", onCtrlDown, true);
    window.removeEventListener("keyup", onCtrlUp, true);
    window.removeEventListener("pointermove", onPointer, { capture: true });
    window.removeEventListener("blur", onBlur);
    wheelZoom(false);
  });
}
