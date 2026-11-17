<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import AppSidebar from "@/components/shell/AppSidebar.vue";
import DropOverlay from "@/components/shell/DropOverlay.vue";
import HomeView from "@/views/HomeView.vue";
import LibraryView from "@/views/LibraryView.vue";
import ReaderView from "@/views/ReaderView.vue";
import ActivityView from "@/views/ActivityView.vue";
import FeedsView from "@/views/FeedsView.vue";
import FindBooksView from "@/views/FindBooksView.vue";
import FindAudiobooksView from "@/views/FindAudiobooksView.vue";
import ItemView from "@/views/ItemView.vue";
import UiHost from "@/components/ui/UiHost.vue";
import SignInDialog from "@/components/article/SignInDialog.vue";
import { useSignInStore } from "@/stores/signin";
import SettingsDialog from "@/components/settings/SettingsDialog.vue";
import CloseQuestion from "@/components/shell/CloseQuestion.vue";
import AddDialog from "@/components/library/AddDialog.vue";
import LinkImportDialog from "@/components/library/LinkImportDialog.vue";
import { useLinkImportStore } from "@/stores/linkImport";
import CollectionPicker from "@/components/library/CollectionPicker.vue";
import ShortcutsDialog from "@/components/shell/ShortcutsDialog.vue";
import PairRecordingDialog from "@/components/recording/PairRecordingDialog.vue";
import SyncReport from "@/components/recording/SyncReport.vue";
import { useLibraryStore } from "@/stores/library";
import { usePlayerStore } from "@/stores/player";
import { useNavigationStore } from "@/stores/navigation";
import { usePreferencesStore } from "@/stores/preferences";
import { listenToCore } from "@/opening";
import { api } from "@/api";
import { useRecordingsStore } from "@/stores/recordings";
import { useArticlesStore } from "@/stores/articles";
import { useReaderStore } from "@/stores/reader";
import { isAudioPath } from "@/recording";
import { asLink } from "@/articles";
import { useShortcuts } from "@/composables/useShortcuts";
import { useFrameLog } from "@/composables/useFrameLog";
import { FEED_DRAG, ITEM_DRAG } from "@/drag";

const library = useLibraryStore(),
  player = usePlayerStore(),
  navigation = useNavigationStore(),
  preferences = usePreferencesStore();
const recordings = useRecordingsStore(),
  reader = useReaderStore(),
  signIn = useSignInStore(),
  linkImport = useLinkImportStore();
/** Dropped EPUBs, PDFs and saved emails (as articles) join the library; dropped audio (files or a folder) is paired with the book whose
 *  page or reader is open, else added on its own unless its name comes close to a book's. */
function dropped(paths: string[]) {
  const books = paths.filter((path) => /\.(epub|pdf|eml)$/i.test(path)),
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
const frames = useFrameLog(() => navigation.view);
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
/** The screen in the window, which the layout (sidebar, the reader's full height) follows. It
 *  changes when the next screen mounts, once the last has faded out: changing the layout while
 *  the old screen fades would make it jump. */
const shown = ref(navigation.view);
const settle = () => {
  shown.value = navigation.view;
};
const native = isTauri();
let unlisten: (() => void) | undefined,
  unlistenCore: (() => void) | undefined,
  disposed = false;
onMounted(async () => {
  void library.start();
  void player.start();
  void preferences.start();
  void recordings.start();
  void linkImport.start();
  void signIn.init();
  void listenToCore(api).then((off) => {
    if (disposed) off();
    else unlistenCore = off;
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
  unlistenCore?.();
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
/** Nothing in the window is dragged out of it (covers, links), but for what carries one of
 *  Aoide's own drag types: library cards to the sidebar's collections, feeds to their folders. */
const OWN_DRAGS = [ITEM_DRAG, FEED_DRAG];
function dragStart(event: DragEvent) {
  const types = Array.from(event.dataTransfer?.types ?? []);
  if (!OWN_DRAGS.some((type) => types.includes(type))) event.preventDefault();
}
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
    :class="{ 'app--reader': shown === 'reader' }"
    @dragstart="dragStart"
    @dragenter="dragEnter"
    @dragover="dragOver"
    @dragleave="dragLeave"
    @dragend="endDrag"
    @drop.prevent="browserDrop"
  >
    <AppSidebar v-if="shown !== 'reader'" />
    <main class="app__main">
      <p v-if="dropError" class="app__error" role="alert">{{ dropError }}</p>
      <Transition name="app__view" mode="out-in" @enter="placeScroll" @after-enter="frames.shown">
        <HomeView v-if="navigation.view === 'home'" @vue:before-mount="settle" />
        <LibraryView v-else-if="navigation.view === 'library'" @vue:before-mount="settle" />
        <FeedsView v-else-if="navigation.view === 'feeds'" @vue:before-mount="settle" />
        <ActivityView v-else-if="navigation.view === 'activity'" @vue:before-mount="settle" />
        <FindBooksView v-else-if="navigation.view === 'find'" @vue:before-mount="settle" />
        <FindAudiobooksView v-else-if="navigation.view === 'audiobooks'" @vue:before-mount="settle" />
        <ItemView
          v-else-if="navigation.view === 'item' && navigation.bookId"
          :key="navigation.bookId"
          :book-id="navigation.bookId"
          @vue:before-mount="settle"
        />
        <ReaderView v-else @vue:before-mount="settle" />
      </Transition>
    </main>
    <DropOverlay v-if="dragging" />
    <AddDialog v-if="navigation.addOpen" />
    <LinkImportDialog v-if="linkImport.view.stage === 'choosing'" />
    <CollectionPicker
      v-if="navigation.collectionsFor"
      :key="navigation.collectionsFor"
      :book-id="navigation.collectionsFor"
      @close="navigation.closeCollections()"
    />
    <SettingsDialog v-if="navigation.settingsOpen" />
    <ShortcutsDialog v-if="navigation.shortcutsOpen" />
    <PairRecordingDialog v-if="recordings.pairing" :key="recordings.pairing.paths.join('|')" />
    <SyncReport v-if="recordings.report" :key="recordings.report" />
    <SignInDialog v-if="signIn.shown && signIn.view" />
    <CloseQuestion />
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
  padding: var(--space-10) 0 var(--space-20);
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
  margin: 0 var(--space-14) var(--space-6);
  padding: var(--space-5) var(--space-7);
  border-radius: var(--radius-md);
  background: var(--page);
  color: var(--accent);
  font-weight: 600;
}
</style>
