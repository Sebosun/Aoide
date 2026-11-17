<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { SpellCheck, X } from "@lucide/vue";
import type { Pronunciation } from "@/api/types";
import UiButton from "@/components/ui/UiButton.vue";
import UiDialog from "@/components/ui/UiDialog.vue";
import PronunciationForm from "@/components/settings/PronunciationForm.vue";
import { fewWords } from "@/components/reader/selection";
import { useNavigationStore } from "@/stores/navigation";
import { useReaderStore } from "@/stores/reader";
import { useUiStore } from "@/stores/ui";

/** "Say this differently…" while a word or a few are selected in the chapter text: a small
 *  dialog with the words filled in, to add them to the open book's pronunciation list. The menu
 *  on selected text opens the same dialog (`navigation.sayDifferently`). */
const reader = useReaderStore(),
  navigation = useNavigationStore(),
  ui = useUiStore();
const selected = ref("");

/** The selection's words, when they are in the chapter text and few enough to be a name or a
 *  term. */
function words(): string {
  const selection = document.getSelection();
  if (!selection || selection.isCollapsed || !selection.rangeCount) return "";
  const node = selection.getRangeAt(0).commonAncestorContainer;
  const element = node instanceof Element ? node : node.parentElement;
  if (!element?.closest(".chapter-text")) return "";
  return fewWords(selection.toString());
}
const follow = () => {
  if (navigation.sayingDifferently === null) selected.value = words();
};
onMounted(() => document.addEventListener("selectionchange", follow));
onUnmounted(() => {
  document.removeEventListener("selectionchange", follow);
  navigation.closeSayDifferently();
});

function saved(entry: Pronunciation) {
  navigation.closeSayDifferently();
  selected.value = "";
  ui.toast(`“${entry.written}” is said as “${entry.spoken}” in this book now.`);
}
</script>

<template>
  <!-- Keeps the selection: pressing the button does not take the focus. -->
  <UiButton
    v-if="selected && reader.book"
    variant="glass"
    size="lg"
    icon
    aria-label="Say this differently…"
    :title="`Say “${selected}” differently…`"
    @mousedown.prevent
    @click="navigation.sayDifferently(selected)"
  >
    <SpellCheck :size="18" aria-hidden="true" />
  </UiButton>
  <UiDialog
    v-if="navigation.sayingDifferently !== null && reader.book"
    label="Say this differently"
    initial-focus="input[aria-label='Said as']"
    @close="navigation.closeSayDifferently()"
  >
    <div class="say-differently">
      <header class="say-differently__head">
        <h2 class="say-differently__title">Say this differently</h2>
        <UiButton variant="glass" size="sm" icon aria-label="Close" @click="navigation.closeSayDifferently()"
          ><X :size="17"
        /></UiButton>
      </header>
      <p class="say-differently__hint">
        The page keeps the book's spelling; the voice says it as you write it here, in every sentence of this book.
      </p>
      <PronunciationForm
        :book-id="reader.book.id"
        :written="navigation.sayingDifferently"
        save-label="Save for this book"
        @saved="saved"
      />
    </div>
  </UiDialog>
</template>

<style scoped>
.say-differently {
  width: min(440px, calc(100vw - 32px));
  display: grid;
  gap: var(--space-6);
  padding: var(--space-12);
}
.say-differently__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-6);
}
.say-differently__title {
  margin: 0;
  font-size: 18px;
  font-weight: 800;
  letter-spacing: -0.01em;
}
.say-differently__hint {
  margin: 0;
  font-size: 12px;
  color: var(--dim);
}
</style>
