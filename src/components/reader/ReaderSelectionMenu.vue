<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, shallowRef } from "vue";
import { usePopover, type Anchor } from "@/composables/usePopover";
import { applies, logTextAction, type TextAction, type TextTarget } from "@/components/reader/textActions";

/** The small menu over selected text (or beside a sentence that was right-clicked): the entries
 *  of `actions` that apply to `target`. It sits above `anchor` when there is room, takes the
 *  focus only when asked (`focus`: opened from the keyboard), and closes on Escape, a click
 *  elsewhere, scrolling, or picking an entry. Pressing it keeps the page's selection. */
const props = defineProps<{ target: TextTarget; anchor: Anchor; actions: TextAction[]; focus?: boolean }>();
const emit = defineEmits<{ close: [] }>();
const list = ref<HTMLElement>(),
  open = ref(false),
  at = shallowRef<Anchor | undefined>(props.anchor);
const { style, above } = usePopover(at, list, open, { prefer: "above", matchWidth: false, maxHeight: 280 });
const shown = computed(() => props.actions.filter((action) => applies(action, props.target)));
/** Where the focus was when the menu took it, to give it back on Escape. */
let before: HTMLElement | null = null;

function close(refocus = false) {
  emit("close");
  if (refocus) before?.focus();
}
async function pick(action: TextAction) {
  logTextAction(action.id);
  close(props.focus);
  await action.run(props.target);
}
const items = () => [...(list.value?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    close(props.focus);
    return;
  }
  if (!list.value?.contains(document.activeElement)) return;
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp" && event.key !== "Home" && event.key !== "End") {
    if (event.key === "Tab") close(true);
    return;
  }
  event.preventDefault();
  const all = items(),
    index = all.indexOf(document.activeElement as HTMLElement);
  const next =
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? all.length - 1
        : (index + (event.key === "ArrowDown" ? 1 : -1) + all.length) % all.length;
  all[next]?.focus();
}
function outside(event: PointerEvent) {
  if (!list.value?.contains(event.target as Node)) close();
}
const onScroll = (event: Event) => {
  if (!list.value?.contains(event.target as Node)) close();
};
onMounted(async () => {
  // Placed once it is drawn (the popover places what opens).
  open.value = true;
  document.addEventListener("keydown", onKeydown);
  document.addEventListener("pointerdown", outside, true);
  window.addEventListener("scroll", onScroll, true);
  if (!props.focus) return;
  before = document.activeElement as HTMLElement | null;
  await nextTick();
  items()[0]?.focus();
});
onUnmounted(() => {
  document.removeEventListener("keydown", onKeydown);
  document.removeEventListener("pointerdown", outside, true);
  window.removeEventListener("scroll", onScroll, true);
});
</script>

<template>
  <Teleport to="body">
    <div
      v-if="shown.length"
      ref="list"
      class="selection-menu"
      :class="{ 'selection-menu--above': above }"
      role="menu"
      :aria-label="target.kind === 'sentence' ? 'Sentence' : 'Selected text'"
      :style="style"
      @mousedown.prevent
    >
      <button
        v-for="action in shown"
        :key="action.id"
        type="button"
        role="menuitem"
        class="selection-menu__item"
        :tabindex="focus ? 0 : -1"
        @click="pick(action)"
      >
        <component :is="action.icon" v-if="action.icon" :size="15" aria-hidden="true" />
        <span>{{ action.label }}</span>
      </button>
    </div>
  </Teleport>
</template>

<style scoped>
.selection-menu {
  position: fixed;
  z-index: 40;
  display: grid;
  gap: 1px;
  min-width: 190px;
  max-width: calc(100vw - 16px);
  padding: var(--space-3);
  border-radius: var(--radius-md);
  background: var(--page);
  color: var(--ink);
  font: 600 13px/1.3 var(--font-ui);
  box-shadow:
    0 18px 40px -16px var(--shadow),
    0 0 0 1px var(--line);
  transform-origin: top left;
  animation: selection-menu-in var(--dur-1) var(--ease);
}
.selection-menu--above {
  transform-origin: bottom left;
}
@keyframes selection-menu-in {
  from {
    opacity: 0;
    transform: scale(0.97);
  }
}
.selection-menu__item {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  width: 100%;
  padding: var(--space-4) var(--space-5);
  border-radius: var(--radius-sm);
  text-align: left;
  white-space: nowrap;
}
.selection-menu__item:hover,
.selection-menu__item:focus-visible {
  background: var(--wash);
}
.selection-menu__item svg {
  color: var(--dim);
}
</style>
