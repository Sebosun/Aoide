<script setup lang="ts">
import { computed, onUnmounted, watch } from "vue";
import { ArrowLeft, Bookmark, Ellipsis, Headphones } from "@lucide/vue";
import ReaderSheet from "../components/reader/ReaderSheet.vue";
import ReaderLines from "../components/reader/ReaderLines.vue";
import ReaderTools from "../components/reader/ReaderTools.vue";
import ChapterMenu from "../components/reader/ChapterMenu.vue";
import ListenButton from "../components/reader/ListenButton.vue";
import AudioReader from "../components/reader/AudioReader.vue";
import { usePlayerControls } from "../composables/usePlayerControls";
import { useReadingTime } from "../composables/useReadingTime";
import { prepareChapter } from "../preparation";
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
    if (reader.shape === "listen") return;
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
useReadingTime();
const backLabel = computed(() => VIEW_NAMES[navigation.readerFrom]);
const { bookmark } = usePlayerControls();
const playingHere = computed(() => player.state.bookId === reader.book?.id);
/** Lines follows the narration, so it is offered only while listening. */
const lines = computed(() => reader.shape === "readAlong" && preferences.appearance.readerMode === "lines");
/** An audiobook on its own: no text, so no chapter menu, Listen button or text tools. */
const audio = computed(() => reader.shape === "listen");
</script>

<template>
  <div v-if="reader.book" class="reader">
    <header class="reader__header">
      <button type="button" class="reader__back" :aria-label="`Back to ${backLabel}`" @click="navigation.leaveReader()">
        <ArrowLeft :size="16" aria-hidden="true" />{{ backLabel }}
      </button>
      <ChapterMenu
        v-if="!audio"
        class="reader__chapters"
        variant="title"
        prepare
        :book-title="reader.book.title"
        :chapters="reader.book.chapters"
        :current="reader.currentChapter"
        :playing="playingHere ? player.state.chapter : null"
        @pick="reader.startChapter($event)"
        @prepare="prepareChapter(reader.book.id, $event)"
      />
      <div class="reader__actions">
        <template v-if="!audio">
          <ListenButton />
          <ReaderTools :lines="reader.shape === 'readAlong'" />
        </template>
        <button
          type="button"
          class="reader__round"
          :aria-label="audio ? 'Bookmark this moment' : 'Bookmark this place'"
          :title="audio ? 'Bookmark this moment' : 'Bookmark this place'"
          @click="bookmark"
        >
          <Bookmark :size="18" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="reader__round"
          aria-label="Search and more"
          :title="audio ? 'Bookmarks and details' : 'Bookmarks, search, narration and details'"
          aria-haspopup="dialog"
          :aria-expanded="Boolean(navigation.tools)"
          @click="openTools(undefined)"
        >
          <Ellipsis :size="18" aria-hidden="true" />
        </button>
      </div>
    </header>
    <AudioReader v-if="audio" />
    <ReaderLines v-else-if="lines" @tools="openTools(undefined)" />
    <ReaderSheet v-else :shape="reader.shape" @tools="openTools" />
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
  flex: none;
  display: grid;
  grid-template-columns: minmax(max-content, 1fr) minmax(0, auto) minmax(max-content, 1fr);
  align-items: center;
  gap: 10px;
  padding: 14px 0;
}
.reader__back {
  justify-self: start;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 44px;
  padding: 0 16px 0 12px;
  border-radius: 22px;
  background: var(--glass);
  font-weight: 600;
}
.reader__chapters {
  justify-self: center;
}
.reader__actions {
  grid-column: -2;
  justify-self: end;
  display: flex;
  align-items: center;
  gap: 8px;
}
.reader__round {
  display: grid;
  place-items: center;
  width: 44px;
  height: 44px;
  border-radius: 50%;
  background: var(--glass);
}
.reader__round[aria-expanded="true"] {
  background: var(--ink);
  color: var(--ground);
}
@media (max-width: 900px) {
  .reader__header {
    grid-template-columns: auto minmax(0, 1fr);
  }
  .reader__chapters {
    grid-column: 1 / -1;
    grid-row: 2;
    justify-self: start;
  }
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
