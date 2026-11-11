<script setup lang="ts">
import { computed } from "vue";
import { TriangleAlert } from "@lucide/vue";
import type { BookView } from "../../api/types";
import CoverArt from "../CoverArt.vue";
import ChapterMenu from "./ChapterMenu.vue";

/** Left margin of the Sheet: cover plate, title, author, chapter and time left, a line for
 *  extraction or language warnings that opens the book's details, and one for a voice the
 *  engine does not offer that opens the Voice tab. */
const props = defineProps<{ book: BookView; coverUrl: string | null; chapter: number; playingChapter: number | null; timeLeft: string }>();
const emit = defineEmits<{ chapter: [index: number]; details: []; voice: []; about: [] }>();
const notes = computed(() => props.book.warnings.length + props.book.excluded.length);
</script>

<template>
  <aside class="book-margin" aria-label="About this book">
    <button type="button" class="book-margin__book" :aria-label="`Book page for ${book.title}`" title="Book page: details, contents and your listening" @click="emit('about')">
      <CoverArt class="book-margin__cover" :title="book.title" :url="coverUrl" />
      <b class="book-margin__title">{{ book.title }}</b>
      <span class="book-margin__author">{{ book.authors.join(", ") || "Unknown author" }}</span>
    </button>
    <hr class="book-margin__rule" />
    <ChapterMenu :chapters="book.chapters" :current="chapter" :playing="playingChapter" @pick="emit('chapter', $event)" />
    <span v-if="timeLeft" class="book-margin__time">{{ timeLeft }} in chapter</span>
    <button v-if="book.languageWarning || notes" type="button" class="book-margin__warning" :title="book.languageWarning ?? undefined" @click="emit('details')">
      <TriangleAlert :size="14" /> <span>{{ book.languageWarning ? 'Language not supported by this voice' : '' }}{{ book.languageWarning && notes ? ' · ' : '' }}{{ notes ? `${notes} extraction ${notes === 1 ? 'note' : 'notes'}` : '' }}</span>
    </button>
    <button v-if="book.voiceWarning" type="button" class="book-margin__warning book-margin__warning--voice" :title="book.voiceWarning" @click="emit('voice')">
      <TriangleAlert :size="14" /> <span>Voice not available, using another</span>
    </button>
  </aside>
</template>

<style scoped>
.book-margin { display: grid; gap: 8px; align-content: start; justify-items: start; font: 500 13px/1.4 var(--font-ui); color: var(--dim); }
.book-margin__book { display: grid; gap: 8px; justify-items: start; text-align: left; border-radius: 12px; }
.book-margin__book:hover .book-margin__title { text-decoration: underline; text-underline-offset: 3px; }
.book-margin__cover { width: 112px; font-size: 10px; border-radius: 10px; margin-bottom: 6px; }
.book-margin__title { font-size: 15px; font-weight: 800; line-height: 1.2; letter-spacing: -.01em; color: var(--ink); }
.book-margin__rule { width: 100%; height: 1px; border: 0; margin: 6px 0; background: var(--line); }
.book-margin__warning { display: inline-flex; align-items: flex-start; gap: 6px; margin-top: 6px; text-align: left; font-size: 12.5px; font-weight: 600; color: var(--warn); }
.book-margin__warning svg { margin-top: 2px; }
@media (max-width: 999px) {
  .book-margin { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 14px; }
  .book-margin__book, .book-margin__rule { display: none; }
  .book-margin__warning { margin-top: 0; }
}
</style>
