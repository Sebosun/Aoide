<script setup lang="ts">
import UiCard from "@/components/ui/UiCard.vue";
import UiButton from "@/components/ui/UiButton.vue";
import UiSegmented from "@/components/ui/UiSegmented.vue";
import ScanFolders, { type Folders } from "@/components/find/ScanFolders.vue";
import { computed, onMounted, ref } from "vue";
import { ArrowLeft, BookOpen, Folder, Headphones, Link2, Settings, X } from "@lucide/vue";
import { api } from "@/api";
import type { AppSettings, AudiobookMatch, AudiobookSuggestion } from "@/api/types";
import { formatBytes } from "@/bytes";
import { useAudiobooksStore, type MatchGroup } from "@/stores/audiobooks";
import { useLibraryStore } from "@/stores/library";
import { useNavigationStore } from "@/stores/navigation";
import { useRecordingsStore } from "@/stores/recordings";
import { recordingStatus } from "@/recording";

/** Find audiobooks: audiobooks on this computer matched by name to books in the library. The
 *  user confirms which to pair, or (Settings → Recordings) lets Aoide pair and check them. */
const audiobooks = useAudiobooksStore(),
  library = useLibraryStore(),
  navigation = useNavigationStore(),
  recordings = useRecordingsStore();
const settings = ref<AppSettings | null>(null);
onMounted(async () => {
  void library.start();
  void recordings.start();
  void audiobooks.start();
  try {
    settings.value = await api.appSettings();
  } catch (e) {
    audiobooks.error = String(e);
  }
});
async function saveFolders(folders: Folders, onAdd: boolean) {
  settings.value = await api.updateAppSettings({ patch: { audiobooks: { ...folders, onAdd } } });
}
const groups = computed(() =>
  (
    [
      { value: "found", label: "Found", ariaLabel: "Show found audiobooks" },
      { value: "checking", label: "Being checked", ariaLabel: "Show books being checked" },
      { value: "paired", label: "Paired", ariaLabel: "Show paired books" },
      { value: "unverified", label: "None passed", ariaLabel: "Show books none of whose audiobooks passed" },
    ] as const
  ).map((g) => ({ ...g, count: audiobooks.counts[g.value as MatchGroup] })),
);
const status = computed(() => {
  const p = audiobooks.progress;
  if (!audiobooks.scanning || !p || p.stage !== "walking") return "";
  return `Looking through ${p.dirs.toLocaleString()} folders · ${p.files.toLocaleString()} files · ${p.found.toLocaleString()} audio files so far`;
});
const verifying = computed(() => Boolean(settings.value?.verifyAudiobooks));
const book = (id: string) => library.books.find((b) => b.id === id);
/** Whether the book's audiobooks have exactly its words. The core lists either only those, or
 *  only close ones, so the closest says it for all of them. */
const exact = (m: AudiobookMatch) => m.candidates[0]?.exact ?? false;
const title = (m: AudiobookMatch) => book(m.bookId)?.title ?? "A book";
/** An audiobook's name: its folder's, or its (first) file's. */
const nameOf = (c: AudiobookSuggestion) => c.paths[0].split("/").pop() ?? c.paths[0];
/** What it holds: "Folder · 15 files · 1.2 GB". */
const holds = (c: AudiobookSuggestion) =>
  [c.folder ? "Folder" : null, `${c.files.length} ${c.files.length === 1 ? "file" : "files"}`, formatBytes(c.bytes)]
    .filter(Boolean)
    .join(" · ");
/** The candidate Aoide is checking or that is paired, if any. */
const current = (m: AudiobookMatch) =>
  m.stage.kind === "verifying" || m.stage.kind === "paired" ? m.candidates[m.stage.candidate] : null;
const syncLabel = (bookId: string) => {
  const r = recordings.byBook[bookId];
  return r ? recordingStatus(r).label : "";
};
const pairableIds = computed(() => (audiobooks.pairable ? audiobooks.visible.map((m) => m.bookId) : []));
function openSettings() {
  navigation.openSettings("recordings");
}
</script>

<template>
  <section class="find-audiobooks">
    <UiButton
      variant="glass"
      size="sm"
      class="find-audiobooks__back"
      aria-label="Back to library"
      @click="navigation.view = 'library'"
    >
      <ArrowLeft :size="15" /> Library
    </UiButton>
    <header class="find-audiobooks__header">
      <div>
        <h1>Find audiobooks</h1>
        <p class="find-audiobooks__subtitle">
          Match audiobooks on this computer to the books in your library, by their names.
        </p>
      </div>
      <div class="find-audiobooks__actions">
        <UiButton
          variant="glass"
          size="sm"
          v-if="audiobooks.scanning"
          class="find-audiobooks__button"
          aria-label="Cancel search"
          @click="audiobooks.cancel()"
        >
          <X :size="15" /> Cancel
        </UiButton>
        <UiButton
          variant="accent"
          class="find-audiobooks__primary"
          aria-label="Scan for audiobooks"
          :disabled="audiobooks.scanning"
          @click="audiobooks.scan()"
        >
          <Headphones :size="16" /> {{ audiobooks.scanning ? "Searching…" : "Scan for audiobooks" }}
        </UiButton>
      </div>
    </header>
    <p v-if="status" class="find-audiobooks__status" role="status">{{ status }}</p>
    <p v-if="audiobooks.error" class="find-audiobooks__error" role="alert">{{ audiobooks.error }}</p>

    <div class="find-audiobooks__mode">
      <p v-if="verifying">
        Aoide pairs each book's closest audiobook and checks a few samples against the text. If they don't match, it
        tries the next one.
      </p>
      <p v-else>
        Nothing is paired until you say so. Pairing checks a few samples against the text before the whole audiobook is
        synced.
      </p>
      <button type="button" class="find-audiobooks__link" @click="openSettings">
        <Settings :size="13" /> {{ verifying ? "Change in Settings" : "Let Aoide check them for you" }}
      </button>
    </div>

    <ScanFolders
      :folders="settings?.audiobooks ?? null"
      :auto="settings?.audiobooks.onAdd ?? true"
      auto-label="Look for audiobooks when books are added"
      :labels="{
        roots: 'Folders to search for audiobooks',
        exclude: 'Folders to skip when searching for audiobooks',
        includeHidden: 'Hidden folders to include in the audiobook search',
      }"
      :save="saveFolders"
      @error="audiobooks.error = $event"
    >
      Only file and folder names are read. Audio files are never changed or copied.
    </ScanFolders>

    <template v-if="audiobooks.matches.length">
      <UiSegmented
        class="find-audiobooks__tabs"
        :model-value="audiobooks.filter"
        label="Show"
        tone="wash"
        :options="groups"
        @update:model-value="audiobooks.show"
      />
      <p v-if="!audiobooks.visible.length" class="find-audiobooks__empty">Nothing here.</p>
      <ul class="find-audiobooks__grid">
        <UiCard
          as="li"
          pad="sm"
          v-for="m in audiobooks.visible"
          :key="m.bookId"
          class="find-audiobooks__card"
          :class="{ 'find-audiobooks__card--selected': audiobooks.selected.has(m.bookId) }"
        >
          <label v-if="audiobooks.pairable" class="find-audiobooks__select"
            ><input
              type="checkbox"
              :checked="audiobooks.selected.has(m.bookId)"
              :aria-label="`Select ${title(m)}`"
              @change="audiobooks.toggle(m.bookId, ($event.target as HTMLInputElement).checked)"
          /></label>
          <div class="find-audiobooks__cover">
            <img v-if="book(m.bookId)?.coverUrl" :src="book(m.bookId)!.coverUrl!" alt="" loading="lazy" /><BookOpen
              v-else
              :size="24"
              :stroke-width="1.2"
            />
          </div>
          <div class="find-audiobooks__info">
            <p class="find-audiobooks__eyebrow" :class="{ 'find-audiobooks__eyebrow--exact': exact(m) }">
              {{ exact(m) ? "Same name" : "Similar name" }}
            </p>
            <h3>{{ title(m) }}</h3>
            <p class="find-audiobooks__authors">{{ book(m.bookId)?.authors.join(", ") || "Unknown author" }}</p>
            <template v-if="current(m)">
              <p class="find-audiobooks__current" :title="current(m)!.files.join('\n')">
                <Link2 :size="12" /><span class="find-audiobooks__label"
                  ><span class="find-audiobooks__name"
                    >{{ m.stage.kind === "verifying" ? "Checking " : "" }}{{ nameOf(current(m)!) }}</span
                  ><span class="find-audiobooks__meta">{{ holds(current(m)!) }}</span></span
                >
              </p>
              <p v-if="syncLabel(m.bookId)" class="find-audiobooks__sync">
                {{ syncLabel(m.bookId)
                }}<span v-if="m.stage.kind === 'verifying'">
                  · audiobook {{ (m.stage.kind === "verifying" ? m.stage.candidate : 0) + 1 }} of
                  {{ m.candidates.length }}</span
                >
              </p>
            </template>
            <template v-else>
              <p v-if="m.stage.kind === 'noneVerified'" class="find-audiobooks__note">
                None of
                {{ m.candidates.length === 1 ? "the audiobook's samples" : `these ${m.candidates.length} audiobooks` }}
                matched the text. You can still pair one.
              </p>
              <fieldset class="find-audiobooks__candidates">
                <legend class="visually-hidden">Audiobook for {{ title(m) }}</legend>
                <label
                  v-for="(c, i) in m.candidates"
                  :key="c.paths[0]"
                  class="find-audiobooks__candidate"
                  :title="c.files.join('\n')"
                >
                  <input
                    type="radio"
                    :name="`candidate-${m.bookId}`"
                    :checked="audiobooks.candidateOf(m.bookId) === i"
                    :aria-label="`Pair ${nameOf(c)} with ${title(m)}`"
                    @change="audiobooks.pick(m.bookId, i)"
                  />
                  <component
                    :is="c.folder ? Folder : Headphones"
                    :size="13"
                    class="find-audiobooks__kind"
                    aria-hidden="true"
                  />
                  <span class="find-audiobooks__label"
                    ><span class="find-audiobooks__name">{{ nameOf(c) }}</span
                    ><span class="find-audiobooks__meta">{{ holds(c) }}</span></span
                  >
                </label>
              </fieldset>
            </template>
          </div>
        </UiCard>
      </ul>
      <footer v-if="pairableIds.length" class="find-audiobooks__footer">
        <UiButton
          size="sm"
          class="find-audiobooks__button"
          aria-label="Pair selected audiobooks"
          :disabled="!audiobooks.selected.size || audiobooks.pairing"
          @click="audiobooks.pair([...audiobooks.selected])"
        >
          <Link2 :size="14" /> Pair selected ({{ audiobooks.selected.size }})
        </UiButton>
        <UiButton
          variant="accent"
          class="find-audiobooks__primary"
          aria-label="Pair all found audiobooks"
          :disabled="audiobooks.pairing"
          @click="audiobooks.pair(pairableIds)"
        >
          <Link2 :size="14" /> Pair all {{ pairableIds.length }}
        </UiButton>
      </footer>
    </template>
    <section v-if="audiobooks.unmatched.length" class="find-audiobooks__unmatched" aria-labelledby="find-unmatched">
      <h2 id="find-unmatched" class="find-audiobooks__heading">
        Not in your library <span class="find-audiobooks__count">{{ audiobooks.unmatched.length }}</span>
      </h2>
      <p class="find-audiobooks__hint">
        These match none of your books by name. Add one to listen to it on its own; you can add its EPUB later.
      </p>
      <ul class="find-audiobooks__others">
        <li
          v-for="c in audiobooks.unmatched"
          :key="c.paths[0]"
          class="find-audiobooks__other"
          :title="c.files.join('\n')"
        >
          <component :is="c.folder ? Folder : Headphones" :size="14" class="find-audiobooks__kind" aria-hidden="true" />
          <span class="find-audiobooks__label"
            ><span class="find-audiobooks__name">{{ nameOf(c) }}</span
            ><span class="find-audiobooks__meta">{{ holds(c) }}</span></span
          >
          <UiButton
            size="sm"
            class="find-audiobooks__button"
            :aria-label="`Add ${nameOf(c)} on its own`"
            :disabled="audiobooks.adding"
            @click="audiobooks.addUnmatched(c)"
          >
            Add
          </UiButton>
        </li>
      </ul>
    </section>
    <div
      v-if="!audiobooks.matches.length && !audiobooks.unmatched.length && !audiobooks.scanning"
      class="find-audiobooks__empty"
    >
      <Headphones :size="36" :stroke-width="1.2" />
      <p>Scan to find audiobooks for the books in your library that don't have one yet.</p>
    </div>
  </section>
</template>

<style scoped>
.find-audiobooks {
  max-width: 1180px;
  margin: 0 auto;
  padding: var(--space-4) var(--space-14) 0;
}
.find-audiobooks__back {
  margin: var(--space-4) 0 var(--space-9);
}
.find-audiobooks__header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-10);
  margin-bottom: var(--space-10);
}
.find-audiobooks__header h1 {
  font-size: 32px;
  font-weight: 800;
  letter-spacing: -0.03em;
}
.find-audiobooks__subtitle {
  color: var(--dim);
  margin-top: var(--space-2);
}
.find-audiobooks__actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-5);
}
.find-audiobooks__status {
  font-size: 13px;
  font-weight: 600;
  color: var(--dim);
  font-variant-numeric: tabular-nums;
}
.find-audiobooks__error {
  color: var(--accent);
  font-size: 12px;
  font-weight: 600;
  white-space: pre-line;
  overflow-wrap: anywhere;
}
.find-audiobooks__hint {
  font-size: 12px;
  color: var(--dim);
}
.find-audiobooks__mode {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4) var(--space-8);
  margin: 0 0 var(--space-6);
  padding: var(--space-6) var(--space-9);
  border-radius: var(--radius-lg);
  background: var(--page);
  font-size: 13px;
  color: var(--dim);
}
.find-audiobooks__link {
  display: inline-flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-6);
  border-radius: var(--radius-pill);
  background: var(--ground);
  font-size: 12px;
  font-weight: 700;
  color: var(--ink);
}
.find-audiobooks__tabs {
  margin-bottom: var(--space-4);
}
.find-audiobooks__count {
  margin-left: var(--space-2);
  font-variant-numeric: tabular-nums;
}
.find-audiobooks__grid {
  list-style: none;
  padding: 0;
  margin: var(--space-9) 0;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
  gap: var(--space-7);
}
.find-audiobooks__card {
  position: relative;
  display: grid;
  grid-template-columns: 56px minmax(0, 1fr);
  gap: var(--space-7);
}
.find-audiobooks__card--selected {
  box-shadow: 0 0 0 2px var(--accent);
}
.find-audiobooks__select {
  position: absolute;
  top: 10px;
  right: 10px;
}
.find-audiobooks__cover {
  width: 56px;
  height: 80px;
  border-radius: var(--radius-sm);
  background: var(--wash);
  display: grid;
  place-items: center;
  color: var(--dim);
  overflow: hidden;
}
.find-audiobooks__cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.find-audiobooks__info {
  min-width: 0;
  display: grid;
  gap: var(--space-2);
  align-content: start;
  padding-right: var(--space-11);
}
.find-audiobooks__info h3 {
  font-size: 14px;
  font-weight: 700;
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}
.find-audiobooks__authors,
.find-audiobooks__sync,
.find-audiobooks__note {
  font-size: 12px;
  margin: 0;
  color: var(--dim);
}
.find-audiobooks__current {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  min-width: 0;
  margin: var(--space-1) 0 0;
  font-size: 12px;
  font-weight: 600;
}
.find-audiobooks__candidates {
  display: grid;
  gap: var(--space-2);
  margin: var(--space-2) 0 0;
  padding: 0;
  border: 0;
  min-width: 0;
}
.find-audiobooks__candidate {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  min-width: 0;
  padding: var(--space-3) var(--space-5);
  border-radius: var(--radius-md);
  background: var(--ground);
  font-size: 12px;
  font-weight: 600;
  color: var(--dim);
  cursor: pointer;
}
.find-audiobooks__name {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.find-audiobooks__kind {
  flex: none;
}
.find-audiobooks__label {
  display: grid;
  min-width: 0;
  flex: 1;
}
.find-audiobooks__label .find-audiobooks__name {
  white-space: normal;
  overflow-wrap: anywhere;
  color: var(--ink);
}
.find-audiobooks__meta {
  font-size: 11px;
  font-weight: 600;
  color: var(--faint);
}
.find-audiobooks__eyebrow {
  justify-self: end;
  margin: 0 0 var(--space-1);
  font-size: 10.5px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--faint);
}
.find-audiobooks__eyebrow--exact {
  color: var(--ok);
}
.find-audiobooks__footer {
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
.find-audiobooks__unmatched {
  display: grid;
  gap: var(--space-4);
  margin-top: var(--space-14);
}
.find-audiobooks__heading {
  margin: 0;
  font-size: 15px;
  font-weight: 700;
}
.find-audiobooks__others {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: var(--space-4);
}
.find-audiobooks__other {
  display: flex;
  align-items: center;
  gap: var(--space-5);
  padding: var(--space-5) var(--space-6);
  border-radius: var(--radius-md);
  background: var(--page);
}
.find-audiobooks__other .find-audiobooks__label {
  flex: 1;
}
.find-audiobooks__empty {
  text-align: center;
  color: var(--dim);
  padding: var(--space-20) var(--space-8);
  display: grid;
  justify-items: center;
  gap: var(--space-6);
}
@media (max-width: 560px) {
  .find-audiobooks {
    padding: var(--space-4) var(--space-8) 0;
  }
  .find-audiobooks__grid {
    grid-template-columns: 1fr;
  }
  .find-audiobooks__footer {
    border-radius: var(--radius-xl);
  }
}
</style>
