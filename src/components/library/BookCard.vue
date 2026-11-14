<script setup lang="ts">
import { computed } from "vue";
import { Download, Ellipsis, Eraser, FolderOpen, FolderPlus, Headphones, Info, Trash2 } from "@lucide/vue";
import type { SyncStage } from "../../api/types";
import { useRecordingsStore } from "../../stores/recordings";
import { useCollections } from "../../composables/useCollections";
import { useLibraryStore } from "../../stores/library";
import { api } from "../../api";
import type { BookSummary } from "../../api/types";
import CoverArt from "../CoverArt.vue";
import UiMenu from "../ui/UiMenu.vue";
import type { MenuEntry } from "../ui/types";
import { useUiStore } from "../../stores/ui";
import { formatBytes } from "../../bytes";
import { ACTION_ICONS, ACTION_LABELS, describePreparation, preparationStatus, runPrepareAction } from "../../preparation";

const props = defineProps<{ book: BookSummary; busy?: boolean }>();
const emit = defineEmits<{ open: []; details: []; remove: [] }>();
const unavailable = computed(() => props.book.status.kind !== "present");
const percent = computed(() => Math.round((props.book.progress?.fraction ?? 0) * 100));
/** A recording's stage in a few words, for the card. */
const RECORDING: Record<SyncStage, string> = {
  checking: "checking recording", waiting: "recording waiting to sync", needsModel: "recording needs a model", transcribing: "syncing recording",
  matching: "matching recording", ready: "narrated recording", sampleMismatch: "recording needs review", poorMatch: "recording needs review",
  paused: "recording paused", missing: "recording missing", failed: "recording sync failed",
};
const status = computed(() => {
  const { prepare, audioBytes, progress, recording } = props.book;
  // Listening progress only once started, so it is not read as the sync's.
  if (recording) return progress?.fraction ? `${percent.value}% · ${RECORDING[recording]}` : RECORDING[recording].replace(/^./, c => c.toUpperCase());
  if (prepare?.state === "done") return `${percent.value}% · ready offline`;
  if (prepare) return describePreparation(preparationStatus(prepare));
  if (audioBytes) return `${percent.value}% · ${formatBytes(audioBytes)}`;
  return progress?.fraction ? `${percent.value}%` : "Not started";
});
const ui = useUiStore(), library = useLibraryStore(), shelves = useCollections();
/** Runs a card action; failures become an error toast. */
async function act(action: () => Promise<unknown>) {
  try { await action(); } catch (e) { ui.toast(String(e), "error"); }
}
const preparation = computed(() => preparationStatus(props.book.prepare));
async function exportBook() {
  ui.toast(`Exporting ${props.book.title}… this can take a while`);
  const path = await api.exportBook(props.book.id);
  ui.toast(`Exported to ${path}`);
}
const recordings = useRecordingsStore();
async function addAudiobook() {
  const paths = await api.chooseAudio(false);
  if (paths?.length) recordings.openPairing(paths, props.book.id);
}
async function deleteAudio() {
  const ok = await ui.confirm({ title: `Delete the generated audio of ${props.book.title}?`, body: `Frees ${formatBytes(props.book.audioBytes ?? 0)}. Aoide makes the audio again when you play the book.`, confirmLabel: "Delete audio", danger: true });
  if (ok) { await api.regenerateBook(props.book.id); ui.toast("Generated audio deleted"); }
}
async function remove() {
  const ok = await ui.confirm({ title: `Remove ${props.book.title} from the library?`, body: "The EPUB file stays where it is. Aoide forgets the book and deletes its generated audio and bookmarks.", confirmLabel: "Remove", danger: true });
  if (ok) emit("remove");
}
const items = computed<MenuEntry[]>(() => {
  const book = props.book, list: MenuEntry[] = [];
  if (!unavailable.value) {
    list.push({ id: "details", label: "Details", icon: Info, action: () => emit("details") });
    list.push({ id: "folder", label: "Show in folder", icon: FolderOpen, action: () => act(() => api.showInFolder(book.id)) });
    for (const action of preparation.value.actions) list.push({ id: action, label: ACTION_LABELS[action], icon: ACTION_ICONS[action], action: () => runPrepareAction(book.id, action) });
    list.push({ id: "audiobook", label: book.recording ? "Replace audiobook…" : "Add audiobook…", icon: Headphones, action: () => act(addAudiobook) });
    if (book.recording !== "ready") list.push({ id: "export", label: "Export as M4B", icon: Download, action: () => act(exportBook) });
    if (book.audioBytes) list.push({ id: "audio", label: "Delete generated audio", icon: Eraser, action: () => act(deleteAudio) });
    list.push({ kind: "separator" }, { kind: "heading", label: "Collections" });
    for (const c of library.collections) list.push({ id: `collection:${c.id}`, label: c.name, ariaLabel: c.name, checked: book.collections.includes(c.id), action: () => act(() => shelves.toggle(book, c.id)) });
    list.push({ id: "new-collection", label: "New collection…", icon: FolderPlus, action: () => act(() => shelves.create(book)) }, { kind: "separator" });
  }
  list.push({ id: "remove", label: "Remove from library", ariaLabel: `Remove ${book.title} from the library`, icon: Trash2, danger: true, action: remove });
  return list;
});
</script>

<template>
  <li class="book-card" :class="{ 'book-card--unavailable': unavailable }">
    <button type="button" class="book-card__open" :aria-label="`Open ${book.title}`" :disabled="unavailable || busy" @click="emit('open')">
      <CoverArt class="book-card__cover" :title="book.title" :url="book.coverUrl" />
    </button>
    <h3 class="book-card__title" :title="book.title">{{ book.title }}</h3>
    <span class="book-card__line">{{ book.authors.join(", ") || "Unknown author" }}</span>
    <span class="book-card__track"><i :style="{ width: `${percent}%` }" /></span>
    <span v-if="book.status.kind === 'missing'" class="book-card__badge book-card__badge--warn">File missing</span>
    <template v-else-if="book.status.kind === 'error'"><span class="book-card__badge book-card__badge--error">Can't open this EPUB</span><span class="book-card__reason" :title="book.status.message">{{ book.status.message }}</span></template>
    <span v-else class="book-card__line"><Headphones v-if="book.recording" class="book-card__mark" :size="12" aria-hidden="true" />{{ status }}</span>
    <UiMenu v-slot="{ toggle, trigger }" class="book-card__more" :label="`Actions for ${book.title}`" :items="items" align="end">
      <button type="button" class="book-card__more-button" :aria-label="`More for ${book.title}`" v-bind="trigger" @click="toggle"><Ellipsis :size="16" /></button>
    </UiMenu>
  </li>
</template>

<style scoped>
.book-card { position: relative; display: grid; gap: 6px; align-content: start; min-width: 0; font-size: 12.5px; }
.book-card__open { display: block; border-radius: 12px; }
.book-card__open:disabled { opacity: 1; cursor: default; }
.book-card__cover { font-size: 13px; border-radius: 12px; transition: box-shadow var(--dur-2) var(--ease); }
.book-card__open:not(:disabled):hover .book-card__cover { box-shadow: 0 22px 44px -16px var(--shadow), 0 0 0 2px var(--line); }
.book-card--unavailable .book-card__cover { filter: grayscale(1); opacity: .35; }
.book-card__title { margin-top: 4px; font-size: 14px; font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.book-card__line, .book-card__reason { color: var(--dim); font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.book-card__reason { font-size: 11.5px; }
.book-card__mark { vertical-align: -1px; margin-right: 4px; color: var(--accent); }
.book-card__track { position: relative; height: 4px; border-radius: 2px; background: var(--line); overflow: hidden; }
.book-card__track i { position: absolute; inset: 0 auto 0 0; background: var(--accent); }
.book-card__badge { font-size: 12px; font-weight: 700; }
.book-card__badge--warn { color: var(--warn); }
.book-card__badge--error { color: var(--accent); }
.book-card__more { position: absolute; top: 8px; right: 8px; }
.book-card__more-button { display: grid; place-items: center; width: 30px; height: 30px; border-radius: 50%; background: var(--glass); opacity: 0; transition: opacity .15s; }
.book-card:hover .book-card__more-button, .book-card:focus-within .book-card__more-button, .book-card__more-button[aria-expanded="true"] { opacity: 1; }
</style>
