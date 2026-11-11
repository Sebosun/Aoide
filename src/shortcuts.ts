/** The keyboard shortcuts, as listed in Settings › About (handled in composables/useShortcuts).
 *  Each entry has one or more alternatives; an alternative is the keys pressed together. */
export const SHORTCUTS: { keys: string[][]; action: string }[] = [
  { keys: [["Space"]], action: "Play or pause" },
  { keys: [["←"], ["→"]], action: "Previous or next sentence" },
  { keys: [["Ctrl", "F"]], action: "Search the open book, or the library" },
  { keys: [["Ctrl", ","]], action: "Settings" },
  { keys: [["Ctrl", "+"]], action: "Larger interface" },
  { keys: [["Ctrl", "−"]], action: "Smaller interface" },
  { keys: [["Ctrl", "0"]], action: "Interface at 100%" },
  { keys: [["Esc"]], action: "Close a dialog or menu" },
];
