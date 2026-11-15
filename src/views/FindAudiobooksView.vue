<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { ArrowLeft, BookOpen, Folder, Headphones, Link2, Settings, X } from "@lucide/vue";
import { api } from "../api";
import type { AppSettings, AudiobookMatch, AudiobookSuggestion } from "../api/types";
import { formatBytes } from "../bytes";
import { useAudiobooksStore, type MatchGroup } from "../stores/audiobooks";
import { useLibraryStore } from "../stores/library";
import { useNavigationStore } from "../stores/navigation";
import { useRecordingsStore } from "../stores/recordings";
import { recordingStatus } from "../recording";

/** Find audiobooks: audiobooks on this computer matched by name to books in the library. The
 *  user confirms which to pair, or (Settings → Recordings) lets Aoide pair and check them. */
const audiobooks = useAudiobooksStore(), library = useLibraryStore(), navigation = useNavigationStore(), recordings = useRecordingsStore();
const settings = ref<AppSettings | null>(null), roots = ref(""), exclude = ref(""), includeHidden = ref(""), onAdd = ref(true), foldersMessage = ref("");
onMounted(async () => {
  void library.start(); void recordings.start(); void audiobooks.start();
  try {
    settings.value = await api.appSettings();
    const search = settings.value.audiobooks;
    roots.value = search.roots.join("\n"); exclude.value = search.exclude.join("\n"); includeHidden.value = search.includeHidden.join("\n"); onAdd.value = search.onAdd;
  } catch (e) { audiobooks.error = String(e); }
});
const lines = (text: string) => text.split("\n").map(l => l.trim()).filter(Boolean);
async function saveFolders() {
  foldersMessage.value = "";
  try {
    const latest = await api.appSettings();
    const next: AppSettings = { ...latest, audiobooks: { roots: lines(roots.value), exclude: lines(exclude.value), includeHidden: lines(includeHidden.value), onAdd: onAdd.value } };
    await api.saveAppSettings(next); settings.value = next; foldersMessage.value = "Saved.";
  } catch (e) { audiobooks.error = String(e); }
}
const groups: { group: MatchGroup; label: string; aria: string }[] = [
  { group: "found", label: "Found", aria: "Show found audiobooks" },
  { group: "checking", label: "Being checked", aria: "Show books being checked" },
  { group: "paired", label: "Paired", aria: "Show paired books" },
  { group: "unverified", label: "None passed", aria: "Show books none of whose audiobooks passed" },
];
const status = computed(() => {
  const p = audiobooks.progress;
  if (!audiobooks.scanning || !p || p.stage !== "walking") return "";
  return `Looking through ${p.dirs.toLocaleString()} folders · ${p.files.toLocaleString()} files · ${p.found.toLocaleString()} audio files so far`;
});
const verifying = computed(() => Boolean(settings.value?.verifyAudiobooks));
const book = (id: string) => library.books.find(b => b.id === id);
/** Whether the book's audiobooks have exactly its words. The core lists either only those, or
 *  only close ones, so the closest says it for all of them. */
const exact = (m: AudiobookMatch) => m.candidates[0]?.exact ?? false;
const title = (m: AudiobookMatch) => book(m.bookId)?.title ?? "A book";
/** An audiobook's name: its folder's, or its (first) file's. */
const nameOf = (c: AudiobookSuggestion) => c.paths[0].split("/").pop() ?? c.paths[0];
/** What it holds: "Folder · 15 files · 1.2 GB". */
const holds = (c: AudiobookSuggestion) => [c.folder ? "Folder" : null, `${c.files.length} ${c.files.length === 1 ? "file" : "files"}`, formatBytes(c.bytes)].filter(Boolean).join(" · ");
/** The candidate Aoide is checking or that is paired, if any. */
const current = (m: AudiobookMatch) => m.stage.kind === "verifying" || m.stage.kind === "paired" ? m.candidates[m.stage.candidate] : null;
const syncLabel = (bookId: string) => { const r = recordings.byBook[bookId]; return r ? recordingStatus(r).label : ""; };
const pairableIds = computed(() => audiobooks.pairable ? audiobooks.visible.map(m => m.bookId) : []);
function openSettings() { navigation.openSettings("recordings"); }
</script>

<template>
  <section class="find-audiobooks">
    <button type="button" class="find-audiobooks__back" aria-label="Back to library" @click="navigation.view = 'library'"><ArrowLeft :size="15" /> Library</button>
    <header class="find-audiobooks__header">
      <div><h1>Find audiobooks</h1><p class="find-audiobooks__subtitle">Match audiobooks on this computer to the books in your library, by their names.</p></div>
      <div class="find-audiobooks__actions">
        <button v-if="audiobooks.scanning" type="button" class="find-audiobooks__button" aria-label="Cancel search" @click="audiobooks.cancel()"><X :size="15" /> Cancel</button>
        <button type="button" class="find-audiobooks__primary" aria-label="Scan for audiobooks" :disabled="audiobooks.scanning" @click="audiobooks.scan()"><Headphones :size="16" /> {{ audiobooks.scanning ? 'Searching…' : 'Scan for audiobooks' }}</button>
      </div>
    </header>
    <p v-if="status" class="find-audiobooks__status" role="status">{{ status }}</p>
    <p v-if="audiobooks.error" class="find-audiobooks__error" role="alert">{{ audiobooks.error }}</p>

    <div class="find-audiobooks__mode">
      <p v-if="verifying">Aoide pairs each book's closest audiobook and checks a few samples against the text. If they don't match, it tries the next one.</p>
      <p v-else>Nothing is paired until you say so. Pairing checks a few samples against the text before the whole audiobook is synced.</p>
      <button type="button" class="find-audiobooks__link" @click="openSettings"><Settings :size="13" /> {{ verifying ? "Change in Settings" : "Let Aoide check them for you" }}</button>
    </div>

    <details class="find-audiobooks__settings">
      <summary>Folders · {{ settings?.audiobooks.roots.length ? settings.audiobooks.roots.join(', ') : 'your home folder' }}</summary>
      <form class="find-audiobooks__folders" @submit.prevent="saveFolders">
        <label class="find-audiobooks__field"><span>Search<br /><small>one per line; empty = home folder</small></span><textarea v-model="roots" rows="2" aria-label="Folders to search for audiobooks" spellcheck="false" /></label>
        <label class="find-audiobooks__field"><span>Skip<br /><small>one per line</small></span><textarea v-model="exclude" rows="2" aria-label="Folders to skip when searching for audiobooks" spellcheck="false" /></label>
        <label class="find-audiobooks__field"><span>Hidden folders to include<br /><small>skipped by default</small></span><textarea v-model="includeHidden" rows="2" aria-label="Hidden folders to include in the audiobook search" spellcheck="false" /></label>
        <label class="find-audiobooks__check"><input v-model="onAdd" type="checkbox" aria-label="Look for audiobooks when books are added" /> Look for audiobooks when books are added</label>
        <p class="find-audiobooks__hint">Only file and folder names are read. Audio files are never changed or copied.</p>
        <div class="find-audiobooks__row"><button type="submit" class="find-audiobooks__button">Save folders</button><span v-if="foldersMessage" class="find-audiobooks__ok" role="status">{{ foldersMessage }}</span></div>
      </form>
    </details>

    <template v-if="audiobooks.matches.length">
      <div class="find-audiobooks__tabs" role="tablist">
        <button v-for="g in groups" :key="g.group" type="button" role="tab" class="find-audiobooks__tab" :class="{ 'find-audiobooks__tab--active': audiobooks.filter === g.group }" :aria-selected="audiobooks.filter === g.group" :aria-label="g.aria" @click="audiobooks.show(g.group)">{{ g.label }} <span class="find-audiobooks__count">{{ audiobooks.counts[g.group] }}</span></button>
      </div>
      <p v-if="!audiobooks.visible.length" class="find-audiobooks__empty">Nothing here.</p>
      <ul class="find-audiobooks__grid">
        <li v-for="m in audiobooks.visible" :key="m.bookId" class="find-audiobooks__card" :class="{ 'find-audiobooks__card--selected': audiobooks.selected.has(m.bookId) }">
          <label v-if="audiobooks.pairable" class="find-audiobooks__select"><input type="checkbox" :checked="audiobooks.selected.has(m.bookId)" :aria-label="`Select ${title(m)}`" @change="audiobooks.toggle(m.bookId, ($event.target as HTMLInputElement).checked)" /></label>
          <div class="find-audiobooks__cover"><img v-if="book(m.bookId)?.coverUrl" :src="book(m.bookId)!.coverUrl!" alt="" loading="lazy" /><BookOpen v-else :size="24" :stroke-width="1.2" /></div>
          <div class="find-audiobooks__info">
            <p class="find-audiobooks__eyebrow" :class="{ 'find-audiobooks__eyebrow--exact': exact(m) }">{{ exact(m) ? "Same name" : "Similar name" }}</p>
            <h3>{{ title(m) }}</h3>
            <p class="find-audiobooks__authors">{{ book(m.bookId)?.authors.join(', ') || 'Unknown author' }}</p>
            <template v-if="current(m)">
              <p class="find-audiobooks__current" :title="current(m)!.files.join('\n')"><Link2 :size="12" /><span class="find-audiobooks__label"><span class="find-audiobooks__name">{{ m.stage.kind === 'verifying' ? 'Checking ' : '' }}{{ nameOf(current(m)!) }}</span><span class="find-audiobooks__meta">{{ holds(current(m)!) }}</span></span></p>
              <p v-if="syncLabel(m.bookId)" class="find-audiobooks__sync">{{ syncLabel(m.bookId) }}<span v-if="m.stage.kind === 'verifying'"> · audiobook {{ (m.stage.kind === 'verifying' ? m.stage.candidate : 0) + 1 }} of {{ m.candidates.length }}</span></p>
            </template>
            <template v-else>
              <p v-if="m.stage.kind === 'noneVerified'" class="find-audiobooks__note">None of {{ m.candidates.length === 1 ? "the audiobook's samples" : `these ${m.candidates.length} audiobooks` }} matched the text. You can still pair one.</p>
              <fieldset class="find-audiobooks__candidates">
                <legend class="visually-hidden">Audiobook for {{ title(m) }}</legend>
                <label v-for="(c, i) in m.candidates" :key="c.paths[0]" class="find-audiobooks__candidate" :title="c.files.join('\n')">
                  <input type="radio" :name="`candidate-${m.bookId}`" :checked="audiobooks.candidateOf(m.bookId) === i" :aria-label="`Pair ${nameOf(c)} with ${title(m)}`" @change="audiobooks.pick(m.bookId, i)" />
                  <component :is="c.folder ? Folder : Headphones" :size="13" class="find-audiobooks__kind" aria-hidden="true" />
                  <span class="find-audiobooks__label"><span class="find-audiobooks__name">{{ nameOf(c) }}</span><span class="find-audiobooks__meta">{{ holds(c) }}</span></span>
                </label>
              </fieldset>
            </template>
          </div>
        </li>
      </ul>
      <footer v-if="pairableIds.length" class="find-audiobooks__footer">
        <button type="button" class="find-audiobooks__button" aria-label="Pair selected audiobooks" :disabled="!audiobooks.selected.size || audiobooks.pairing" @click="audiobooks.pair([...audiobooks.selected])"><Link2 :size="14" /> Pair selected ({{ audiobooks.selected.size }})</button>
        <button type="button" class="find-audiobooks__primary" aria-label="Pair all found audiobooks" :disabled="audiobooks.pairing" @click="audiobooks.pair(pairableIds)"><Link2 :size="14" /> Pair all {{ pairableIds.length }}</button>
      </footer>
    </template>
    <div v-else-if="!audiobooks.scanning" class="find-audiobooks__empty"><Headphones :size="36" :stroke-width="1.2" /><p>Scan to find audiobooks for the books in your library that don't have one yet.</p></div>
  </section>
</template>

<style scoped>
.find-audiobooks { max-width: 1180px; margin: 0 auto; padding: 8px 28px 0; }
.find-audiobooks__back { display: inline-flex; align-items: center; gap: 6px; margin: 8px 0 18px; padding: 6px 12px; border-radius: 999px; background: var(--glass); font-weight: 700; font-size: 13px; }
.find-audiobooks__header { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 20px; margin-bottom: 20px; }
.find-audiobooks__header h1 { font-size: 32px; font-weight: 800; letter-spacing: -.03em; }
.find-audiobooks__subtitle { color: var(--dim); margin-top: 4px; }
.find-audiobooks__actions, .find-audiobooks__row { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; }
.find-audiobooks__primary { display: inline-flex; align-items: center; gap: 8px; height: 42px; padding: 0 20px; border-radius: 999px; background: var(--accent); color: var(--on-accent); font-weight: 800; }
.find-audiobooks__button { display: inline-flex; align-items: center; gap: 6px; height: 36px; padding: 0 14px; border-radius: 999px; background: var(--glass); font-size: 13px; font-weight: 700; }
.find-audiobooks__status { font-size: 13px; font-weight: 600; color: var(--dim); font-variant-numeric: tabular-nums; }
.find-audiobooks__error { color: var(--accent); font-size: 12px; font-weight: 600; white-space: pre-line; overflow-wrap: anywhere; }
.find-audiobooks__ok { color: var(--ok); font-size: 12px; font-weight: 600; }
.find-audiobooks__hint { font-size: 12px; color: var(--dim); }
.find-audiobooks__mode { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px 16px; margin: 0 0 12px; padding: 12px 18px; border-radius: 18px; background: var(--page); font-size: 13px; color: var(--dim); }
.find-audiobooks__link { display: inline-flex; align-items: center; gap: 6px; padding: 5px 12px; border-radius: 999px; background: var(--ground); font-size: 12px; font-weight: 700; color: var(--ink); }
.find-audiobooks__settings { margin: 0 0 26px; padding: 16px 20px; border-radius: 22px; background: var(--page); font-size: 13px; }
.find-audiobooks__settings summary { cursor: pointer; font-weight: 600; color: var(--dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.find-audiobooks__settings .find-audiobooks__button { background: var(--ground); }
.find-audiobooks__folders { display: grid; gap: 12px; margin-top: 14px; }
.find-audiobooks__field { display: grid; grid-template-columns: 170px minmax(0, 1fr); gap: 12px; align-items: start; }
.find-audiobooks__field small { color: var(--faint); }
.find-audiobooks__check { display: inline-flex; align-items: center; gap: 8px; font-size: 13px; }
.find-audiobooks__tabs { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 8px; }
.find-audiobooks__tab { padding: 6px 13px; border-radius: 999px; background: var(--glass); font-size: 12.5px; font-weight: 600; color: var(--dim); }
.find-audiobooks__tab--active { background: var(--ink); color: var(--ground); }
.find-audiobooks__count { margin-left: 5px; font-variant-numeric: tabular-nums; }
.find-audiobooks__grid { list-style: none; padding: 0; margin: 18px 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(340px, 1fr)); gap: 14px; }
.find-audiobooks__card { position: relative; display: grid; grid-template-columns: 56px minmax(0, 1fr); gap: 14px; padding: 12px; border-radius: 16px; background: var(--page); }
.find-audiobooks__card--selected { box-shadow: 0 0 0 2px var(--accent); }
.find-audiobooks__select { position: absolute; top: 10px; right: 10px; }
.find-audiobooks__cover { width: 56px; height: 80px; border-radius: 8px; background: var(--wash); display: grid; place-items: center; color: var(--dim); overflow: hidden; }
.find-audiobooks__cover img { width: 100%; height: 100%; object-fit: cover; }
.find-audiobooks__info { min-width: 0; display: grid; gap: 4px; align-content: start; padding-right: 22px; }
.find-audiobooks__info h3 { font-size: 14px; font-weight: 700; margin: 0; overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; }
.find-audiobooks__authors, .find-audiobooks__sync, .find-audiobooks__note { font-size: 12px; margin: 0; color: var(--dim); }
.find-audiobooks__current { display: flex; align-items: center; gap: 6px; min-width: 0; margin: 2px 0 0; font-size: 12px; font-weight: 600; }
.find-audiobooks__candidates { display: grid; gap: 4px; margin: 4px 0 0; padding: 0; border: 0; min-width: 0; }
.find-audiobooks__candidate { display: flex; align-items: center; gap: 6px; min-width: 0; padding: 6px 10px; border-radius: 12px; background: var(--ground); font-size: 12px; font-weight: 600; color: var(--dim); cursor: pointer; }
.find-audiobooks__name { min-width: 0; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.find-audiobooks__kind { flex: none; }
.find-audiobooks__label { display: grid; min-width: 0; flex: 1; }
.find-audiobooks__label .find-audiobooks__name { white-space: normal; overflow-wrap: anywhere; color: var(--ink); }
.find-audiobooks__meta { font-size: 11px; font-weight: 600; color: var(--faint); }
.find-audiobooks__eyebrow { justify-self: end; margin: 0 0 2px; font-size: 10.5px; font-weight: 700; letter-spacing: .06em; text-transform: uppercase; color: var(--faint); }
.find-audiobooks__eyebrow--exact { color: var(--ok); }
.find-audiobooks__footer { position: sticky; bottom: 100px; display: flex; flex-wrap: wrap; justify-content: flex-end; align-items: center; gap: 10px; padding: 12px; border-radius: 999px; background: var(--page); box-shadow: 0 18px 40px -16px var(--shadow); font-size: 13px; font-weight: 600; }
.find-audiobooks__footer .find-audiobooks__button { background: var(--ground); }
.find-audiobooks__empty { text-align: center; color: var(--dim); padding: 40px 16px; display: grid; justify-items: center; gap: 12px; }
@media (max-width: 560px) { .find-audiobooks { padding: 8px 16px 0; } .find-audiobooks__field { grid-template-columns: 1fr; } .find-audiobooks__grid { grid-template-columns: 1fr; } .find-audiobooks__footer { border-radius: 22px; } }
</style>
