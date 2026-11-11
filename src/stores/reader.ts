import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api } from "../api";
import type { BookView, ChapterView } from "../api/types";
import { useNavigationStore } from "./navigation";
import { usePlayerStore } from "./player";

export const useReaderStore = defineStore("reader", () => {
  const book = ref<BookView | null>(null), cache = ref<Record<string, ChapterView>>({}), currentChapter = ref(0), error = ref(""), loading = ref(false);
  let openRequest = 0, chapterRequest = 0;
  const chapter = computed(() => book.value ? cache.value[`${book.value.id}:${currentChapter.value}`] : undefined);
  /** A chapter's text, fetched once; does not change which chapter is shown. */
  async function chapterText(id: string, index: number) { const key = `${id}:${index}`; return cache.value[key] ??= await api.chapter(id, index); }
  async function loadChapter(index: number) {
    const id = book.value?.id; if (!id) return;
    const request = ++chapterRequest;
    try { await chapterText(id, index); if (book.value?.id === id && request === chapterRequest) currentChapter.value = index; }
    catch (e) { error.value = String(e); }
  }
  async function open(id: string) {
    const request = ++openRequest; loading.value = true; error.value = "";
    try { const result = await api.openBook(id); if (request !== openRequest) return; book.value = result; await loadChapter(result.progress?.chapter ?? 0); useNavigationStore().view = "reader"; }
    catch (e) { error.value = String(e); } finally { if (request === openRequest) loading.value = false; }
  }
  /** Shows a chapter of the open book and plays it from its first spoken sentence. */
  async function startChapter(index: number) {
    await loadChapter(index);
    const first = chapter.value?.blocks.flatMap(b => b.segments).find(s => s.speakable);
    if (first && book.value && currentChapter.value === index) await usePlayerStore().play(book.value.id, index, first.id);
  }
  return { book, cache, currentChapter, chapter, error, loading, open, chapterText, loadChapter, startChapter };
});
