<script setup lang="ts">
import { computed, onUnmounted, ref, shallowRef, watch } from "vue";
import { Copy, Trash2 } from "@lucide/vue";
import { usePopover, type Anchor } from "@/composables/usePopover";
import { useReaderStore } from "@/stores/reader";
import { useUiStore } from "@/stores/ui";

/** A highlight clicked in the chapter drawn in `root` (the page, or Lines) opens a small card
 *  beside it: its note to write, Copy and Remove. The click does not play the sentence. The
 *  card closes on Escape or a click elsewhere. */
const props = defineProps<{ root: HTMLElement | undefined }>();
const reader = useReaderStore(),
  ui = useUiStore();
const openId = ref<number | null>(null),
  anchor = shallowRef<Anchor>(),
  card = ref<HTMLElement>(),
  open = computed(() => openId.value !== null);
const highlight = computed(() => reader.highlights.find((h) => h.id === openId.value) ?? null);
const { style } = usePopover(anchor, card, open, { prefer: "below", matchWidth: false, maxHeight: 360 });

function close() {
  openId.value = null;
}
/** Opens the card for a highlight clicked with the main button, when nothing is selected. */
function onClick(event: MouseEvent) {
  const mark = (event.target as Element | null)?.closest?.("mark[data-highlight-id]");
  const selection = document.getSelection();
  if (!mark || event.button !== 0 || (selection && !selection.isCollapsed)) return;
  event.stopPropagation();
  event.preventDefault();
  anchor.value = mark;
  openId.value = Number(mark.getAttribute("data-highlight-id"));
}
watch(
  () => props.root,
  (root, was) => {
    was?.removeEventListener("click", onClick, true);
    root?.addEventListener("click", onClick, true);
  },
  { immediate: true },
);
// Gone from the list (removed elsewhere): nothing to show.
watch(highlight, (now) => now || close());
function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape" && open.value) {
    event.preventDefault();
    close();
  }
}
function outside(event: PointerEvent) {
  if (open.value && !card.value?.contains(event.target as Node)) close();
}
document.addEventListener("keydown", onKeydown);
document.addEventListener("pointerdown", outside, true);
onUnmounted(() => {
  props.root?.removeEventListener("click", onClick, true);
  document.removeEventListener("keydown", onKeydown);
  document.removeEventListener("pointerdown", outside, true);
});

async function saveNote(event: Event) {
  const id = openId.value,
    note = (event.target as HTMLTextAreaElement).value.trim();
  if (id === null || note === highlight.value?.note) return;
  try {
    await reader.noteHighlight(id, note);
  } catch (e) {
    ui.toast(String(e), "error");
  }
}
async function copy() {
  const text = highlight.value?.text ?? "";
  try {
    await navigator.clipboard.writeText(text);
    ui.toast("Copied", "info", "copy");
  } catch (e) {
    ui.toast(`Could not copy: ${e}`, "error");
  }
}
async function remove() {
  const id = openId.value;
  if (id === null) return;
  close();
  try {
    await reader.removeHighlight(id);
  } catch (e) {
    ui.toast(String(e), "error");
  }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="highlight" ref="card" class="highlight-card" role="dialog" aria-label="Highlight" :style="style">
      <blockquote class="highlight-card__text">{{ highlight.text }}</blockquote>
      <textarea
        class="highlight-card__note"
        rows="3"
        aria-label="Note"
        placeholder="Add a note"
        :value="highlight.note"
        @change="saveNote"
      />
      <div class="highlight-card__actions">
        <button type="button" class="highlight-card__button" @click="copy">
          <Copy :size="14" aria-hidden="true" />Copy
        </button>
        <button type="button" class="highlight-card__button" @click="remove">
          <Trash2 :size="14" aria-hidden="true" />Remove
        </button>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.highlight-card {
  position: fixed;
  z-index: 40;
  display: grid;
  gap: var(--space-4);
  width: min(320px, calc(100vw - 16px));
  padding: var(--space-5);
  border-radius: var(--radius-md);
  background: var(--page);
  color: var(--ink);
  font: 500 13px/1.4 var(--font-ui);
  box-shadow:
    0 18px 40px -16px var(--shadow),
    0 0 0 1px var(--line);
  animation: highlight-card-in var(--dur-1) var(--ease);
}
@keyframes highlight-card-in {
  from {
    opacity: 0;
    transform: scale(0.97);
  }
}
.highlight-card__text {
  margin: 0;
  padding-left: var(--space-4);
  border-left: 3px solid var(--mark);
  max-height: 6.5em;
  overflow: hidden;
  color: var(--dim);
  font: 400 14px/1.45 var(--read-font);
  display: -webkit-box;
  -webkit-line-clamp: 4;
  -webkit-box-orient: vertical;
}
.highlight-card__note {
  width: 100%;
  resize: vertical;
  font: inherit;
}
.highlight-card__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
}
.highlight-card__button {
  display: inline-flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-sm);
  font-weight: 600;
}
.highlight-card__button:hover,
.highlight-card__button:focus-visible {
  background: var(--wash);
}
.highlight-card__button svg {
  color: var(--dim);
}
</style>
