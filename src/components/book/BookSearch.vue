<script setup lang="ts">
import { ref } from "vue";
import { api } from "../../api";
import type { SearchHit } from "../../api/types";
import { usePlayerStore } from "../../stores/player";
const props = defineProps<{ bookId: string }>();
const player = usePlayerStore();
const query = ref(""), hits = ref<SearchHit[] | null>(null), error = ref(""), searching = ref(false);
let request = 0;
async function search() {
  const q = query.value.trim(); error.value = "";
  if (q.length < 2) { hits.value = null; return; }
  const id = ++request; searching.value = true;
  try { const result = await api.searchBook(props.bookId, q); if (id === request) hits.value = result; }
  catch (e) { error.value = String(e); } finally { if (id === request) searching.value = false; }
}
function parts(hit: SearchHit) {
  const chars = Array.from(hit.snippet);
  return [chars.slice(0, hit.matchStart).join(""), chars.slice(hit.matchStart, hit.matchEnd).join(""), chars.slice(hit.matchEnd).join("")];
}
</script>

<template>
  <form class="book-search__form" role="search" @submit.prevent="search">
    <input v-model="query" type="search" aria-label="Search in book" placeholder="Search this book…" />
    <button type="submit" class="book-search__button" :disabled="searching">Search</button>
  </form>
  <p v-if="error" class="book-search__error" role="alert">{{ error }}</p>
  <p v-if="hits && !hits.length" class="book-search__empty">No matches.</p>
  <p v-if="hits && hits.length >= 200" class="book-search__empty">Showing the first 200 matches.</p>
  <ul v-if="hits?.length" class="book-search__list">
    <li v-for="hit in hits" :key="`${hit.chapter}:${hit.segmentId}:${hit.matchStart}`">
      <button type="button" class="book-search__hit" @click="player.play(bookId, hit.chapter, hit.segmentId)">
        <small>{{ hit.chapterTitle }}</small>
        <span>{{ parts(hit)[0] }}<mark>{{ parts(hit)[1] }}</mark>{{ parts(hit)[2] }}</span>
      </button>
    </li>
  </ul>
</template>

<style scoped>
.book-search__form { display: flex; gap: 8px; margin-bottom: 14px; }
.book-search__form input { flex: 1; }
.book-search__button { display: inline-flex; align-items: center; gap: 6px; border-radius: 999px; background: var(--ground); font-size: 12.5px; font-weight: 700; padding: 8px 14px; white-space: nowrap; }
.book-search__list { list-style: none; padding: 0; margin: 0; display: grid; gap: 4px; }
.book-search__hit { display: grid; gap: 4px; text-align: left; width: 100%; padding: 10px; border-radius: 8px; font: 14px/1.55 var(--font-read); }
.book-search__hit:hover { background: var(--wash); }
.book-search__hit small { font: 11px var(--font-ui); color: var(--dim); }
.book-search__hit mark { background: var(--wash); color: inherit; border-radius: 3px; padding: 0 2px; }
.book-search__empty { font-size: 12px; color: var(--dim); }
.book-search__error { font-size: 12px; color: var(--accent); }
</style>
