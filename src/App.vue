<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import AppTopBar from "./components/shell/AppTopBar.vue";
import DropOverlay from "./components/shell/DropOverlay.vue";
import LibraryView from "./views/LibraryView.vue";
import ReaderView from "./views/ReaderView.vue";
import StatsView from "./views/StatsView.vue";
import FindBooksView from "./views/FindBooksView.vue";
import BookDetailsView from "./views/BookDetailsView.vue";
import MiniPlayer from "./components/player/MiniPlayer.vue";
import UiHost from "./components/ui/UiHost.vue";
import SettingsDialog from "./components/settings/SettingsDialog.vue";
import { useLibraryStore } from "./stores/library";
import { usePlayerStore } from "./stores/player";
import { useNavigationStore } from "./stores/navigation";
import { usePreferencesStore } from "./stores/preferences";
import { useShortcuts } from "./composables/useShortcuts";

const library = useLibraryStore(), player = usePlayerStore(), navigation = useNavigationStore(), preferences = usePreferencesStore();
const dragging = ref(false), dropError = ref("");
useShortcuts();
// Screens cross-fade. The library keeps its scroll position for the way back; other screens
// open at the top.
let libraryScroll = 0;
watch(() => navigation.view, (_, from) => { if (from === "library") libraryScroll = window.scrollY; }, { flush: "sync" });
function placeScroll() { window.scrollTo(0, navigation.view === "library" ? libraryScroll : 0); }
const native = isTauri();
let unlisten: (() => void) | undefined, disposed = false;
onMounted(async () => {
  void library.start(); void player.start(); void preferences.start();
  if (native) {
    try {
      const off = await getCurrentWebview().onDragDropEvent(event => {
        // Only files from outside: a drag that starts inside the window carries no paths.
        const type = event.payload.type;
        dragging.value = type === "enter" ? event.payload.paths.length > 0 : type === "over" && dragging.value;
        if (event.payload.type === "drop") { navigation.view = "library"; void library.importFiles(event.payload.paths.filter(path => /\.epub$/i.test(path))); }
      });
      if (disposed) off(); else unlisten = off;
    } catch (e) { dropError.value = `File drop unavailable: ${String(e)}`; }
  }
});
onUnmounted(() => { disposed = true; unlisten?.(); window.removeEventListener("blur", endDrag); void Promise.all([library.start(), player.start()]).then(() => { library.dispose(); player.dispose(); preferences.dispose(); }); });
// The browser path: only drags that carry files count, and nested enter/leave pairs are
// counted so moving over child elements does not flicker or strand the overlay.
let depth = 0;
const carriesFiles = (event: DragEvent) => Array.from(event.dataTransfer?.types ?? []).includes("Files");
function dragEnter(event: DragEvent) { if (!carriesFiles(event)) return; event.preventDefault(); depth += 1; dragging.value = true; }
function dragOver(event: DragEvent) { if (carriesFiles(event)) event.preventDefault(); }
function dragLeave(event: DragEvent) { if (!carriesFiles(event)) return; depth = Math.max(0, depth - 1); if (!depth) dragging.value = false; }
function endDrag() { depth = 0; dragging.value = false; }
window.addEventListener("blur", endDrag);
function browserDrop(event: DragEvent) {
  endDrag();
  if (native || !carriesFiles(event)) return;
  const files = Array.from(event.dataTransfer?.files ?? []).filter(file => /\.epub$/i.test(file.name));
  navigation.view = "library";
  void library.importFiles(files.map(file => file.name));
}
</script>

<template>
  <div class="app" :class="{ 'app--reader': navigation.view === 'reader' }" @dragstart.prevent @dragenter="dragEnter" @dragover="dragOver" @dragleave="dragLeave" @dragend="endDrag" @drop.prevent="browserDrop">
    <AppTopBar />
    <main class="app__main">
      <p v-if="dropError" class="app__error" role="alert">{{ dropError }}</p>
      <Transition name="app__view" mode="out-in" @enter="placeScroll">
        <LibraryView v-if="navigation.view === 'library'" />
        <StatsView v-else-if="navigation.view === 'stats'" />
        <FindBooksView v-else-if="navigation.view === 'find'" />
        <BookDetailsView v-else-if="navigation.view === 'book' && navigation.bookId" :key="navigation.bookId" :book-id="navigation.bookId" />
        <ReaderView v-else />
      </Transition>
    </main>
    <DropOverlay v-if="dragging" />
    <MiniPlayer />
    <SettingsDialog v-if="navigation.settingsOpen" />
    <UiHost />
  </div>
</template>

<style scoped>
.app { min-height: 100vh; display: flex; flex-direction: column; padding-bottom: 110px; }
.app__main { flex: 1; min-width: 0; }
.app--reader { height: 100vh; min-height: 0; padding-bottom: 0; overflow: hidden; }
.app__view-enter-active { transition: opacity var(--dur-2) var(--ease), transform var(--dur-2) var(--ease); }
.app__view-leave-active { transition: opacity var(--dur-1) var(--ease); }
.app__view-enter-from { opacity: 0; transform: translateY(8px); }
.app__view-leave-to { opacity: 0; }
.app__error { margin: 0 28px 12px; padding: 10px 14px; border-radius: 12px; background: var(--page); color: var(--accent); font-weight: 600; }
</style>
