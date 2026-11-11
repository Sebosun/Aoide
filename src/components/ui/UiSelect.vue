<script setup lang="ts" generic="T extends string | number">
import { computed, onUnmounted, ref, useId, watch } from "vue";
import { Check, ChevronDown } from "@lucide/vue";
import { usePopover } from "../../composables/usePopover";
import type { SelectOption } from "./types";

/** A dropdown that replaces the native <select>: a button that opens a listbox of options.
 *  Arrow keys, Home/End and Page keys move; typing jumps to a matching option; Enter or Space
 *  picks; Escape closes. `field` looks like a form field, `pill` like the player's pills
 *  (the `value` slot draws the button content). */
const props = withDefaults(defineProps<{ modelValue: T; options: SelectOption<T>[]; label: string; variant?: "field" | "pill"; placeholder?: string }>(), { variant: "field", placeholder: "Choose…" });
const emit = defineEmits<{ "update:modelValue": [value: T] }>();
defineSlots<{ value?(props: { option: SelectOption<T> | undefined }): unknown }>();

const id = useId();
const open = ref(false), active = ref(-1), keyboard = ref(false);
const button = ref<HTMLButtonElement>(), list = ref<HTMLElement>();
const { style, above } = usePopover(button, list, open);
const selected = computed(() => props.options.find(o => o.value === props.modelValue));
const enabled = (i: number) => i >= 0 && i < props.options.length && !props.options[i].disabled;

function show() {
  const at = props.options.findIndex(o => o.value === props.modelValue);
  active.value = enabled(at) ? at : props.options.findIndex(o => !o.disabled);
  keyboard.value = false;
  open.value = true;
}
function close(refocus = false) { open.value = false; if (refocus) button.value?.focus(); }
function pick(i: number) {
  if (!enabled(i)) return;
  if (props.options[i].value !== props.modelValue) emit("update:modelValue", props.options[i].value);
  close(true);
}
function moveTo(i: number, step: number) {
  const last = props.options.length - 1;
  for (let at = Math.max(0, Math.min(last, i)); at >= 0 && at <= last; at += step) if (enabled(at)) { active.value = at; return; }
}
let typed = "", typedTimer: ReturnType<typeof setTimeout> | undefined;
function typeahead(char: string) {
  typed += char.toLowerCase();
  clearTimeout(typedTimer);
  typedTimer = setTimeout(() => { typed = ""; }, 700);
  const count = props.options.length, start = typed.length === 1 ? active.value + 1 : active.value;
  for (let k = 0; k < count; k++) {
    const at = (((start + k) % count) + count) % count;
    if (enabled(at) && props.options[at].label.toLowerCase().startsWith(typed)) { active.value = at; return; }
  }
}
function onButtonKey(event: KeyboardEvent) {
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
  event.preventDefault();
  show();
  keyboard.value = true;
}
function onListKey(event: KeyboardEvent) {
  const key = event.key;
  keyboard.value = true;
  if (key === "Escape") { event.preventDefault(); event.stopPropagation(); close(true); }
  else if (key === "Tab") { event.preventDefault(); close(true); }
  else if (key === "Enter" || key === " ") { event.preventDefault(); pick(active.value); }
  else if (key === "ArrowDown") { event.preventDefault(); moveTo(active.value + 1, 1); }
  else if (key === "ArrowUp") { event.preventDefault(); moveTo(active.value - 1, -1); }
  else if (key === "Home") { event.preventDefault(); moveTo(0, 1); }
  else if (key === "End") { event.preventDefault(); moveTo(props.options.length - 1, -1); }
  else if (key === "PageDown") { event.preventDefault(); moveTo(active.value + 8, -1); }
  else if (key === "PageUp") { event.preventDefault(); moveTo(active.value - 8, 1); }
  else if (key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) { event.preventDefault(); typeahead(key); }
}
function outside(event: PointerEvent) {
  const target = event.target as Node;
  if (!button.value?.contains(target) && !list.value?.contains(target)) close();
}
watch(open, value => {
  document.removeEventListener("pointerdown", outside);
  if (!value) return;
  document.addEventListener("pointerdown", outside);
});
watch([active, open], () => { if (open.value) document.getElementById(`${id}-${active.value}`)?.scrollIntoView?.({ block: "nearest" }); }, { flush: "post" });
watch(list, element => element?.focus(), { flush: "post" });
onUnmounted(() => { document.removeEventListener("pointerdown", outside); clearTimeout(typedTimer); });
</script>

<template>
  <button ref="button" type="button" class="ui-select" :class="`ui-select--${variant}`" :aria-label="label" aria-haspopup="listbox" :aria-expanded="open" :aria-controls="open ? `${id}-list` : undefined" @click="open ? close() : show()" @keydown="onButtonKey">
    <slot name="value" :option="selected"><span class="ui-select__value">{{ selected?.label ?? placeholder }}</span><ChevronDown :size="15" class="ui-select__chevron" aria-hidden="true" /></slot>
    <Teleport to="body">
      <ul v-if="open" :id="`${id}-list`" ref="list" class="ui-select__list" :class="{ 'ui-select__list--above': above, 'ui-select__list--keyboard': keyboard }" role="listbox" tabindex="-1" :aria-label="label" :aria-activedescendant="active >= 0 ? `${id}-${active}` : undefined" :style="style" @keydown="onListKey" @focusout="!list?.contains($event.relatedTarget as Node) && $event.relatedTarget && close()">
        <li v-for="(option, i) in options" :id="`${id}-${i}`" :key="String(option.value)" role="option" class="ui-select__option" :class="{ 'ui-select__option--active': i === active, 'ui-select__option--selected': option.value === modelValue }" :aria-selected="option.value === modelValue" :aria-disabled="option.disabled || undefined" @click="pick(i)" @pointermove="enabled(i) && (active = i, keyboard = false)">
          <span class="ui-select__label">{{ option.label }}</span><small v-if="option.hint" class="ui-select__hint">{{ option.hint }}</small><Check v-if="option.value === modelValue" :size="15" class="ui-select__check" aria-hidden="true" />
        </li>
      </ul>
    </Teleport>
  </button>
</template>

<style scoped>
.ui-select { display: inline-flex; align-items: center; gap: 8px; min-width: 0; text-align: left; white-space: nowrap; }
.ui-select--field { justify-content: space-between; padding: 8px 10px 8px 12px; border: 1px solid var(--line); border-radius: 10px; background: var(--page); font-weight: 600; }
.ui-select--field:hover { border-color: var(--faint); }
.ui-select--field[aria-expanded="true"] { border-color: var(--accent); }
.ui-select--pill { height: 34px; padding: 0 13px; border-radius: 999px; background: var(--glass); font-weight: 700; font-size: 13px; font-variant-numeric: tabular-nums; }
.ui-select__value { overflow: hidden; text-overflow: ellipsis; }
.ui-select__chevron { color: var(--dim); transition: transform var(--dur-1) var(--ease); }
.ui-select[aria-expanded="true"] .ui-select__chevron { transform: rotate(180deg); }
.ui-select__list { position: fixed; z-index: 40; max-width: calc(100vw - 16px); margin: 0; padding: 6px; list-style: none; overflow-y: auto; overscroll-behavior: contain; border-radius: 14px; background: var(--page); color: var(--ink); font: 600 13px/1.3 var(--font-ui); box-shadow: 0 18px 40px -16px var(--shadow), 0 0 0 1px var(--line); transform-origin: top center; animation: ui-select-in var(--dur-1) var(--ease); }
.ui-select__list--above { transform-origin: bottom center; }
.ui-select__list:focus-visible { outline: none; }
@keyframes ui-select-in { from { opacity: 0; transform: scale(.97); } }
.ui-select__option { display: grid; grid-template-columns: minmax(0, 1fr) auto auto; align-items: center; gap: 10px; padding: 8px 10px; border-radius: 9px; cursor: pointer; white-space: nowrap; }
.ui-select__option--active { background: var(--wash); }
.ui-select__list--keyboard .ui-select__option--active { box-shadow: inset 0 0 0 2px var(--accent); }
.ui-select__option--selected { font-weight: 800; }
.ui-select__option[aria-disabled="true"] { opacity: .45; cursor: not-allowed; }
.ui-select__label { overflow: hidden; text-overflow: ellipsis; }
.ui-select__hint { color: var(--dim); font-weight: 500; font-size: 12px; }
.ui-select__check { grid-column: 3; color: var(--accent); }
</style>
