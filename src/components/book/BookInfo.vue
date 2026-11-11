<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api } from "../../api";
import type { BookDetails } from "../../api/types";
import MetadataLookup from "./MetadataLookup.vue";
import { useReaderStore } from "../../stores/reader";
const props = withDefaults(defineProps<{ bookId: string; notesOpen?: boolean }>(), { notesOpen: false });
const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
const reader = useReaderStore();
const details = ref<BookDetails | null>(null), error = ref("");
onMounted(async () => { try { details.value = await api.bookDetails(props.bookId); } catch (e) { error.value = String(e); } });
</script>

<template>
  <p v-if="error" class="book-info__error" role="alert">{{ error }}</p>
  <dl v-if="details" class="book-info__details">
    <template v-if="reader.book?.authors.length"><dt>Author</dt><dd>{{ reader.book.authors.join(', ') }}</dd></template>
    <template v-if="reader.book?.language"><dt>Language</dt><dd>{{ reader.book.language }}</dd></template>
    <template v-if="details.publisher"><dt>Publisher</dt><dd>{{ details.publisher }}</dd></template>
    <template v-if="details.published"><dt>Published</dt><dd>{{ details.published }}</dd></template>
    <template v-if="details.isbn"><dt>ISBN</dt><dd>{{ details.isbn }}</dd></template>
    <template v-if="details.subjects.length"><dt>Subjects</dt><dd>{{ details.subjects.slice(0, 8).join(', ') }}</dd></template>
  </dl>
  <p v-if="details?.description" class="book-info__description">{{ details.description }}</p>
  <MetadataLookup :book-id="bookId" :matched="Boolean(details?.openLibraryKey)" @applied="details = $event" />
  <details v-if="reader.book && (reader.book.warnings.length || reader.book.excluded.length)" class="book-info__extraction" :open="notesOpen">
    <summary>Extraction notes · {{ plural(reader.book.warnings.length, "warning", "warnings") }}, {{ plural(reader.book.excluded.length, "section", "sections") }} left out</summary>
    <p v-for="section in reader.book.excluded" :key="section.title"><strong>{{ section.title }}</strong>: {{ section.reason }}</p>
    <p v-for="warning in reader.book.warnings" :key="warning">{{ warning }}</p>
  </details>
</template>

<style scoped>
.book-info__details { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 8px 14px; font-size: 13px; margin: 0 0 14px; }
.book-info__details dt { color: var(--dim); }
.book-info__details dd { margin: 0; overflow-wrap: anywhere; }
.book-info__description { font: 14px/1.6 var(--font-read); color: var(--dim); white-space: pre-line; max-height: 280px; overflow-y: auto; }
.book-info__extraction { margin-top: 22px; font-size: 12px; color: var(--dim); }
.book-info__extraction summary { cursor: pointer; }
.book-info__error { font-size: 12px; color: var(--accent); overflow-wrap: anywhere; }
</style>
