<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { Eraser, HardDrive, Sparkle, Trash2 } from "@lucide/vue";
import { api } from "../../api";
import type { StorageReport } from "../../api/types";
import SettingsSection from "./SettingsSection.vue";
import SettingsField from "./SettingsField.vue";
import SettingsNote from "./SettingsNote.vue";
import SettingsActions from "./SettingsActions.vue";
import { formatBytes } from "../../bytes";
import { useAppSettings } from "../../composables/useAppSettings";
import { useLibraryStore } from "../../stores/library";
import { useUiStore } from "../../stores/ui";

/** What uses disk space, cleanup of audio nobody needs, and the cache limit. */
const library = useLibraryStore(), ui = useUiStore();
const report = ref<StorageReport | null>(null), loadError = ref(""), busy = ref(false);
const STALE_DAYS = 30;
async function load() {
  try { report.value = await api.storageReport(); loadError.value = ""; } catch (e) { loadError.value = String(e); }
}
onMounted(async () => { await library.start(); await load(); });

const parts = computed(() => {
  const r = report.value;
  if (!r) return [];
  return [
    { key: "audio", label: "Generated audio", bytes: r.audioBytes },
    { key: "books", label: "Books", bytes: r.booksBytes },
    { key: "covers", label: "Covers", bytes: r.coversBytes },
    { key: "database", label: "Database", bytes: r.databaseBytes },
  ];
});
const total = computed(() => parts.value.reduce((n, p) => n + p.bytes, 0));
const rows = computed(() => (report.value?.books ?? []).map(entry => {
  const book = library.books.find(b => b.id === entry.bookId);
  return { ...entry, title: book?.title ?? "Unknown book", fraction: book?.progress?.fraction ?? 0, lastOpenedAt: book?.lastOpenedAt ?? book?.addedAt ?? 0 };
}));
const finished = computed(() => rows.value.filter(r => !r.inUse && r.fraction >= 0.99));
const stale = computed(() => {
  const cutoff = Date.now() - STALE_DAYS * 86_400_000;
  return rows.value.filter(r => !r.inUse && r.lastOpenedAt < cutoff);
});
const sum = (list: { bytes: number }[]) => list.reduce((n, r) => n + r.bytes, 0);
const books = (n: number) => `${n} ${n === 1 ? "book" : "books"}`;
const when = (ms: number) => ms ? new Date(ms).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" }) : "never";

async function cleanUnused() {
  busy.value = true;
  try { const freed = await api.cleanUnusedFiles(); ui.toast(`Freed ${formatBytes(freed.bytes)}`); }
  catch (e) { ui.toast(String(e), "error"); }
  finally { busy.value = false; await load(); }
}
async function deleteAudio(list: { bookId: string; title: string; bytes: number }[], title: string) {
  if (!list.length) return;
  const ok = await ui.confirm({ title, body: `Frees ${formatBytes(sum(list))}. Aoide makes the audio again when you play ${list.length === 1 ? "the book" : "these books"}.`, confirmLabel: "Delete audio", danger: true });
  if (!ok) return;
  busy.value = true;
  try { for (const row of list) await api.regenerateBook(row.bookId); ui.toast(`Freed ${formatBytes(sum(list))}`); }
  catch (e) { ui.toast(String(e), "error"); }
  finally { busy.value = false; await Promise.all([load(), library.refresh()]); }
}

const { settings, error, message, save } = useAppSettings();
const gb = ref("");
watch(settings, value => { gb.value = value?.cacheLimitBytes ? String(+(value.cacheLimitBytes / 1024 ** 3).toFixed(2)) : ""; }, { once: true });
function submit() {
  const raw = String(gb.value ?? "").trim(), size = Number(raw);
  void save({ cacheLimitBytes: raw && size > 0 ? Math.round(size * 1024 ** 3) : null });
}
</script>

<template>
  <div class="storage">
    <p v-if="loadError" class="storage__error" role="alert">{{ loadError }}</p>
    <SettingsSection title="Disk use" :icon="HardDrive">
      <div v-if="report" class="storage__usage" aria-label="Disk use" role="group">
        <p class="storage__total"><strong>{{ formatBytes(total) }}</strong> used by Aoide</p>
        <div class="storage__bar" aria-hidden="true"><i v-for="part in parts" :key="part.key" :class="`storage__bar-part storage__bar-part--${part.key}`" :style="{ flexGrow: part.bytes }" /></div>
        <ul class="storage__legend">
          <li v-for="part in parts" :key="part.key"><span :class="`storage__dot storage__dot--${part.key}`" />{{ part.label }} <b>{{ formatBytes(part.bytes) }}</b></li>
        </ul>
      </div>
      <SettingsNote>Books are the EPUB copies in your library folder; the files you added from elsewhere are not counted or changed.</SettingsNote>
    </SettingsSection>

    <SettingsSection v-if="report" title="Clean up" :icon="Sparkle">
      <div class="storage__cards">
        <article class="storage__card" aria-label="Unused files">
          <h3>Unused files</h3>
          <p>Audio left from old voices or settings, and covers of removed books.</p>
          <b>{{ report.unusedFiles ? `${report.unusedFiles} ${report.unusedFiles === 1 ? "file" : "files"} · ${formatBytes(report.unusedBytes)}` : "Nothing to clean up" }}</b>
          <button type="button" class="storage__button" :disabled="busy || !report.unusedFiles" @click="cleanUnused"><Eraser :size="14" /> Clean up</button>
        </article>
        <article class="storage__card" aria-label="Audio of finished books">
          <h3>Finished books</h3>
          <p>Generated audio of books you have listened to the end.</p>
          <b>{{ finished.length ? `${books(finished.length)} · ${formatBytes(sum(finished))}` : "None" }}</b>
          <button type="button" class="storage__button" :disabled="busy || !finished.length" @click="deleteAudio(finished, `Delete the audio of ${books(finished.length)} you finished?`)"><Trash2 :size="14" /> Delete audio</button>
        </article>
        <article class="storage__card" aria-label="Audio of books not opened recently">
          <h3>Not opened in {{ STALE_DAYS }} days</h3>
          <p>Generated audio of books you have not opened for a while.</p>
          <b>{{ stale.length ? `${books(stale.length)} · ${formatBytes(sum(stale))}` : "None" }}</b>
          <button type="button" class="storage__button" :disabled="busy || !stale.length" @click="deleteAudio(stale, `Delete the audio of ${books(stale.length)} not opened in ${STALE_DAYS} days?`)"><Trash2 :size="14" /> Delete audio</button>
        </article>
      </div>
      <SettingsNote>Audio of the book that is playing, being prepared or exported is never deleted here.</SettingsNote>
    </SettingsSection>

    <SettingsSection v-if="report" title="Audio by book">
      <p v-if="!rows.length" class="storage__empty">No generated audio yet.</p>
      <table v-else class="storage__table" aria-label="Audio by book">
        <thead><tr><th scope="col">Book</th><th scope="col">Audio</th><th scope="col">Last opened</th><th scope="col"><span class="visually-hidden">Actions</span></th></tr></thead>
        <tbody>
          <tr v-for="row in rows" :key="row.bookId">
            <th scope="row">{{ row.title }}</th>
            <td>{{ formatBytes(row.bytes) }}</td>
            <td>{{ when(row.lastOpenedAt) }}</td>
            <td><button type="button" class="storage__icon" :aria-label="`Delete audio of ${row.title}`" :title="row.inUse ? 'In use' : 'Delete audio'" :disabled="busy || row.inUse" @click="deleteAudio([row], `Delete the generated audio of ${row.title}?`)"><Trash2 :size="14" /></button></td>
          </tr>
        </tbody>
      </table>
    </SettingsSection>

    <form v-if="settings" class="storage__limit" @submit.prevent="submit">
      <SettingsSection title="Audio cache limit">
        <SettingsField label="Keep at most"><span class="storage__unit"><input v-model="gb" type="number" min="0" step="0.5" aria-label="Audio cache limit in GB" placeholder="No limit" /> GB</span></SettingsField>
        <SettingsNote>Above the limit, Aoide deletes the oldest generated audio first, never the book you are listening to.</SettingsNote>
        <SettingsActions label="Save limit" :message="message" :error="error" />
      </SettingsSection>
    </form>
  </div>
</template>

<style scoped>
.storage { display: grid; gap: 22px; }
.storage__limit { padding-top: 22px; border-top: 1px solid var(--line); }
.storage__error { font-size: 12.5px; font-weight: 600; color: var(--accent); }
.storage__usage { display: grid; gap: 10px; }
.storage__total { font-size: 13px; color: var(--dim); }
.storage__total strong { font-size: 22px; font-weight: 800; color: var(--ink); letter-spacing: -.02em; margin-right: 4px; }
.storage__bar { display: flex; gap: 2px; height: 12px; border-radius: 6px; overflow: hidden; background: var(--line); }
.storage__bar-part { flex-basis: 0; min-width: 3px; }
.storage__bar-part--audio, .storage__dot--audio { background: var(--accent); }
.storage__bar-part--books, .storage__dot--books { background: var(--ink); }
.storage__bar-part--covers, .storage__dot--covers { background: var(--dim); }
.storage__bar-part--database, .storage__dot--database { background: var(--faint); }
.storage__legend { display: flex; flex-wrap: wrap; gap: 6px 18px; padding: 0; margin: 0; list-style: none; font-size: 12.5px; color: var(--dim); }
.storage__legend li { display: inline-flex; align-items: center; gap: 7px; }
.storage__legend b { color: var(--ink); font-variant-numeric: tabular-nums; }
.storage__dot { width: 9px; height: 9px; border-radius: 50%; }
.storage__cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(190px, 1fr)); gap: 10px; }
.storage__card { display: grid; gap: 6px; align-content: start; padding: 14px; border-radius: 14px; background: var(--ground); font-size: 12.5px; }
.storage__card h3 { font-size: 13.5px; font-weight: 800; }
.storage__card p { color: var(--dim); line-height: 1.45; }
.storage__card b { margin-top: 4px; font-variant-numeric: tabular-nums; }
.storage__button { justify-self: start; display: inline-flex; align-items: center; gap: 6px; margin-top: 6px; height: 32px; padding: 0 14px; border-radius: 999px; background: var(--page); font-weight: 700; font-size: 12.5px; box-shadow: inset 0 0 0 1px var(--line); }
.storage__empty { font-size: 13px; color: var(--dim); }
.storage__table { width: 100%; border-collapse: collapse; font-size: 13px; }
.storage__table th, .storage__table td { padding: 8px 10px; text-align: left; border-bottom: 1px solid var(--line); }
.storage__table thead th { font-size: 11px; font-weight: 700; letter-spacing: .06em; text-transform: uppercase; color: var(--dim); }
.storage__table tbody th { font-weight: 700; max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.storage__table td { color: var(--dim); font-variant-numeric: tabular-nums; white-space: nowrap; }
.storage__table td:last-child { text-align: right; }
.storage__icon { display: inline-grid; place-items: center; width: 30px; height: 30px; border-radius: 50%; color: var(--dim); }
.storage__icon:hover:not(:disabled) { background: var(--wash); color: var(--accent); }
.storage__unit { display: flex; align-items: center; gap: 8px; }
.storage__unit input { width: 120px; }
</style>
