<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { Check, FileText, Play, Trash2, X } from "@lucide/vue";
import { api } from "../../api";
import type { SyncReportView } from "../../api/types";
import UiDialog from "../ui/UiDialog.vue";
import { useRecordingsStore } from "../../stores/recordings";
import { useLibraryStore } from "../../stores/library";
import { useUiStore } from "../../stores/ui";

/** How well a book's recording matched its text: the share found, chapter by chapter, and the
 *  narration that is not in the text, each stretch playable. */
const recordings = useRecordingsStore(), library = useLibraryStore(), ui = useUiStore();
const bookId = recordings.report!;
const report = ref<SyncReportView | null>(null), error = ref("");
const recording = computed(() => recordings.byBook[bookId] ?? null);
const title = computed(() => library.books.find(b => b.id === bookId)?.title ?? "");
onMounted(async () => { try { report.value = await api.syncReport(bookId); } catch (e) { error.value = String(e); } });

const share = (part: number, whole: number) => whole ? Math.floor((part / whole) * 100) : 0;
const clock = (ms: number) => {
  const s = Math.floor(ms / 1000), h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60);
  return `${h}:${String(m).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
};
const skipped = computed(() => report.value?.chapters.filter(c => c.words > 0 && c.timedWords === 0) ?? []);
const narrated = computed(() => report.value?.chapters.filter(c => c.timedWords > 0) ?? []);
async function play(atMs: number) {
  try { await api.playRecordingAt(bookId, atMs); } catch (e) { ui.toast(String(e), "error"); }
}
async function accept() {
  try { await recordings.act(bookId, "accept"); recordings.closeReport(); } catch (e) { error.value = String(e); }
}
async function remove() {
  const ok = await ui.confirm({ title: `Remove the recording from ${title.value}?`, body: "The audio files stay where they are.", confirmLabel: "Remove", danger: true });
  if (ok) { await recordings.remove(bookId); recordings.closeReport(); }
}
</script>

<template>
  <UiDialog label="Sync report" @close="recordings.closeReport()">
    <div class="sync-report">
      <header class="sync-report__head">
        <FileText :size="18" aria-hidden="true" />
        <h2 class="sync-report__title">Sync report<span v-if="title"> · {{ title }}</span></h2>
        <button type="button" class="sync-report__close" aria-label="Close" @click="recordings.closeReport()"><X :size="16" /></button>
      </header>
      <p v-if="error" class="sync-report__error" role="alert">{{ error }}</p>
      <template v-if="report">
        <p class="sync-report__summary"><strong>{{ share(report.summary.matchedWords, report.summary.words) }}%</strong> of the text was heard in the recording, and {{ share(report.summary.timedWords, report.summary.words) }}% has a place in it ({{ report.summary.words.toLocaleString() }} words).</p>
        <section class="sync-report__section">
          <h3 class="sync-report__heading">Chapters</h3>
          <table class="sync-report__table">
            <thead><tr><th scope="col">Chapter</th><th scope="col">Found</th><th scope="col">In the recording</th></tr></thead>
            <tbody>
              <tr v-for="chapter in narrated" :key="chapter.index" :class="{ 'sync-report__low': share(chapter.matchedWords, chapter.words) < 75 }">
                <td class="sync-report__name">{{ chapter.title || `Chapter ${chapter.index + 1}` }}</td>
                <td class="sync-report__number">{{ share(chapter.matchedWords, chapter.words) }}%</td>
                <td class="sync-report__number"><button v-if="chapter.startMs !== null" type="button" class="sync-report__time" :aria-label="`Play ${chapter.title} from ${clock(chapter.startMs)}`" @click="play(chapter.startMs)"><Play :size="11" /> {{ clock(chapter.startMs) }}</button></td>
              </tr>
            </tbody>
          </table>
        </section>
        <section v-if="skipped.length" class="sync-report__section">
          <h3 class="sync-report__heading">Not in the recording</h3>
          <p class="sync-report__hint">The narrator did not read these; playback goes past them.</p>
          <ul class="sync-report__list"><li v-for="chapter in skipped" :key="chapter.index">{{ chapter.title || `Chapter ${chapter.index + 1}` }} <span class="sync-report__dim">({{ chapter.words.toLocaleString() }} words)</span></li></ul>
        </section>
        <section v-if="report.offText.length" class="sync-report__section">
          <h3 class="sync-report__heading">In the recording but not in the text</h3>
          <p class="sync-report__hint">Credits, a dedication left out of the EPUB, or words Aoide misheard. They play before the next sentence.</p>
          <ul class="sync-report__list">
            <li v-for="gap in report.offText" :key="gap.startMs" class="sync-report__gap">
              <button type="button" class="sync-report__time" :aria-label="`Play the stretch at ${clock(gap.startMs)}`" @click="play(gap.startMs)"><Play :size="11" /> {{ clock(gap.startMs) }}</button>
              <span class="sync-report__dim">{{ Math.round((gap.endMs - gap.startMs) / 1000) }} s</span>
              <span class="sync-report__excerpt">“{{ gap.excerpt }}…”</span>
            </li>
          </ul>
        </section>
        <footer v-if="recording?.stage === 'poorMatch'" class="sync-report__actions">
          <p class="sync-report__hint">Little of the text was found. The recording may be abridged, or of another edition or translation.</p>
          <button type="button" class="sync-report__button" @click="remove"><Trash2 :size="14" /> Remove recording</button>
          <button type="button" class="sync-report__button sync-report__button--primary" @click="accept"><Check :size="14" /> Use it anyway</button>
        </footer>
      </template>
    </div>
  </UiDialog>
</template>

<style scoped>
.sync-report { width: min(640px, 94vw); max-height: 84vh; overflow-y: auto; display: grid; gap: 16px; padding: 20px 22px; }
.sync-report__head { display: flex; align-items: center; gap: 10px; color: var(--accent); }
.sync-report__title { flex: 1; min-width: 0; font-size: 16px; color: var(--ink); margin: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.sync-report__title span { color: var(--dim); font-weight: 600; }
.sync-report__close { display: grid; place-items: center; width: 32px; height: 32px; border-radius: 50%; background: var(--glass); color: var(--ink); }
.sync-report__summary { font-size: 13.5px; line-height: 1.5; }
.sync-report__section { display: grid; gap: 8px; padding-top: 12px; border-top: 1px solid var(--line); }
.sync-report__heading { font-size: 11px; font-weight: 700; letter-spacing: .06em; text-transform: uppercase; color: var(--faint); margin: 0; }
.sync-report__hint { font-size: 12px; color: var(--dim); line-height: 1.45; }
.sync-report__table { width: 100%; border-collapse: collapse; font-size: 12.5px; }
.sync-report__table th { text-align: left; font-weight: 600; color: var(--dim); padding: 4px 6px; }
.sync-report__table td { padding: 4px 6px; border-top: 1px solid var(--line); }
.sync-report__name { max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.sync-report__number { font-variant-numeric: tabular-nums; white-space: nowrap; }
.sync-report__low td { color: var(--warn); }
.sync-report__list { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; font-size: 12.5px; }
.sync-report__gap { display: grid; grid-template-columns: auto auto minmax(0, 1fr); align-items: baseline; gap: 8px; }
.sync-report__excerpt { color: var(--dim); font-style: italic; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.sync-report__dim { color: var(--dim); }
.sync-report__time { display: inline-flex; align-items: center; gap: 4px; border-radius: 999px; padding: 2px 8px; background: var(--ground); font-size: 12px; font-weight: 700; font-variant-numeric: tabular-nums; }
.sync-report__error { font-size: 12px; font-weight: 600; color: var(--accent); }
.sync-report__actions { display: flex; flex-wrap: wrap; justify-content: flex-end; align-items: center; gap: 8px; padding-top: 12px; border-top: 1px solid var(--line); }
.sync-report__actions .sync-report__hint { flex-basis: 100%; }
.sync-report__button { display: inline-flex; align-items: center; gap: 6px; border-radius: 999px; padding: 9px 16px; font-size: 13px; font-weight: 700; background: var(--ground); }
.sync-report__button--primary { background: var(--accent); color: var(--on-accent); }
</style>
