<script setup lang="ts">
import UiCard from "@/components/ui/UiCard.vue";
import UiButton from "@/components/ui/UiButton.vue";
import UiSegmented from "@/components/ui/UiSegmented.vue";
import ScanFolders, { type Folders } from "@/components/find/ScanFolders.vue";
import { computed, onMounted, ref } from "vue";
import { ArrowLeft, BookOpen, FolderSearch, Globe, Download, Headphones, X } from "@lucide/vue";
import { useNavigationStore } from "@/stores/navigation";
import { api } from "@/api";
import type { AppSettings, CandidateStatus } from "@/api/types";
import { formatBytes } from "@/bytes";
import { useDiscoveryStore } from "@/stores/discovery";
import { useRecordingsStore } from "@/stores/recordings";
import { audiobookName } from "@/recording";
const discovery = useDiscoveryStore(),
  navigation = useNavigationStore(),
  recordings = useRecordingsStore();
const settings = ref<AppSettings | null>(null);
onMounted(async () => {
  void discovery.start();
  try {
    settings.value = await api.appSettings();
  } catch (e) {
    discovery.error = String(e);
  }
});
async function saveFolders(folders: Folders, onStartup: boolean) {
  settings.value = await api.updateAppSettings({ patch: { scan: { ...folders, onStartup } } });
}
const groups = computed(() =>
  (
    [
      { value: "new", label: "New", ariaLabel: "Show new books" },
      { value: "inLibrary", label: "In library", ariaLabel: "Show books already in the library" },
      { value: "removed", label: "Removed earlier", ariaLabel: "Show books removed from the library" },
      { value: "unreadable", label: "Unreadable", ariaLabel: "Show unreadable files" },
    ] as const
  ).map((g) => ({ ...g, count: discovery.counts[g.value as CandidateStatus] })),
);
const importable = computed(() => discovery.filter === "new" || discovery.filter === "removed");
const status = computed(() => {
  const p = discovery.progress;
  if (!discovery.scanning || !p) return "";
  if (p.stage === "walking")
    return `Looking through ${p.dirs.toLocaleString()} folders · ${p.files.toLocaleString()} files · ${p.found} EPUBs so far`;
  if (p.stage === "reading") return `Reading ${p.read} of ${p.toRead} new or changed books…`;
  return "";
});
const where = (path: string) => path.split("/").slice(0, -1).join("/") || "/";
const anyAudiobook = computed(() => discovery.visible.some((c) => c.audiobook.length));
</script>

<template>
  <section class="find">
    <UiButton
      variant="glass"
      size="sm"
      class="find__back"
      aria-label="Back to library"
      @click="navigation.view = 'library'"
    >
      <ArrowLeft :size="15" /> Library
    </UiButton>
    <header class="find__header">
      <div>
        <h1>Find books</h1>
        <p class="find__subtitle">
          Scan this computer for EPUBs that are not in your library yet, and the audiobooks kept beside them.
        </p>
      </div>
      <div class="find__actions">
        <UiButton
          variant="glass"
          size="sm"
          v-if="discovery.scanning"
          class="find__button"
          aria-label="Cancel scan"
          @click="discovery.cancel()"
        >
          <X :size="15" /> Cancel
        </UiButton>
        <UiButton
          variant="accent"
          class="find__primary"
          aria-label="Scan for books"
          :disabled="discovery.scanning"
          @click="discovery.scan()"
        >
          <FolderSearch :size="16" /> {{ discovery.scanning ? "Scanning…" : "Scan this computer" }}
        </UiButton>
      </div>
    </header>
    <p v-if="status" class="find__status" role="status">{{ status }}</p>
    <p v-if="discovery.error" class="find__error" role="alert">{{ discovery.error }}</p>

    <ScanFolders
      :folders="settings?.scan ?? null"
      :auto="settings?.scan.onStartup ?? false"
      auto-label="Look for new books when Aoide starts"
      :labels="{ roots: 'Folders to search', exclude: 'Folders to skip', includeHidden: 'Hidden folders to include' }"
      :save="saveFolders"
      @error="discovery.error = $event"
    >
      Aoide skips hidden folders, build and dependency folders, other drives, and its own library. Files are only read,
      never changed.
    </ScanFolders>

    <template v-if="discovery.candidates.length">
      <div class="find__toolbar">
        <UiSegmented v-model="discovery.filter" label="Show" tone="wash" :options="groups" />
        <div v-if="discovery.filter === 'new'" class="find__row">
          <UiButton
            size="sm"
            class="find__button"
            aria-label="Fetch details online"
            :disabled="!discovery.needingDetails.length || !!discovery.fetching"
            @click="discovery.fetchDetails()"
          >
            <Globe :size="14" />
            {{
              discovery.fetching
                ? `Fetching ${discovery.fetching.done} / ${discovery.fetching.total}`
                : `Fetch details for ${discovery.needingDetails.length} books`
            }}
          </UiButton>
          <label class="find__check"
            ><input v-model="discovery.applyDetails" type="checkbox" aria-label="Use online details when importing" />
            Use online details when importing</label
          >
        </div>
      </div>
      <p v-if="discovery.filter === 'new'" class="find__hint">
        Fetching details sends each book's title, first author and ISBN to Open Library. Results are kept for 30 days.
      </p>

      <p v-if="!discovery.visible.length" class="find__empty">Nothing here.</p>
      <ul class="find__grid">
        <UiCard
          as="li"
          pad="sm"
          v-for="c in discovery.visible"
          :key="c.id"
          class="find__card"
          :class="{ 'find__card--selected': discovery.selected.has(c.id) }"
        >
          <label v-if="importable" class="find__select"
            ><input
              type="checkbox"
              :checked="discovery.selected.has(c.id)"
              :aria-label="`Select ${c.title}`"
              @change="discovery.toggle(c.id, ($event.target as HTMLInputElement).checked)"
          /></label>
          <div class="find__cover">
            <img v-if="c.coverUrl" :src="c.coverUrl" alt="" loading="lazy" /><img
              v-else-if="c.suggestion?.coverUrl"
              :src="c.suggestion.coverUrl"
              alt=""
              loading="lazy"
              referrerpolicy="no-referrer"
            /><BookOpen v-else :size="26" :stroke-width="1.2" />
          </div>
          <div class="find__info">
            <h3>{{ c.title }}</h3>
            <p class="find__authors">{{ c.authors.join(", ") || "Unknown author" }}</p>
            <p v-if="c.suggestion" class="find__suggestion" :title="c.suggestion.title">
              <Globe :size="12" />
              {{ [c.suggestion.published, c.suggestion.publisher].filter(Boolean).join(" · ") || c.suggestion.title }}
            </p>
            <p v-if="c.error" class="find__error">{{ c.error }}</p>
            <p class="find__path" :title="c.paths.join('\n')">
              {{ where(c.paths[0]) }}<span v-if="c.paths.length > 1"> · {{ c.paths.length }} copies</span>
            </p>
            <p class="find__size">{{ formatBytes(c.size) }}</p>
            <div v-if="c.audiobook.length" class="find__audiobook">
              <Headphones :size="12" /><span class="find__audiobook-name" :title="c.audiobook.join('\n')">{{
                audiobookName(c.audiobook)
              }}</span>
              <span v-if="recordings.byBook[c.id]" class="find__paired">Paired</span>
              <button
                v-else-if="c.status === 'inLibrary'"
                type="button"
                class="find__pair"
                :aria-label="`Pair the audiobook beside ${c.title}`"
                @click="recordings.openPairing(c.audiobook, c.id)"
              >
                Pair…
              </button>
            </div>
          </div>
        </UiCard>
      </ul>
      <footer v-if="importable && discovery.visible.length" class="find__footer">
        <label v-if="anyAudiobook" class="find__check"
          ><input
            v-model="discovery.pairAudiobooks"
            type="checkbox"
            aria-label="Pair audiobooks found beside books when importing"
          />
          Pair audiobooks found beside them</label
        >
        <span v-if="discovery.importing" role="status"
          >Importing {{ discovery.importing.done + 1 }} of {{ discovery.importing.total
          }}{{ discovery.importing.current ? ` · ${discovery.importing.current}` : "" }}</span
        >
        <UiButton
          size="sm"
          class="find__button"
          aria-label="Import selected books"
          :disabled="!discovery.selected.size || !!discovery.importing"
          @click="discovery.importBooks([...discovery.selected])"
        >
          <Download :size="14" /> Import selected ({{ discovery.selected.size }})
        </UiButton>
        <UiButton
          variant="accent"
          v-if="discovery.filter === 'new'"
          class="find__primary"
          aria-label="Import all new books"
          :disabled="!!discovery.importing"
          @click="discovery.importBooks(discovery.visible.map((c) => c.id))"
        >
          <Download :size="14" /> Import all {{ discovery.visible.length }}
        </UiButton>
      </footer>
    </template>
    <div v-else-if="!discovery.scanning" class="find__empty">
      <FolderSearch :size="36" :stroke-width="1.2" />
      <p>Scan this computer to find EPUBs you have not added yet. It usually takes a few seconds.</p>
    </div>
  </section>
</template>

<style scoped>
.find {
  max-width: 1180px;
  margin: 0 auto;
  padding: var(--space-4) var(--space-14) 0;
}
.find__back {
  margin: var(--space-4) 0 var(--space-9);
}
.find__header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-10);
  margin-bottom: var(--space-10);
}
.find__header h1 {
  font-size: 32px;
  font-weight: 800;
  letter-spacing: -0.03em;
}
.find__subtitle {
  color: var(--dim);
  margin-top: var(--space-2);
}
.find__actions,
.find__row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-5);
}
.find__status {
  font-size: 13px;
  font-weight: 600;
  color: var(--dim);
  font-variant-numeric: tabular-nums;
}
.find__error {
  color: var(--accent);
  font-size: 12px;
  font-weight: 600;
  white-space: pre-line;
  overflow-wrap: anywhere;
}
.find__hint {
  font-size: 12px;
  color: var(--dim);
}
.find__check {
  display: inline-flex;
  align-items: center;
  gap: var(--space-4);
  font-size: 13px;
}
.find__toolbar {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: center;
  gap: var(--space-6);
  margin-bottom: var(--space-4);
}
.find__grid {
  list-style: none;
  padding: 0;
  margin: var(--space-9) 0;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: var(--space-7);
}
.find__card {
  position: relative;
  display: grid;
  grid-template-columns: 64px minmax(0, 1fr);
  gap: var(--space-7);
}
.find__card--selected {
  box-shadow: 0 0 0 2px var(--accent);
}
.find__select {
  position: absolute;
  top: 10px;
  right: 10px;
}
.find__cover {
  width: 64px;
  height: 92px;
  border-radius: var(--radius-sm);
  background: var(--wash);
  display: grid;
  place-items: center;
  color: var(--dim);
  overflow: hidden;
}
.find__cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.find__info {
  min-width: 0;
  display: grid;
  gap: var(--space-1);
  align-content: start;
  padding-right: var(--space-11);
}
.find__info h3 {
  font-size: 14px;
  font-weight: 700;
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}
.find__authors,
.find__path,
.find__size {
  font-size: 12px;
  margin: 0;
  color: var(--dim);
}
.find__path,
.find__size {
  color: var(--faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.find__audiobook {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
  margin-top: var(--space-1);
  font-size: 11.5px;
  font-weight: 600;
  color: var(--dim);
}
.find__audiobook-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.find__pair {
  flex: none;
  margin-left: auto;
  padding: var(--space-1) var(--space-5);
  border-radius: var(--radius-pill);
  background: var(--ground);
  font-size: 11.5px;
  font-weight: 700;
}
.find__paired {
  flex: none;
  margin-left: auto;
  color: var(--ok);
}
.find__suggestion {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: 11px;
  font-weight: 600;
  color: var(--accent);
  margin: 0;
}
.find__footer {
  position: sticky;
  bottom: 16px;
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  align-items: center;
  gap: var(--space-5);
  padding: var(--space-6);
  border-radius: var(--radius-pill);
  background: var(--page);
  box-shadow: 0 18px 40px -16px var(--shadow);
  font-size: 13px;
  font-weight: 600;
}
.find__empty {
  text-align: center;
  color: var(--dim);
  padding: var(--space-20) var(--space-8);
  display: grid;
  justify-items: center;
  gap: var(--space-6);
}
@media (max-width: 560px) {
  .find {
    padding: var(--space-4) var(--space-8) 0;
  }
  .find__grid {
    grid-template-columns: 1fr;
  }
  .find__footer {
    border-radius: var(--radius-xl);
  }
}
</style>
