<script setup lang="ts">
import { computed } from "vue";
import { BookmarkCheck, BookmarkPlus } from "@lucide/vue";
import type { EntryView } from "@/api/types";
import { age } from "@/days";
import { formatDuration, readingMs } from "@/estimate";
import { NEEDS } from "@/articles";
import { useLibraryStore } from "@/stores/library";

/** One feed entry: its feed, age and length, headline and summary; opening reads it, the
 *  bookmark keeps it in the library. */
const props = defineProps<{ entry: EntryView; opening?: boolean }>();
defineEmits<{ open: []; save: [] }>();
const library = useLibraryStore();
/** After the feed's name: " · 2 hours ago · 9 min read · in your library". */
const meta = computed(() => {
  const e = props.entry;
  const length =
    e.hasFullText && e.words ? `${formatDuration(readingMs(e.words, library.readingWpm))} read` : "summary only";
  return [age(e.at), length, ...(e.inLibrary ? ["in your library"] : [])].map((part) => ` · ${part}`).join("");
});
</script>

<template>
  <article class="feed-entry" :class="{ 'feed-entry--read': entry.read }">
    <span v-if="!entry.read" class="feed-entry__dot" role="img" aria-label="Unread"></span>
    <span v-else class="feed-entry__dot feed-entry__dot--none" aria-hidden="true"></span>
    <button type="button" class="feed-entry__open" @click="$emit('open')">
      <span class="feed-entry__meta"
        ><b>{{ entry.feedTitle ?? "Feed" }}</b
        >{{ meta }}</span
      >
      <span class="feed-entry__title">{{ entry.title ?? entry.url ?? "Untitled" }}</span>
      <span v-if="entry.summary" class="feed-entry__summary">{{ entry.summary }}</span>
      <span v-if="opening" class="feed-entry__note" role="status">Fetching the article…</span>
      <span v-else-if="entry.needsYou" class="feed-entry__note feed-entry__note--needs"
        >Needs you · {{ NEEDS[entry.needsYou] }}</span
      >
      <span v-else-if="!entry.hasFullText && !entry.bookId" class="feed-entry__note"
        >The feed carries a summary. The full article is fetched when you open it.</span
      >
    </button>
    <button
      type="button"
      class="feed-entry__save"
      :class="{ 'feed-entry__save--on': entry.inLibrary }"
      :aria-label="entry.inLibrary ? 'Saved to library' : 'Save to library'"
      :aria-pressed="entry.inLibrary"
      :title="entry.inLibrary ? 'In your library (click to take it out)' : 'Save to library'"
      @click="$emit('save')"
    >
      <BookmarkCheck v-if="entry.inLibrary" :size="18" aria-hidden="true" />
      <BookmarkPlus v-else :size="18" aria-hidden="true" />
    </button>
  </article>
</template>

<style scoped>
.feed-entry {
  display: flex;
  align-items: flex-start;
  gap: var(--space-7);
  padding: var(--space-8) 0;
  border-bottom: 1px solid var(--line);
}
.feed-entry:last-child {
  border-bottom: 0;
}
.feed-entry__dot {
  flex: none;
  width: 8px;
  height: 8px;
  margin-top: var(--space-4);
  border-radius: 50%;
  background: var(--accent);
}
.feed-entry__dot--none {
  background: none;
}
.feed-entry__open {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  text-align: left;
  border-radius: var(--radius-sm);
}
.feed-entry__meta {
  font-size: 12px;
  color: var(--dim);
}
.feed-entry__meta b {
  font-weight: 700;
}
.feed-entry__title {
  font-family: var(--font-read);
  font-size: 21px;
  line-height: 1.2;
  font-weight: 500;
}
.feed-entry__open:hover .feed-entry__title {
  color: var(--accent);
}
.feed-entry--read .feed-entry__title {
  color: var(--dim);
}
.feed-entry__summary {
  font-family: var(--font-read);
  font-size: 15px;
  line-height: 1.45;
  color: var(--dim);
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.feed-entry__note {
  font-size: 12px;
  color: var(--dim);
}
.feed-entry__note--needs {
  color: var(--warn);
  font-weight: 600;
}
.feed-entry__save {
  flex: none;
  display: grid;
  place-items: center;
  width: 44px;
  height: 44px;
  border-radius: 50%;
  background: var(--wash);
}
.feed-entry__save:hover {
  color: var(--accent);
}
.feed-entry__save--on,
.feed-entry__save--on:hover {
  background: var(--ink);
  color: var(--ground);
}
</style>
