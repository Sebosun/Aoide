<script setup lang="ts">
import { onMounted, onUnmounted, shallowRef, watch } from "vue";
import type { ChapterView } from "@/api/types";
import type { Anchor } from "@/composables/usePopover";
import ReaderSelectionMenu from "@/components/reader/ReaderSelectionMenu.vue";
import { rangeOfSelection } from "@/components/reader/selection";
import { useTextActions, type TextTarget } from "@/components/reader/textActions";

/** Watches what is selected in a chapter's text (`root`, drawn from `chapter`) and opens the
 *  menu on it: when the pointer that selected is released, or once a selection made with the
 *  keyboard has settled. The menu closes when the selection goes. */
const props = defineProps<{ root: HTMLElement | undefined; chapter: ChapterView | undefined }>();
const actions = useTextActions();
/** How long a selection made with the keyboard stays the same before the menu opens. */
const SETTLE_MS = 300;
const open = shallowRef<{ target: TextTarget; anchor: Anchor; focus: boolean } | null>(null);
let pointerDown = false,
  settling: ReturnType<typeof setTimeout> | undefined;

/** A still rectangle around what is selected (the range's, else its first sentence's). */
function rectangle(range: Range): Anchor {
  const rect =
    typeof range.getBoundingClientRect === "function"
      ? range.getBoundingClientRect()
      : (range.startContainer.parentElement?.getBoundingClientRect() ?? new DOMRect());
  return { getBoundingClientRect: () => rect };
}
function openOnSelection(focus = false) {
  clearTimeout(settling);
  const selection = document.getSelection();
  const range =
    selection && props.root && props.chapter ? rangeOfSelection(selection, props.root, props.chapter) : null;
  open.value =
    range && selection
      ? { target: { kind: "selection", range }, anchor: rectangle(selection.getRangeAt(0)), focus }
      : null;
}
function onPointerdown(event: PointerEvent) {
  pointerDown = Boolean(props.root?.contains(event.target as Node));
}
function onPointerup() {
  if (!pointerDown) return;
  pointerDown = false;
  // The selection is final once the release has been handled.
  setTimeout(() => openOnSelection(), 0);
}
function onSelectionchange() {
  clearTimeout(settling);
  const selection = document.getSelection();
  if (!selection || selection.isCollapsed) {
    if (open.value?.target.kind === "selection") open.value = null;
    return;
  }
  if (pointerDown) return;
  settling = setTimeout(() => openOnSelection(), SETTLE_MS);
}
onMounted(() => {
  document.addEventListener("pointerdown", onPointerdown, true);
  document.addEventListener("pointerup", onPointerup, true);
  document.addEventListener("selectionchange", onSelectionchange);
});
onUnmounted(() => {
  clearTimeout(settling);
  document.removeEventListener("pointerdown", onPointerdown, true);
  document.removeEventListener("pointerup", onPointerup, true);
  document.removeEventListener("selectionchange", onSelectionchange);
});
// Another chapter: what was selected is gone.
watch(
  () => props.chapter,
  () => (open.value = null),
);
</script>

<template>
  <ReaderSelectionMenu
    v-if="open"
    :key="JSON.stringify(open.target)"
    :target="open.target"
    :anchor="open.anchor"
    :actions="actions"
    :focus="open.focus"
    @close="open = null"
  />
</template>
