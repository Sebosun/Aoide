<script setup lang="ts">
import { ref } from "vue";
import { Globe } from "@lucide/vue";
import { api } from "../../api";
import type { BookDetails, MetadataMatch } from "../../api/types";
import { useLibraryStore } from "../../stores/library";

/** "Find details online": searches Open Library and applies the match the person picks. */
const props = defineProps<{ bookId: string; matched: boolean }>();
const emit = defineEmits<{ applied: [details: BookDetails] }>();
const library = useLibraryStore();
const matches = ref<MetadataMatch[] | null>(null), error = ref(""), searching = ref(false), applying = ref<string | null>(null);
async function lookup(refresh = false) {
  error.value = ""; searching.value = true;
  try { matches.value = await api.lookupMetadata(props.bookId, refresh); } catch (e) { error.value = String(e); } finally { searching.value = false; }
}
async function apply(match: MetadataMatch) {
  error.value = ""; applying.value = match.key;
  try { const details = await api.applyMetadata(props.bookId, match); matches.value = null; emit("applied", details); await library.refresh(); }
  catch (e) { error.value = String(e); } finally { applying.value = null; }
}
</script>

<template>
  <div class="lookup">
    <p v-if="error" class="lookup__error" role="alert">{{ error }}</p>
    <button type="button" class="lookup__button" aria-label="Find details online" :disabled="searching" @click="lookup()"><Globe :size="14" /> {{ searching ? 'Searching…' : matched ? 'Pick a different match' : 'Find details online' }}</button>
    <p class="lookup__hint">Searches Open Library by ISBN, then title and author. Nothing changes until you choose a match.</p>
    <p v-if="matches && !matches.length" class="lookup__hint">No matches found.</p>
    <button v-if="matches" type="button" class="lookup__link" aria-label="Search again" :disabled="searching" @click="lookup(true)">Not right? Search Open Library again</button>
    <ul v-if="matches?.length" class="lookup__matches">
      <li v-for="match in matches" :key="match.key" class="lookup__match">
        <img v-if="match.coverUrl" :src="match.coverUrl" alt="" loading="lazy" referrerpolicy="no-referrer" draggable="false" />
        <div><strong>{{ match.title }}</strong><span>{{ match.authors.join(', ') }}</span><span>{{ [match.published, match.publisher].filter(Boolean).join(' · ') }}</span></div>
        <button type="button" class="lookup__button" :disabled="applying !== null" @click="apply(match)">Use this</button>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.lookup { display: grid; gap: 6px; justify-items: start; }
.lookup__button { display: inline-flex; align-items: center; gap: 6px; border-radius: 999px; background: var(--ground); font-size: 12.5px; font-weight: 700; padding: 8px 14px; white-space: nowrap; }
.lookup__link { font-size: 11px; color: var(--accent); text-decoration: underline; padding: 4px 0; }
.lookup__hint { font-size: 11px; color: var(--dim); }
.lookup__matches { justify-self: stretch; list-style: none; padding: 0; margin: 0; display: grid; gap: 10px; }
.lookup__match { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; gap: 10px; align-items: center; padding: 8px; border: 1px solid var(--line); border-radius: 8px; }
.lookup__match img { width: 40px; height: 60px; object-fit: cover; border-radius: 3px; }
.lookup__match:not(:has(img)) { grid-template-columns: minmax(0, 1fr) auto; }
.lookup__match div { display: grid; gap: 2px; font-size: 12px; min-width: 0; }
.lookup__match span { color: var(--dim); }
.lookup__error { font-size: 12px; color: var(--accent); overflow-wrap: anywhere; }
</style>
