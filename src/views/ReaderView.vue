<script setup lang="ts">
import { onUnmounted, watch } from "vue";
import ReaderSheet from "../components/reader/ReaderSheet.vue";
import ReaderLines from "../components/reader/ReaderLines.vue";
import { usePreferencesStore } from "../stores/preferences";
import BookToolsSheet from "../components/book/BookToolsSheet.vue";
import { useNavigationStore, type ToolsTab } from "../stores/navigation";
import { useReaderStore } from "../stores/reader";
import { usePlayerStore } from "../stores/player";

const reader = useReaderStore(), player = usePlayerStore(), preferences = usePreferencesStore(), navigation = useNavigationStore();
// The open book follows the playhead into its next chapter.
watch(() => [reader.book?.id, player.state.bookId, player.state.chapter], async () => {
  if (player.state.bookId === reader.book?.id && player.state.chapter !== reader.currentChapter) await reader.loadChapter(player.state.chapter);
}, { immediate: true });

function openTools(tab: ToolsTab | undefined) { if (navigation.tools && !tab) navigation.closeTools(); else navigation.openTools(tab); }
onUnmounted(() => navigation.closeTools());
</script>

<template>
  <div v-if="reader.book" class="reader">
    <ReaderLines v-if="preferences.appearance.readerMode === 'lines'" @tools="openTools(undefined)" />
    <ReaderSheet v-else @tools="openTools" />
    <BookToolsSheet v-if="navigation.tools" :initial-tab="navigation.tools" @close="navigation.closeTools()" />
  </div>
</template>

<style scoped>
.reader { height: calc(100vh - 64px); display: flex; padding: 4px 28px 0; }
@media (max-width: 760px) { .reader { padding: 4px 10px 0; } }
</style>
