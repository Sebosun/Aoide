import type { Component } from "vue";

export interface SelectOption<T extends string | number = string> { value: T; label: string; hint?: string; disabled?: boolean }

/** Something a menu does. With `checked` set it is a checkable item (menuitemcheckbox);
 *  `current` marks the item the menu opens on. */
export interface MenuAction { kind?: "item"; id: string | number; label: string; ariaLabel?: string; icon?: Component; danger?: boolean; disabled?: boolean; current?: boolean; checked?: boolean; action: () => void }
/** One row of a UiMenu. Separators split the menu into groups; a heading names its group. */
export type MenuEntry = MenuAction | { kind: "separator" } | { kind: "heading"; label: string } | { kind: "note"; label: string; icon?: Component };
