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
  /** A right click opened the menu: WebKit may follow it with a click, which is not one to play. */
  swallowClick = false,
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
  if (!range || !selection) {
    open.value = null;
    return;
  }
  const target: TextTarget = { kind: "selection", range };
  // The same words: the menu stays as it is (and where the focus is).
  if (open.value && JSON.stringify(open.value.target) === JSON.stringify(target)) return;
  open.value = { target, anchor: rectangle(selection.getRangeAt(0)), focus };
}
/** A right click (or the menu key): the menu on what is selected, else on the sentence under
 *  the pointer. Elsewhere the page keeps its own menu. */
function onContextmenu(event: MouseEvent) {
  const root = props.root,
    chapter = props.chapter;
  if (!root || !chapter || !root.contains(event.target as Node)) return;
  const at = new DOMRect(event.clientX, event.clientY, 0, 0),
    anchor = { getBoundingClientRect: () => at };
  const selection = document.getSelection();
  const range = selection && !selection.isCollapsed ? rangeOfSelection(selection, root, chapter) : null;
  if (range) {
    event.preventDefault();
    swallowClick = true;
    open.value = { target: { kind: "selection", range }, anchor, focus: true };
    return;
  }
  const id = (event.target as Element).closest?.("[data-segment-id]")?.getAttribute("data-segment-id");
  if (!id || !chapter.blocks.some((b) => b.segments.some((s) => s.id === id))) return;
  event.preventDefault();
  swallowClick = true;
  open.value = { target: { kind: "sentence", chapter: chapter.index, segmentId: id }, anchor, focus: true };
}
function onClick(event: MouseEvent) {
  if (!swallowClick || !props.root?.contains(event.target as Node)) return;
  swallowClick = false;
  event.stopPropagation();
}
function onPointerdown(event: PointerEvent) {
  swallowClick = false;
  // The main button selects; a right click opens the menu itself (`onContextmenu`).
  pointerDown = event.button === 0 && Boolean(props.root?.contains(event.target as Node));
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
    // A menu that took the focus stays: focusing it may clear the selection for a moment.
    if (open.value?.target.kind === "selection" && !open.value.focus) open.value = null;
    return;
  }
  if (pointerDown) return;
  settling = setTimeout(() => openOnSelection(), SETTLE_MS);
}
onMounted(() => {
  document.addEventListener("pointerdown", onPointerdown, true);
  document.addEventListener("pointerup", onPointerup, true);
  document.addEventListener("selectionchange", onSelectionchange);
  document.addEventListener("contextmenu", onContextmenu);
  document.addEventListener("click", onClick, true);
});
onUnmounted(() => {
  clearTimeout(settling);
  document.removeEventListener("pointerdown", onPointerdown, true);
  document.removeEventListener("pointerup", onPointerup, true);
  document.removeEventListener("selectionchange", onSelectionchange);
  document.removeEventListener("contextmenu", onContextmenu);
  document.removeEventListener("click", onClick, true);
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
