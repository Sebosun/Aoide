<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { ArrowLeft, BookOpen, FolderSearch, Globe, Download, X } from "@lucide/vue";
import { useNavigationStore } from "../stores/navigation";
import { api } from "../api";
import type { AppSettings, CandidateStatus } from "../api/types";
import { formatBytes } from "../bytes";
import { useDiscoveryStore } from "../stores/discovery";
const discovery = useDiscoveryStore(), navigation = useNavigationStore();
const settings = ref<AppSettings | null>(null), roots = ref(""), exclude = ref(""), includeHidden = ref(""), onStartup = ref(false), foldersMessage = ref("");
onMounted(async () => {
  void discovery.start();
  try {
    settings.value = await api.appSettings();
    const scan = settings.value.scan;
    roots.value = scan.roots.join("\n"); exclude.value = scan.exclude.join("\n"); includeHidden.value = scan.includeHidden.join("\n"); onStartup.value = scan.onStartup;
  } catch (e) { discovery.error = String(e); }
});
const lines = (text: string) => text.split("\n").map(l => l.trim()).filter(Boolean);
async function saveFolders() {
  if (!settings.value) return;
  foldersMessage.value = "";
  const next: AppSettings = { ...settings.value, scan: { roots: lines(roots.value), exclude: lines(exclude.value), includeHidden: lines(includeHidden.value), onStartup: onStartup.value } };
  try { await api.saveAppSettings(next); settings.value = next; foldersMessage.value = "Saved."; } catch (e) { discovery.error = String(e); }
}
const groups: { status: CandidateStatus; label: string; aria: string }[] = [
  { status: "new", label: "New", aria: "Show new books" },
  { status: "inLibrary", label: "In library", aria: "Show books already in the library" },
  { status: "removed", label: "Removed earlier", aria: "Show books removed from the library" },
  { status: "unreadable", label: "Unreadable", aria: "Show unreadable files" },
];
const importable = computed(() => discovery.filter === "new" || discovery.filter === "removed");
const status = computed(() => {
  const p = discovery.progress;
  if (!discovery.scanning || !p) return "";
  if (p.stage === "walking") return `Looking through ${p.dirs.toLocaleString()} folders · ${p.files.toLocaleString()} files · ${p.found} EPUBs so far`;
  if (p.stage === "reading") return `Reading ${p.read} of ${p.toRead} new or changed books…`;
  return "";
});
const where = (path: string) => path.split("/").slice(0, -1).join("/") || "/";
</script>

<template>
  <section class="find">
    <button type="button" class="find__back" aria-label="Back to library" @click="navigation.view = 'library'"><ArrowLeft :size="15" /> Library</button>
    <header class="find__header">
      <div><h1>Find books</h1><p class="find__subtitle">Scan this computer for EPUBs that are not in your library yet.</p></div>
      <div class="find__actions">
        <button v-if="discovery.scanning" type="button" class="find__button" aria-label="Cancel scan" @click="discovery.cancel()"><X :size="15" /> Cancel</button>
        <button type="button" class="find__primary" aria-label="Scan for books" :disabled="discovery.scanning" @click="discovery.scan()"><FolderSearch :size="16" /> {{ discovery.scanning ? 'Scanning…' : 'Scan this computer' }}</button>
      </div>
    </header>
    <p v-if="status" class="find__status" role="status">{{ status }}</p>
    <p v-if="discovery.error" class="find__error" role="alert">{{ discovery.error }}</p>

    <details class="find__settings">
      <summary>Folders · {{ settings?.scan.roots.length ? settings.scan.roots.join(', ') : 'your home folder' }}</summary>
      <form class="find__folders" @submit.prevent="saveFolders">
        <label class="find__field"><span>Search<br /><small>one per line; empty = home folder</small></span><textarea v-model="roots" rows="2" aria-label="Folders to search" spellcheck="false" /></label>
        <label class="find__field"><span>Skip<br /><small>one per line</small></span><textarea v-model="exclude" rows="2" aria-label="Folders to skip" spellcheck="false" /></label>
        <label class="find__field"><span>Hidden folders to include<br /><small>skipped by default</small></span><textarea v-model="includeHidden" rows="2" aria-label="Hidden folders to include" spellcheck="false" /></label>
        <label class="find__check"><input v-model="onStartup" type="checkbox" aria-label="Look for new books when Aoide starts" /> Look for new books when Aoide starts</label>
        <p class="find__hint">Aoide skips hidden folders, build and dependency folders, other drives, and its own library. Files are only read, never changed.</p>
        <div class="find__row"><button type="submit" class="find__button">Save folders</button><span v-if="foldersMessage" class="find__ok" role="status">{{ foldersMessage }}</span></div>
      </form>
    </details>

    <template v-if="discovery.candidates.length">
      <div class="find__toolbar">
        <div class="find__tabs" role="tablist">
          <button v-for="g in groups" :key="g.status" type="button" role="tab" class="find__tab" :class="{ 'find__tab--active': discovery.filter === g.status }" :aria-selected="discovery.filter === g.status" :aria-label="g.aria" @click="discovery.filter = g.status">{{ g.label }} <span class="find__count">{{ discovery.counts[g.status] }}</span></button>
        </div>
        <div v-if="discovery.filter === 'new'" class="find__row">
          <button type="button" class="find__button" aria-label="Fetch details online" :disabled="!discovery.needingDetails.length || !!discovery.fetching" @click="discovery.fetchDetails()"><Globe :size="14" /> {{ discovery.fetching ? `Fetching ${discovery.fetching.done} / ${discovery.fetching.total}` : `Fetch details for ${discovery.needingDetails.length} books` }}</button>
          <label class="find__check"><input v-model="discovery.applyDetails" type="checkbox" aria-label="Use online details when importing" /> Use online details when importing</label>
        </div>
      </div>
      <p v-if="discovery.filter === 'new'" class="find__hint">Fetching details sends each book's title, first author and ISBN to Open Library. Results are kept for 30 days.</p>

      <p v-if="!discovery.visible.length" class="find__empty">Nothing here.</p>
      <ul class="find__grid">
        <li v-for="c in discovery.visible" :key="c.id" class="find__card" :class="{ 'find__card--selected': discovery.selected.has(c.id) }">
          <label v-if="importable" class="find__select"><input type="checkbox" :checked="discovery.selected.has(c.id)" :aria-label="`Select ${c.title}`" @change="discovery.toggle(c.id, ($event.target as HTMLInputElement).checked)" /></label>
          <div class="find__cover"><img v-if="c.coverUrl" :src="c.coverUrl" alt="" loading="lazy" /><img v-else-if="c.suggestion?.coverUrl" :src="c.suggestion.coverUrl" alt="" loading="lazy" referrerpolicy="no-referrer" /><BookOpen v-else :size="26" :stroke-width="1.2" /></div>
          <div class="find__info">
            <h3>{{ c.title }}</h3>
            <p class="find__authors">{{ c.authors.join(', ') || 'Unknown author' }}</p>
            <p v-if="c.suggestion" class="find__suggestion" :title="c.suggestion.title"><Globe :size="12" /> {{ [c.suggestion.published, c.suggestion.publisher].filter(Boolean).join(' · ') || c.suggestion.title }}</p>
            <p v-if="c.error" class="find__error">{{ c.error }}</p>
            <p class="find__path" :title="c.paths.join('\n')">{{ where(c.paths[0]) }}<span v-if="c.paths.length > 1"> · {{ c.paths.length }} copies</span></p>
            <p class="find__size">{{ formatBytes(c.size) }}</p>
          </div>
        </li>
      </ul>
      <footer v-if="importable && discovery.visible.length" class="find__footer">
        <span v-if="discovery.importing" role="status">Importing {{ discovery.importing.done + 1 }} of {{ discovery.importing.total }}{{ discovery.importing.current ? ` · ${discovery.importing.current}` : '' }}</span>
        <button type="button" class="find__button" aria-label="Import selected books" :disabled="!discovery.selected.size || !!discovery.importing" @click="discovery.importBooks([...discovery.selected])"><Download :size="14" /> Import selected ({{ discovery.selected.size }})</button>
        <button v-if="discovery.filter === 'new'" type="button" class="find__primary" aria-label="Import all new books" :disabled="!!discovery.importing" @click="discovery.importBooks(discovery.visible.map(c => c.id))"><Download :size="14" /> Import all {{ discovery.visible.length }}</button>
      </footer>
    </template>
    <div v-else-if="!discovery.scanning" class="find__empty"><FolderSearch :size="36" :stroke-width="1.2" /><p>Scan this computer to find EPUBs you have not added yet. It usually takes a few seconds.</p></div>
  </section>
</template>

<style scoped>
.find { max-width: 1180px; margin: 0 auto; padding: 8px 28px 0; }
.find__back { display: inline-flex; align-items: center; gap: 6px; margin: 8px 0 18px; padding: 6px 12px; border-radius: 999px; background: var(--glass); font-weight: 700; font-size: 13px; }
.find__header { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 20px; margin-bottom: 20px; }
.find__header h1 { font-size: 32px; font-weight: 800; letter-spacing: -.03em; }
.find__subtitle { color: var(--dim); margin-top: 4px; }
.find__actions, .find__row { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; }
.find__primary { display: inline-flex; align-items: center; gap: 8px; height: 42px; padding: 0 20px; border-radius: 999px; background: var(--accent); color: var(--on-accent); font-weight: 800; }
.find__button { display: inline-flex; align-items: center; gap: 6px; height: 36px; padding: 0 14px; border-radius: 999px; background: var(--glass); font-size: 13px; font-weight: 700; }
.find__status { font-size: 13px; font-weight: 600; color: var(--dim); font-variant-numeric: tabular-nums; }
.find__error { color: var(--accent); font-size: 12px; font-weight: 600; white-space: pre-line; overflow-wrap: anywhere; }
.find__ok { color: var(--ok); font-size: 12px; font-weight: 600; }
.find__hint { font-size: 12px; color: var(--dim); }
.find__settings { margin: 8px 0 26px; padding: 16px 20px; border-radius: 22px; background: var(--page); font-size: 13px; }
.find__settings summary { cursor: pointer; font-weight: 600; color: var(--dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.find__settings .find__button { background: var(--ground); }
.find__folders { display: grid; gap: 12px; margin-top: 14px; }
.find__field { display: grid; grid-template-columns: 170px minmax(0, 1fr); gap: 12px; align-items: start; }
.find__field small { color: var(--faint); }
.find__check { display: inline-flex; align-items: center; gap: 8px; font-size: 13px; }
.find__toolbar { display: flex; flex-wrap: wrap; justify-content: space-between; align-items: center; gap: 12px; margin-bottom: 8px; }
.find__tabs { display: flex; flex-wrap: wrap; gap: 8px; }
.find__tab { padding: 6px 13px; border-radius: 999px; background: var(--glass); font-size: 12.5px; font-weight: 600; color: var(--dim); }
.find__tab--active { background: var(--ink); color: var(--ground); }
.find__count { margin-left: 5px; font-variant-numeric: tabular-nums; }
.find__grid { list-style: none; padding: 0; margin: 18px 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 14px; }
.find__card { position: relative; display: grid; grid-template-columns: 64px minmax(0, 1fr); gap: 14px; padding: 12px; border-radius: 16px; background: var(--page); }
.find__card--selected { box-shadow: 0 0 0 2px var(--accent); }
.find__select { position: absolute; top: 10px; right: 10px; }
.find__cover { width: 64px; height: 92px; border-radius: 8px; background: var(--wash); display: grid; place-items: center; color: var(--dim); overflow: hidden; }
.find__cover img { width: 100%; height: 100%; object-fit: cover; }
.find__info { min-width: 0; display: grid; gap: 3px; align-content: start; padding-right: 22px; }
.find__info h3 { font-size: 14px; font-weight: 700; margin: 0; overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; }
.find__authors, .find__path, .find__size { font-size: 12px; margin: 0; color: var(--dim); }
.find__path, .find__size { color: var(--faint); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.find__suggestion { display: flex; align-items: center; gap: 5px; font-size: 11px; font-weight: 600; color: var(--accent); margin: 0; }
.find__footer { position: sticky; bottom: 100px; display: flex; flex-wrap: wrap; justify-content: flex-end; align-items: center; gap: 10px; padding: 12px; border-radius: 999px; background: var(--page); box-shadow: 0 18px 40px -16px var(--shadow); font-size: 13px; font-weight: 600; }
.find__footer .find__button { background: var(--ground); }
.find__empty { text-align: center; color: var(--dim); padding: 40px 16px; display: grid; justify-items: center; gap: 12px; }
@media (max-width: 560px) { .find { padding: 8px 16px 0; } .find__field { grid-template-columns: 1fr; } .find__grid { grid-template-columns: 1fr; } .find__footer { border-radius: 22px; } }
</style>
