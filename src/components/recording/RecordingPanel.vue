<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { FolderOpen, Headphones, Plus } from "@lucide/vue";
import { api } from "../../api";
import type { ModelDownloadView } from "../../api/types";
import { useRecordingsStore } from "../../stores/recordings";
import { useLibraryStore } from "../../stores/library";
import { useUiStore } from "../../stores/ui";
import { usePlayerStore } from "../../stores/player";
import { formatBytes } from "../../bytes";
import { RECORDING_ACTION_ICONS, RECORDING_ACTION_LABELS, describeFiles, recordingStatus, type RecordingAction } from "../../recording";

/** A book's recording: what it is, how its sync is going and what can be done about it; or a
 *  way to add one. Used on the book page and in book tools. */
const props = defineProps<{ bookId: string }>();
const recordings = useRecordingsStore(), library = useLibraryStore(), ui = useUiStore();
const recording = computed(() => recordings.byBook[props.bookId] ?? null);
const status = computed(() => recording.value ? recordingStatus(recording.value) : null);
const error = ref(""), download = ref<ModelDownloadView | null>(null);
let unlisten: (() => void) | undefined, disposed = false;
onMounted(async () => {
  await recordings.start();
  try { await recordings.load(props.bookId); } catch (e) { error.value = String(e); }
  const off = await api.onModelDownload(d => { if (d.id === recording.value?.modelId) download.value = d; });
  if (disposed) off(); else unlisten = off;
});
onUnmounted(() => { disposed = true; unlisten?.(); });

const title = computed(() => library.books.find(b => b.id === props.bookId)?.title ?? "this book");
async function add(folder: boolean) {
  error.value = "";
  try {
    const paths = await api.chooseAudio(folder);
    if (paths?.length) recordings.openPairing(paths, props.bookId);
  } catch (e) { error.value = String(e); }
}
async function act(action: RecordingAction) {
  error.value = "";
  const id = props.bookId;
  try {
    if (action === "report") recordings.openReport(id);
    else if (action === "listen") { await recordings.setNarration(id, "recording"); await usePlayerStore().play(id, null, null); }
    else if (action === "download") await downloadModel();
    else if (action === "export") await exportReadAlong();
    else if (action === "remove") {
      const ok = await ui.confirm({ title: `Remove the recording from ${title.value}?`, body: "Aoide forgets the recording and its sync. The audio files stay where they are.", confirmLabel: "Remove", danger: true });
      if (ok) await recordings.remove(id);
    } else if (action === "resync") {
      const ok = await ui.confirm({ title: "Sync the recording again?", body: "Aoide listens to the whole recording again. It plays with the speech engine until the new sync is done.", confirmLabel: "Sync again" });
      if (ok) await recordings.act(id, "resync");
    } else await recordings.act(id, action);
  } catch (e) { error.value = String(e); }
}
async function exportReadAlong() {
  ui.toast(`Exporting ${title.value} with its recording… this can take a minute`);
  const path = await api.exportReadAlong(props.bookId);
  ui.toast(`Exported to ${path}`);
}
async function downloadModel() {
  const id = recording.value?.modelId;
  if (!id) return;
  download.value = { id, stage: "downloading", doneBytes: 0, totalBytes: 0, error: null };
  try { await api.downloadModel(id); await recordings.load(props.bookId); }
  catch (e) { download.value = { id, stage: "failed", doneBytes: 0, totalBytes: 0, error: String(e) }; }
}
const downloading = computed(() => download.value?.stage === "downloading");
</script>

<template>
  <section class="recording-panel" aria-label="Recording">
    <template v-if="recording && status">
      <div class="recording-panel__head">
        <Headphones class="recording-panel__icon" :size="18" aria-hidden="true" />
        <div class="recording-panel__text">
          <p class="recording-panel__label" :class="`recording-panel__label--${status.stage}`">{{ status.label }}</p>
          <p class="recording-panel__files">{{ describeFiles(recording) }}</p>
        </div>
      </div>
      <div v-if="status.percent !== null || status.working" class="recording-panel__track" :class="{ 'recording-panel__track--busy': status.percent === null }" role="progressbar" :aria-valuenow="status.percent ?? undefined" aria-valuemin="0" aria-valuemax="100" :aria-label="`${status.label} progress`"><i :style="{ width: `${status.percent ?? 30}%` }" /></div>
      <p v-if="status.detail" class="recording-panel__detail">{{ status.detail }}</p>
      <template v-if="download && status.actions.includes('download')">
        <progress v-if="downloading" :value="download.doneBytes" :max="Math.max(1, download.totalBytes)" aria-label="Model download progress" />
        <p v-if="downloading" class="recording-panel__detail">Downloading {{ download.id }} · {{ formatBytes(download.doneBytes) }}<template v-if="download.totalBytes"> of {{ formatBytes(download.totalBytes) }}</template></p>
        <p v-else-if="download.error" class="recording-panel__error" role="alert">{{ download.error }}</p>
      </template>
      <div v-if="status.actions.length" class="recording-panel__actions">
        <button v-for="action in status.actions" :key="action" type="button" class="recording-panel__button" :class="{ 'recording-panel__button--danger': action === 'remove' }" :disabled="action === 'download' && downloading" :aria-label="RECORDING_ACTION_LABELS[action]" @click="act(action)"><component :is="RECORDING_ACTION_ICONS[action]" :size="14" /> {{ RECORDING_ACTION_LABELS[action] }}</button>
      </div>
    </template>
    <template v-else>
      <div class="recording-panel__head">
        <Headphones class="recording-panel__icon" :size="18" aria-hidden="true" />
        <div class="recording-panel__text">
          <p class="recording-panel__label">Listen to a real narrator</p>
          <p class="recording-panel__files">Pair a DRM-free audiobook of this book (an M4B, or MP3, FLAC or Ogg files) and it plays with the same read-along highlighting. Nothing leaves your computer.</p>
        </div>
      </div>
      <div class="recording-panel__actions">
        <button type="button" class="recording-panel__button recording-panel__button--primary" aria-label="Add audiobook files" @click="add(false)"><Plus :size="14" /> Add audiobook…</button>
        <button type="button" class="recording-panel__button" aria-label="Add an audiobook folder" @click="add(true)"><FolderOpen :size="14" /> Choose a folder…</button>
      </div>
    </template>
    <p v-if="error" class="recording-panel__error" role="alert">{{ error }}</p>
  </section>
</template>

<style scoped>
.recording-panel { display: grid; gap: 10px; }
.recording-panel__head { display: flex; align-items: flex-start; gap: 12px; }
.recording-panel__icon { flex-shrink: 0; margin-top: 2px; color: var(--accent); }
.recording-panel__text { display: grid; gap: 2px; min-width: 0; }
.recording-panel__label { font-size: 14px; font-weight: 700; }
.recording-panel__label--sampleMismatch, .recording-panel__label--poorMatch, .recording-panel__label--missing, .recording-panel__label--failed, .recording-panel__label--needsModel { color: var(--warn); }
.recording-panel__files { font-size: 12px; color: var(--dim); line-height: 1.45; }
.recording-panel__detail { font-size: 12px; color: var(--dim); font-variant-numeric: tabular-nums; line-height: 1.45; }
.recording-panel__error { font-size: 12px; font-weight: 600; color: var(--accent); overflow-wrap: anywhere; }
.recording-panel__track { position: relative; height: 5px; border-radius: 999px; background: var(--line); overflow: hidden; }
.recording-panel__track i { position: absolute; inset: 0 auto 0 0; border-radius: inherit; background: var(--accent); transition: width var(--dur-2) var(--ease); }
.recording-panel__track--busy i { animation: recording-panel-busy 1.4s var(--ease) infinite; }
@keyframes recording-panel-busy { from { transform: translateX(-100%); } to { transform: translateX(340%); } }
@media (prefers-reduced-motion: reduce) { .recording-panel__track--busy i { animation: none; } }
.recording-panel progress { width: 100%; height: 5px; }
.recording-panel__actions { display: flex; flex-wrap: wrap; gap: 8px; }
.recording-panel__button { display: inline-flex; align-items: center; gap: 6px; border-radius: 999px; background: var(--ground); font-size: 12.5px; font-weight: 700; padding: 8px 14px; white-space: nowrap; }
.recording-panel__button--primary { background: var(--accent); color: var(--on-accent); }
.recording-panel__button--danger { color: var(--accent); }
</style>
