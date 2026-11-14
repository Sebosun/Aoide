<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { FileAudio, Headphones, ShieldAlert, X } from "@lucide/vue";
import { api } from "../../api";
import type { InspectionView } from "../../api/types";
import UiDialog from "../ui/UiDialog.vue";
import UiSelect from "../ui/UiSelect.vue";
import { useRecordingsStore } from "../../stores/recordings";
import { useLibraryStore } from "../../stores/library";
import { useNavigationStore } from "../../stores/navigation";
import { formatDuration } from "../../estimate";
import { describeFiles } from "../../recording";

/** Pairing dropped or chosen audio with a book: what the files are, which book they read, and
 *  any that cannot be used. "Check and sync" attaches them; the sync runs in the background. */
const recordings = useRecordingsStore(), library = useLibraryStore(), navigation = useNavigationStore();
const pairing = recordings.pairing!;
const found = ref<InspectionView | null>(null), error = ref(""), busy = ref(false);
const bookId = ref<string>(pairing.bookId ?? "");
onMounted(async () => {
  void library.start();
  try {
    found.value = await api.inspectRecording(pairing.paths);
    if (!bookId.value && found.value.suggestedBook) bookId.value = found.value.suggestedBook;
  } catch (e) { error.value = String(e); }
});
const books = computed(() => library.books.filter(b => b.status.kind === "present").map(b => ({ value: b.id, label: b.title, hint: b.authors.join(", ") || undefined })));
const shown = computed(() => found.value?.files.slice(0, 6) ?? []);
const usable = computed(() => Boolean(found.value?.files.length) && !found.value?.problems.length);
const replacing = computed(() => Boolean(bookId.value && recordings.byBook[bookId.value]));
/** Recognition runs at roughly 25× real time on an ordinary CPU. */
const estimate = computed(() => found.value ? formatDuration(found.value.totalMs / 25) : "");
async function pair() {
  if (!bookId.value || !usable.value) return;
  busy.value = true; error.value = "";
  try {
    await recordings.attach(bookId.value, pairing.paths);
    const id = bookId.value;
    recordings.closePairing();
    if (navigation.view !== "reader") navigation.openBook(id);
  } catch (e) { error.value = String(e); } finally { busy.value = false; }
}
</script>

<template>
  <UiDialog label="Pair an audiobook" @close="recordings.closePairing()">
    <div class="pair">
      <header class="pair__head">
        <Headphones :size="18" aria-hidden="true" />
        <h2 class="pair__title">Pair an audiobook</h2>
        <button type="button" class="pair__close" aria-label="Close" @click="recordings.closePairing()"><X :size="16" /></button>
      </header>
      <p v-if="!found && !error" class="pair__hint" role="status">Looking at the files…</p>
      <template v-if="found">
        <p class="pair__summary">{{ found.files.length ? describeFiles(found) : "No audio Aoide can play was found." }}<template v-if="found.title"> · tagged “{{ found.title }}”<template v-if="found.author"> by {{ found.author }}</template></template></p>
        <ul v-if="found.files.length" class="pair__files" aria-label="Audio files in order">
          <li v-for="file in shown" :key="file.path" class="pair__file"><FileAudio :size="14" aria-hidden="true" /><span class="pair__name" :title="file.path">{{ file.name }}</span><span class="pair__length">{{ formatDuration(file.durationMs) }}</span></li>
          <li v-if="found.files.length > shown.length" class="pair__more">and {{ found.files.length - shown.length }} more</li>
        </ul>
        <ul v-if="found.problems.length" class="pair__problems" role="alert" aria-label="Files that cannot be used">
          <li v-for="problem in found.problems" :key="problem.path" class="pair__problem"><ShieldAlert :size="14" aria-hidden="true" /><span><strong>{{ problem.name }}</strong> — {{ problem.problem }}</span></li>
        </ul>
        <div class="pair__field"><span>Book</span><UiSelect v-model="bookId" label="The book it reads" :options="books" placeholder="Choose the book…" /></div>
        <p class="pair__hint">Aoide first checks three short samples against the book's text, then listens to the whole recording in the background: about {{ estimate }} on a typical computer, faster with a GPU. Nothing leaves your computer and the files are not changed or copied.</p>
        <p v-if="replacing" class="pair__warn">This book already has a recording; it will be replaced.</p>
      </template>
      <p v-if="error" class="pair__error" role="alert">{{ error }}</p>
      <footer class="pair__actions">
        <button type="button" class="pair__button" @click="recordings.closePairing()">Cancel</button>
        <button type="button" class="pair__button pair__button--primary" :disabled="!bookId || !usable || busy" @click="pair">Check and sync</button>
      </footer>
    </div>
  </UiDialog>
</template>

<style scoped>
.pair { width: min(560px, 92vw); display: grid; gap: 14px; padding: 20px 22px; }
.pair__head { display: flex; align-items: center; gap: 10px; color: var(--accent); }
.pair__title { flex: 1; font-size: 16px; color: var(--ink); margin: 0; }
.pair__close { display: grid; place-items: center; width: 32px; height: 32px; border-radius: 50%; background: var(--glass); color: var(--ink); }
.pair__summary { font-size: 13px; font-weight: 600; }
.pair__files, .pair__problems { list-style: none; margin: 0; padding: 0; display: grid; gap: 4px; max-height: 200px; overflow-y: auto; }
.pair__file { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: 8px; font-size: 12.5px; color: var(--dim); }
.pair__name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--ink); }
.pair__length { font-variant-numeric: tabular-nums; }
.pair__more { font-size: 12px; color: var(--faint); padding-left: 22px; }
.pair__problem { display: flex; gap: 8px; font-size: 12.5px; color: var(--warn); line-height: 1.45; }
.pair__problem svg { flex-shrink: 0; margin-top: 2px; }
.pair__field { display: grid; grid-template-columns: 60px minmax(0, 1fr); align-items: center; gap: 10px; font-size: 13px; }
.pair__field > span { color: var(--dim); }
.pair__hint { font-size: 12px; color: var(--dim); line-height: 1.5; }
.pair__warn { font-size: 12px; font-weight: 600; color: var(--warn); }
.pair__error { font-size: 12px; font-weight: 600; color: var(--accent); overflow-wrap: anywhere; }
.pair__actions { display: flex; justify-content: flex-end; gap: 8px; }
.pair__button { border-radius: 999px; padding: 9px 18px; font-size: 13px; font-weight: 700; background: var(--ground); }
.pair__button--primary { background: var(--accent); color: var(--on-accent); }
</style>
