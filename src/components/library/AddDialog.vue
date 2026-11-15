<script setup lang="ts">
import { FilePlus, FolderOpen, FolderSearch, Headphones, RefreshCw, X } from "@lucide/vue";
import UiDialog from "../ui/UiDialog.vue";
import { api } from "../../api";
import { useLibraryStore } from "../../stores/library";
import { useNavigationStore } from "../../stores/navigation";
import { useRecordingsStore } from "../../stores/recordings";

/** Everything that adds to the library, in one place: opened by the sidebar's Add and Ctrl N.
 *  Files can also be dropped anywhere on the window (App routes them). */
const library = useLibraryStore(),
  navigation = useNavigationStore(),
  recordings = useRecordingsStore();
const close = () => navigation.closeAdd();
/** EPUBs join the library; Library opens to show them (and anything that could not be read). */
async function chooseBooks() {
  const paths = await api.chooseBooks().catch((e) => {
    library.error = String(e);
    return null;
  });
  if (!paths?.length) return;
  close();
  navigation.view = "library";
  await library.importFiles(paths);
}
/** An audiobook (files or a folder) is paired with a book or added on its own, as when dropped. */
async function chooseAudio(folder: boolean) {
  const paths = await api.chooseAudio(folder).catch((e) => {
    library.error = String(e);
    return null;
  });
  if (!paths?.length) return;
  close();
  const open = navigation.view === "item" ? navigation.bookId : null;
  await recordings.receive(paths, library.books.find((b) => b.id === open && b.kind !== "audiobook")?.id ?? null);
}
function go(view: "find" | "audiobooks") {
  close();
  navigation.view = view;
}
function rescan() {
  close();
  navigation.view = "library";
  void library.rescan();
}
</script>

<template>
  <UiDialog label="Add to your library" @close="close">
    <div class="add-dialog">
      <div class="add-dialog__head">
        <h1 class="add-dialog__title">Add to your library</h1>
        <button type="button" class="add-dialog__close" aria-label="Close" @click="close">
          <X :size="18" aria-hidden="true" />
        </button>
      </div>
      <section class="add-dialog__section" aria-labelledby="add-files">
        <h2 id="add-files" class="add-dialog__label">Files on this computer</h2>
        <div class="add-dialog__drop">
          <b class="add-dialog__drop-title">Drop books or audiobooks anywhere on this window</b>
          <p class="add-dialog__hint">
            EPUB for text. M4B, MP3, FLAC or Ogg for audio: an audiobook is paired with its book, or added on its own.
          </p>
          <div class="add-dialog__buttons">
            <button type="button" class="add-dialog__button add-dialog__button--main" @click="chooseBooks">
              <FilePlus :size="16" aria-hidden="true" /> Choose books…
            </button>
            <button type="button" class="add-dialog__button" @click="chooseAudio(false)">
              <Headphones :size="16" aria-hidden="true" /> Choose an audiobook…
            </button>
            <button type="button" class="add-dialog__button" @click="chooseAudio(true)">
              <FolderOpen :size="16" aria-hidden="true" /> Choose an audiobook folder…
            </button>
          </div>
        </div>
      </section>
      <section class="add-dialog__section" aria-labelledby="add-find">
        <h2 id="add-find" class="add-dialog__label">Find on this computer</h2>
        <div class="add-dialog__buttons add-dialog__buttons--start">
          <button type="button" class="add-dialog__button" @click="go('find')">
            <FolderSearch :size="16" aria-hidden="true" /> Scan for books…
          </button>
          <button type="button" class="add-dialog__button" @click="go('audiobooks')">
            <Headphones :size="16" aria-hidden="true" /> Find audiobooks…
          </button>
          <button type="button" class="add-dialog__button" :disabled="library.loading" @click="rescan">
            <RefreshCw :size="16" aria-hidden="true" /> Rescan library folder
          </button>
        </div>
      </section>
    </div>
  </UiDialog>
</template>

<style scoped>
.add-dialog {
  width: 560px;
  max-width: 100%;
  display: flex;
  flex-direction: column;
  gap: 22px;
  padding: 28px;
  overflow-y: auto;
}
.add-dialog__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.add-dialog__title {
  margin: 0;
  font-size: 22px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.add-dialog__close {
  display: grid;
  place-items: center;
  width: 44px;
  height: 44px;
  border-radius: 50%;
  background: var(--ground);
}
.add-dialog__section {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.add-dialog__label {
  margin: 0;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--dim);
}
.add-dialog__drop {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 28px 20px;
  border: 1.5px dashed var(--faint);
  border-radius: 18px;
  background: var(--ground);
  text-align: center;
}
.add-dialog__drop-title {
  font-size: 16px;
}
.add-dialog__hint {
  margin: 0;
  max-width: 380px;
  font-size: 13px;
  line-height: 1.45;
  color: var(--dim);
}
.add-dialog__buttons {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 8px;
}
.add-dialog__buttons--start {
  justify-content: flex-start;
}
.add-dialog__button {
  height: 44px;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 0 18px;
  border-radius: 22px;
  background: var(--wash);
  font-weight: 600;
}
.add-dialog__button--main {
  background: var(--ink);
  color: var(--ground);
}
</style>
