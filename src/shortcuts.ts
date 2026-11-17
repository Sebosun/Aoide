/** The keyboard shortcuts: one table drives both the list (Settings › About) and the handler
 *  (composables/useShortcuts), so they cannot drift apart. */

/** What a shortcut does; useShortcuts has one action per id. */
export type ShortcutId =
  | "playPause"
  | "sentence"
  | "chapter"
  | "jump"
  | "speed"
  | "normalSpeed"
  | "volume"
  | "mute"
  | "bookmark"
  | "highlight"
  | "sleep"
  | "find"
  | "add"
  | "settings"
  | "zoomIn"
  | "zoomOut"
  | "zoomReset"
  | "back"
  | "textMenu"
  | "help";

/** Keys pressed together. `key` is the `KeyboardEvent.key` (a letter in lower case); `also`
 *  are other keys that do the same without being listed (`=` for `+`). `arg` is what the action
 *  is given for this combination (-1 or 1 for a direction). */
export interface KeyCombo {
  key: string;
  ctrl?: boolean;
  shift?: boolean;
  also?: string[];
  arg?: number;
}

/** One row of the list. `typing`: it also works while a text field has the focus (and decides
 *  itself whether it works under a dialog); the others never do either. */
export interface Shortcut {
  id: ShortcutId;
  label: string;
  keys: KeyCombo[];
  typing?: boolean;
}

export const SHORTCUTS: Shortcut[] = [
  { id: "playPause", label: "Play or pause", keys: [{ key: " " }] },
  {
    id: "sentence",
    label: "Previous or next sentence",
    keys: [
      { key: "ArrowLeft", arg: -1 },
      { key: "ArrowRight", arg: 1 },
    ],
  },
  {
    id: "chapter",
    label: "Previous or next chapter",
    keys: [
      { key: "ArrowLeft", shift: true, arg: -1 },
      { key: "ArrowRight", shift: true, arg: 1 },
    ],
  },
  {
    id: "jump",
    label: "Back or forward 30 s, or three sentences",
    keys: [
      { key: "ArrowLeft", ctrl: true, arg: -1 },
      { key: "ArrowRight", ctrl: true, arg: 1 },
    ],
  },
  {
    id: "speed",
    label: "Slower or faster by 0.1×",
    keys: [
      { key: "[", arg: -1 },
      { key: "]", arg: 1 },
    ],
  },
  { id: "normalSpeed", label: "Back to 1×", keys: [{ key: "\\" }] },
  {
    id: "volume",
    label: "Volume up or down by 10%",
    keys: [
      { key: "ArrowUp", shift: true, arg: 1 },
      { key: "ArrowDown", shift: true, arg: -1 },
    ],
  },
  { id: "mute", label: "Mute or unmute", keys: [{ key: "m" }] },
  { id: "bookmark", label: "Bookmark here", keys: [{ key: "b" }] },
  { id: "highlight", label: "Highlight the sentence just spoken", keys: [{ key: "h" }] },
  { id: "sleep", label: "Sleep timer", keys: [{ key: "t" }] },
  { id: "find", label: "Search the open book, or the library", keys: [{ key: "f", ctrl: true }], typing: true },
  { id: "add", label: "Add books or audiobooks", keys: [{ key: "n", ctrl: true }], typing: true },
  { id: "settings", label: "Settings", keys: [{ key: ",", ctrl: true }], typing: true },
  { id: "zoomIn", label: "Larger interface", keys: [{ key: "+", ctrl: true, also: ["="] }], typing: true },
  { id: "zoomOut", label: "Smaller interface", keys: [{ key: "-", ctrl: true, also: ["_"] }], typing: true },
  { id: "zoomReset", label: "Interface at 100%", keys: [{ key: "0", ctrl: true }], typing: true },
  { id: "back", label: "Close a dialog or menu, or leave the reader", keys: [{ key: "Escape" }] },
  {
    id: "textMenu",
    label: "The menu on the selected text, or on the sentence in focus",
    keys: [{ key: "ContextMenu" }, { key: "F10", shift: true }],
  },
  { id: "help", label: "Show this list", keys: [{ key: "?" }] },
];

const NAMES: Record<string, string> = {
  " ": "Space",
  ArrowLeft: "←",
  ArrowRight: "→",
  ArrowUp: "↑",
  ArrowDown: "↓",
  Escape: "Esc",
  ContextMenu: "Menu",
  "-": "−",
};

/** The keys of a combination as they are written on the keyboard: `["Ctrl", "F"]`. */
export function keyLabels(combo: KeyCombo): string[] {
  const key = NAMES[combo.key] ?? (combo.key.length === 1 ? combo.key.toUpperCase() : combo.key);
  return [...(combo.ctrl ? ["Ctrl"] : []), ...(combo.shift ? ["Shift"] : []), key];
}

/** A character that Shift may be needed to type (`?`, `+`), as opposed to a letter or a named key. */
const symbol = (key: string) => key.length === 1 && !/[\p{L}\s]/u.test(key);
const lower = (key: string) => (key.length === 1 ? key.toLowerCase() : key);

/** The row and the combination a key press is, if any. ⌘ counts as Ctrl. Shift must match, except
 *  on a symbol, where it is part of typing the character. */
export function matchShortcut(event: KeyboardEvent): { shortcut: Shortcut; combo: KeyCombo } | null {
  const key = lower(event.key),
    ctrl = event.ctrlKey || event.metaKey;
  for (const shortcut of SHORTCUTS)
    for (const combo of shortcut.keys)
      if (
        [combo.key, ...(combo.also ?? [])].map(lower).includes(key) &&
        Boolean(combo.ctrl) === ctrl &&
        (symbol(key) || Boolean(combo.shift) === event.shiftKey)
      )
        return { shortcut, combo };
  return null;
}
