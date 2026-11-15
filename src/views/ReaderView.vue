<script setup lang="ts">
import { computed, onUnmounted, watch } from "vue";
import { ArrowLeft, Headphones } from "@lucide/vue";
import ReaderSheet from "../components/reader/ReaderSheet.vue";
import ReaderLines from "../components/reader/ReaderLines.vue";
import { usePreferencesStore } from "../stores/preferences";
import BookToolsSheet from "../components/book/BookToolsSheet.vue";
import { useNavigationStore, VIEW_NAMES, type ToolsTab } from "../stores/navigation";
import { useReaderStore } from "../stores/reader";
import { usePlayerStore } from "../stores/player";

const reader = useReaderStore(),
  player = usePlayerStore(),
  preferences = usePreferencesStore(),
  navigation = useNavigationStore();
// The open book follows the playhead into its next chapter.
watch(
  () => [reader.book?.id, player.state.bookId, player.state.chapter],
  async () => {
    if (player.state.bookId === reader.book?.id && player.state.chapter !== reader.currentChapter)
      await reader.loadChapter(player.state.chapter);
  },
  { immediate: true },
);

/** The recording is playing narration that is not in the book's text (credits, a dedication). */
const offText = computed(
  () => player.state.bookId === reader.book?.id && player.state.offText && player.state.status === "playing",
);
function openTools(tab: ToolsTab | undefined) {
  if (navigation.tools && !tab) navigation.closeTools();
  else navigation.openTools(tab);
}
onUnmounted(() => navigation.closeTools());
const backLabel = computed(() => VIEW_NAMES[navigation.readerFrom]);
</script>

<template>
  <div v-if="reader.book" class="reader">
    <header class="reader__header">
      <button type="button" class="reader__back" :aria-label="`Back to ${backLabel}`" @click="navigation.leaveReader()">
        <ArrowLeft :size="18" aria-hidden="true" />{{ backLabel }}
      </button>
    </header>
    <ReaderLines v-if="preferences.appearance.readerMode === 'lines'" @tools="openTools(undefined)" />
    <ReaderSheet v-else @tools="openTools" />
    <Transition name="reader__chip">
      <p v-if="offText" class="reader__chip" role="status">
        <Headphones :size="14" aria-hidden="true" /> Not in the text
      </p>
    </Transition>
    <BookToolsSheet v-if="navigation.tools" :initial-tab="navigation.tools" @close="navigation.closeTools()" />
  </div>
</template>

<style scoped>
.reader {
  position: relative;
  height: 100vh;
  display: flex;
  flex-direction: column;
  padding: 0 28px;
}
.reader__header {
  height: 60px;
  flex: none;
  display: flex;
  align-items: center;
}
.reader__back {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 36px;
  padding: 0 12px 0 8px;
  margin-left: -8px;
  border-radius: 12px;
  font-weight: 600;
  color: var(--dim);
}
.reader__back:hover {
  color: var(--ink);
  background: var(--glass);
}
.reader__chip {
  position: absolute;
  top: 14px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 2;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 7px 14px;
  border-radius: 999px;
  background: var(--ink);
  color: var(--ground);
  font-size: 12.5px;
  font-weight: 700;
  box-shadow: 0 10px 24px -12px var(--shadow);
  pointer-events: none;
}
.reader__chip-enter-active,
.reader__chip-leave-active {
  transition: opacity var(--dur-2) var(--ease);
}
.reader__chip-enter-from,
.reader__chip-leave-to {
  opacity: 0;
}
@media (max-width: 760px) {
  .reader {
    padding: 0 10px;
  }
}
</style>
