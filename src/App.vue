<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import AppSidebar from "./components/shell/AppSidebar.vue";
import DropOverlay from "./components/shell/DropOverlay.vue";
import HomeView from "./views/HomeView.vue";
import LibraryView from "./views/LibraryView.vue";
import ReaderView from "./views/ReaderView.vue";
import ActivityView from "./views/ActivityView.vue";
import FeedsView from "./views/FeedsView.vue";
import FindBooksView from "./views/FindBooksView.vue";
import FindAudiobooksView from "./views/FindAudiobooksView.vue";
import ItemView from "./views/ItemView.vue";
import UiHost from "./components/ui/UiHost.vue";
import SignInDialog from "./components/article/SignInDialog.vue";
import { useSignInStore } from "./stores/signin";
import SettingsDialog from "./components/settings/SettingsDialog.vue";
import AddDialog from "./components/library/AddDialog.vue";
import PairRecordingDialog from "./components/recording/PairRecordingDialog.vue";
import SyncReport from "./components/recording/SyncReport.vue";
import { useLibraryStore } from "./stores/library";
import { usePlayerStore } from "./stores/player";
import { useNavigationStore } from "./stores/navigation";
import { usePreferencesStore } from "./stores/preferences";
import { listenForProblems } from "./problems";
import { api } from "./api";
import { useRecordingsStore } from "./stores/recordings";
import { useArticlesStore } from "./stores/articles";
import { useReaderStore } from "./stores/reader";
import { isAudioPath } from "./recording";
import { asLink } from "./articles";
import { useShortcuts } from "./composables/useShortcuts";

const library = useLibraryStore(),
  player = usePlayerStore(),
  navigation = useNavigationStore(),
  preferences = usePreferencesStore();
const recordings = useRecordingsStore(),
  reader = useReaderStore(),
  signIn = useSignInStore();
/** Dropped EPUBs join the library; dropped audio (files or a folder) is paired with the book whose
 *  page or reader is open, else added on its own unless its name comes close to a book's. */
function dropped(paths: string[]) {
  const books = paths.filter((path) => /\.epub$/i.test(path)),
    audio = paths.filter(isAudioPath);
  if (audio.length) {
    const open =
      navigation.view === "item" ? navigation.bookId : navigation.view === "reader" ? (reader.book?.id ?? null) : null;
    // Only a book (an EPUB) is paired with audio.
    const book = library.books.find((b) => b.id === open && b.kind === "book")?.id ?? null;
    void recordings.receive(audio, book);
  }
  if (books.length) {
    navigation.closeAdd();
    navigation.view = "library";
    void library.importFiles(books);
  }
}
/** A link pasted outside a text field, or dropped on the window, opens Add with it filled in. */
function offerLink(text: string | undefined) {
  const link = text && /^\s*https?:\/\//i.test(text) ? asLink(text) : null;
  if (link) navigation.openAdd(link);
  return !!link;
}
function pasted(event: ClipboardEvent) {
  const target = event.target as HTMLElement | null;
  if (target?.closest("input, textarea, select, [contenteditable]:not([contenteditable='false'])")) return;
  if (offerLink(event.clipboardData?.getData("text"))) event.preventDefault();
}
window.addEventListener("paste", pasted);
const dragging = ref(false),
  dropError = ref("");
useShortcuts();
// Screens cross-fade. The library keeps its scroll position for the way back; other screens
// open at the top.
let libraryScroll = 0;
watch(
  () => navigation.view,
  (_, from) => {
    if (from === "library") libraryScroll = window.scrollY;
  },
  { flush: "sync" },
);
function placeScroll() {
  window.scrollTo(0, navigation.view === "library" ? libraryScroll : 0);
}
const native = isTauri();
let unlisten: (() => void) | undefined,
  unlistenProblems: (() => void) | undefined,
  disposed = false;
onMounted(async () => {
  void library.start();
  void player.start();
  void preferences.start();
  void recordings.start();
  void signIn.init();
  void listenForProblems(api).then((off) => {
    if (disposed) off();
    else unlistenProblems = off;
  });
  if (native) {
    try {
      const off = await getCurrentWebview().onDragDropEvent((event) => {
        // Only files from outside: a drag that starts inside the window carries no paths.
        const type = event.payload.type;
        dragging.value = type === "enter" ? event.payload.paths.length > 0 : type === "over" && dragging.value;
        if (event.payload.type === "drop") dropped(event.payload.paths);
      });
      if (disposed) off();
      else unlisten = off;
    } catch (e) {
      dropError.value = `File drop unavailable: ${String(e)}`;
    }
  }
});
onUnmounted(() => {
  disposed = true;
  unlisten?.();
  unlistenProblems?.();
  window.removeEventListener("blur", endDrag);
  window.removeEventListener("paste", pasted);
  void Promise.all([library.start(), player.start()]).then(() => {
    library.dispose();
    player.dispose();
    preferences.dispose();
    recordings.dispose();
    useArticlesStore().dispose();
  });
});
// The browser path: only drags that carry files count, and nested enter/leave pairs are
// counted so moving over child elements does not flicker or strand the overlay.
let depth = 0;
const carriesFiles = (event: DragEvent) => Array.from(event.dataTransfer?.types ?? []).includes("Files");
function dragEnter(event: DragEvent) {
  if (!carriesFiles(event)) return;
  event.preventDefault();
  depth += 1;
  dragging.value = true;
}
const carriesLink = (event: DragEvent) => Array.from(event.dataTransfer?.types ?? []).includes("text/uri-list");
function dragOver(event: DragEvent) {
  if (carriesFiles(event) || carriesLink(event)) event.preventDefault();
}
function dragLeave(event: DragEvent) {
  if (!carriesFiles(event)) return;
  depth = Math.max(0, depth - 1);
  if (!depth) dragging.value = false;
}
function endDrag() {
  depth = 0;
  dragging.value = false;
}
window.addEventListener("blur", endDrag);
function browserDrop(event: DragEvent) {
  endDrag();
  if (!carriesFiles(event)) {
    offerLink(event.dataTransfer?.getData("text/uri-list").split("\n")[0]);
    return;
  }
  if (native) return;
  dropped(Array.from(event.dataTransfer?.files ?? []).map((file) => file.name));
}
</script>

<template>
  <div
    class="app"
    :class="{ 'app--reader': navigation.view === 'reader' }"
    @dragstart.prevent
    @dragenter="dragEnter"
    @dragover="dragOver"
    @dragleave="dragLeave"
    @dragend="endDrag"
    @drop.prevent="browserDrop"
  >
    <AppSidebar v-if="navigation.view !== 'reader'" />
    <main class="app__main">
      <p v-if="dropError" class="app__error" role="alert">{{ dropError }}</p>
      <Transition name="app__view" mode="out-in" @enter="placeScroll">
        <HomeView v-if="navigation.view === 'home'" />
        <LibraryView v-else-if="navigation.view === 'library'" />
        <FeedsView v-else-if="navigation.view === 'feeds'" />
        <ActivityView v-else-if="navigation.view === 'activity'" />
        <FindBooksView v-else-if="navigation.view === 'find'" />
        <FindAudiobooksView v-else-if="navigation.view === 'audiobooks'" />
        <ItemView
          v-else-if="navigation.view === 'item' && navigation.bookId"
          :key="navigation.bookId"
          :book-id="navigation.bookId"
        />
        <ReaderView v-else />
      </Transition>
    </main>
    <DropOverlay v-if="dragging" />
    <AddDialog v-if="navigation.addOpen" />
    <SettingsDialog v-if="navigation.settingsOpen" />
    <PairRecordingDialog v-if="recordings.pairing" :key="recordings.pairing.paths.join('|')" />
    <SyncReport v-if="recordings.report" :key="recordings.report" />
    <SignInDialog v-if="signIn.shown && signIn.view" />
    <UiHost />
  </div>
</template>

<style scoped>
.app {
  min-height: 100vh;
  display: flex;
}
.app__main {
  flex: 1;
  min-width: 0;
  padding: 20px 0 40px;
}
.app--reader {
  height: 100vh;
  min-height: 0;
  overflow: hidden;
}
.app--reader .app__main {
  padding: 0;
}
@media (max-width: 719px) {
  .app {
    flex-direction: column;
  }
}
.app__view-enter-active {
  transition:
    opacity var(--dur-2) var(--ease),
    transform var(--dur-2) var(--ease);
}
.app__view-leave-active {
  transition: opacity var(--dur-1) var(--ease);
}
.app__view-enter-from {
  opacity: 0;
  transform: translateY(8px);
}
.app__view-leave-to {
  opacity: 0;
}
.app__error {
  margin: 0 28px 12px;
  padding: 10px 14px;
  border-radius: 12px;
  background: var(--page);
  color: var(--accent);
  font-weight: 600;
}
</style>
