<script setup lang="ts">
import { computed, shallowRef, ref } from "vue";
import { Check } from "@lucide/vue";
import { useMenu } from "../../composables/useMenu";
import { usePopover } from "../../composables/usePopover";
import type { MenuAction, MenuEntry } from "./types";

/** A popup menu. The default slot draws the button and gets `toggle`, `open` and the ARIA
 *  attributes to bind (`trigger`); `items` are the rows. The list is placed next to the button
 *  (below, or above when there is more room) and stays inside the window. Arrow keys move,
 *  Escape and outside clicks close; picking an item closes the menu, returns focus to the
 *  button and runs the item's action. The `item` slot can draw a row's content. */
const props = withDefaults(defineProps<{ label: string; items: MenuEntry[]; align?: "start" | "end"; width?: number; maxHeight?: number }>(), { align: "start", width: 230, maxHeight: 420 });
defineSlots<{
  default(props: { open: boolean; toggle: () => void; trigger: { "aria-haspopup": "menu"; "aria-expanded": boolean } }): unknown;
  item?(props: { item: MenuAction; index: number }): unknown;
}>();

const root = ref<HTMLElement>(), list = ref<HTMLElement>(), button = shallowRef<HTMLElement>();
const { open, close, onKeydown } = useMenu(root, list, button);
const { style, above } = usePopover(button, list, open, { align: props.align, maxHeight: props.maxHeight });
function toggle() {
  button.value = root.value?.querySelector<HTMLElement>('[aria-haspopup="menu"]') ?? undefined;
  open.value = !open.value;
}
function pick(item: MenuAction) { close(true); item.action(); }

/** The rows between separators; a group that starts with a heading is named by it. */
const groups = computed(() => {
  const all: { label?: string; rows: { entry: Exclude<MenuEntry, { kind: "separator" }>; index: number }[] }[] = [{ rows: [] }];
  let index = 0;
  for (const entry of props.items) {
    if (entry.kind === "separator") { all.push({ rows: [] }); continue; }
    const group = all[all.length - 1];
    if (entry.kind === "heading" && !group.rows.length) group.label = entry.label;
    group.rows.push({ entry, index: entry.kind === undefined || entry.kind === "item" ? index++ : -1 });
  }
  return all.filter(group => group.rows.length);
});
const isAction = (entry: MenuEntry): entry is MenuAction => entry.kind === undefined || entry.kind === "item";
</script>

<template>
  <div ref="root" class="ui-menu">
    <slot :open="open" :toggle="toggle" :trigger="{ 'aria-haspopup': 'menu', 'aria-expanded': open }" />
    <Teleport to="body">
      <div v-if="open" ref="list" class="ui-menu__list" :class="[`ui-menu__list--${align}`, { 'ui-menu__list--above': above }]" role="menu" :aria-label="label" :style="{ ...style, width: `${width}px` }" @keydown="onKeydown">
        <div v-for="(group, g) in groups" :key="g" class="ui-menu__group" :role="group.label ? 'group' : undefined" :aria-label="group.label">
          <template v-for="({ entry, index }, r) in group.rows" :key="isAction(entry) ? entry.id : `${entry.kind}:${r}`">
            <span v-if="entry.kind === 'heading'" class="ui-menu__heading" aria-hidden="true">{{ entry.label }}</span>
            <p v-else-if="entry.kind === 'note'" class="ui-menu__note"><component :is="entry.icon" v-if="entry.icon" :size="15" aria-hidden="true" /> {{ entry.label }}</p>
            <button v-else type="button" class="ui-menu__item" :class="{ 'ui-menu__item--danger': entry.danger, 'ui-menu__item--current': entry.current }" :role="entry.checked === undefined ? 'menuitem' : 'menuitemcheckbox'" :aria-checked="entry.checked" :aria-current="entry.current ? 'true' : undefined" :aria-label="entry.ariaLabel" :disabled="entry.disabled" @click="pick(entry)">
              <slot name="item" :item="entry" :index="index">
                <Check v-if="entry.checked !== undefined" :size="15" class="ui-menu__check" aria-hidden="true" /><component :is="entry.icon" v-else-if="entry.icon" :size="15" aria-hidden="true" />
                <span class="ui-menu__label">{{ entry.label }}</span>
              </slot>
            </button>
          </template>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.ui-menu__list { position: fixed; z-index: 40; max-width: calc(100vw - 16px); overflow-y: auto; overscroll-behavior: contain; padding: 6px; border-radius: 14px; background: var(--page); color: var(--ink); font: 600 13px/1.3 var(--font-ui); box-shadow: 0 18px 40px -16px var(--shadow), 0 0 0 1px var(--line); transform-origin: top left; animation: ui-menu-in var(--dur-1) var(--ease); }
.ui-menu__list--end { transform-origin: top right; }
.ui-menu__list--above { transform-origin: bottom left; }
.ui-menu__list--above.ui-menu__list--end { transform-origin: bottom right; }
@keyframes ui-menu-in { from { opacity: 0; transform: scale(.97); } }
.ui-menu__group { display: grid; gap: 1px; }
.ui-menu__group + .ui-menu__group { margin-top: 4px; padding-top: 4px; border-top: 1px solid var(--line); }
.ui-menu__heading { padding: 4px 10px 2px; font-size: 11px; font-weight: 700; letter-spacing: .06em; text-transform: uppercase; color: var(--faint); }
.ui-menu__item { display: flex; align-items: center; gap: 9px; width: 100%; min-width: 0; padding: 9px 10px; border-radius: 9px; text-align: left; }
.ui-menu__item:hover:not(:disabled), .ui-menu__item:focus-visible { background: var(--wash); }
.ui-menu__item svg { color: var(--dim); }
.ui-menu__item--danger, .ui-menu__item--danger svg { color: var(--accent); }
.ui-menu__item--current { background: var(--glass); font-weight: 700; }
.ui-menu__check { visibility: hidden; }
.ui-menu__item[aria-checked="true"] .ui-menu__check { visibility: visible; color: var(--accent); }
.ui-menu__label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ui-menu__note { display: flex; align-items: center; gap: 9px; padding: 8px 10px 6px; font-size: 12.5px; font-weight: 500; color: var(--dim); }
</style>
