<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { api } from "@/api";
import type { LibraryHits, PassageHitView } from "@/api/types";
import { useLibraryStore } from "@/stores/library";
import { useNavigationStore } from "@/stores/navigation";
import { useReaderStore } from "@/stores/reader";

/** The library's search in the items' text (core/indexing.rs): under the title matches, each
 *  item with its best passages, the words found marked. A passage opens the reader at its
 *  sentence, without playing. Shown only while the index is on (Settings › Library). */
const library = useLibraryStore(),
  reader = useReaderStore(),
  navigation = useNavigationStore();
const on = ref(false);
const found = ref<LibraryHits[] | null>(null),
  error = ref("");
let timer: ReturnType<typeof setTimeout> | undefined,
  request = 0;
const readSetting = () =>
  api
    .appSettings()
    .then((s) => (on.value = s.searchText))
    .catch(() => {});
onMounted(readSetting);
watch(
  () => navigation.settingsOpen,
  (open) => !open && void readSetting(),
);
onUnmounted(() => clearTimeout(timer));
/** Asked a moment after typing stops, so each key does not search. */
watch(
  () => [library.query.trim(), on.value] as const,
  ([query, enabled]) => {
    clearTimeout(timer);
    const asked = ++request;
    if (!query || !enabled) {
      found.value = null;
      return;
    }
    timer = setTimeout(async () => {
      try {
        const hits = await api.searchLibrary({ query });
        if (asked === request) {
          found.value = hits;
          error.value = "";
        }
      } catch (e) {
        if (asked === request) error.value = String(e);
      }
    }, 250);
  },
  { immediate: true },
);
const items = computed(() =>
  (found.value ?? []).map((item) => ({
    ...item,
    book: library.books.find((b) => b.id === item.bookId) ?? null,
  })),
);
/** A snippet in runs, marked or not (marks are character ranges, so split by code points). */
function runs(hit: PassageHitView) {
  const chars = Array.from(hit.snippet);
  const out: { text: string; marked: boolean }[] = [];
  let at = 0;
  for (const [from, to] of hit.marks) {
    if (from > at) out.push({ text: chars.slice(at, from).join(""), marked: false });
    out.push({ text: chars.slice(from, to).join(""), marked: true });
    at = to;
  }
  if (at < chars.length) out.push({ text: chars.slice(at).join(""), marked: false });
  return out;
}
const open = (bookId: string, hit: PassageHitView) =>
  reader.openAt(bookId, hit).catch((e) => (error.value = String(e)));
</script>

<template>
  <section v-if="on && found" class="text-search" aria-label="In the text">
    <h2 class="text-search__title">In the text</h2>
    <p v-if="error" class="text-search__note" role="alert">{{ error }}</p>
    <p v-else-if="!items.length" class="text-search__note">Nothing in the text has all of these words.</p>
    <ul class="text-search__items">
      <li v-for="item in items" :key="item.bookId" class="text-search__item">
        <div class="text-search__head">
          <b class="text-search__name">{{ item.book?.title ?? "An item" }}</b>
          <span v-if="item.book?.authors.length" class="text-search__by">{{ item.book.authors.join(", ") }}</span>
        </div>
        <ul class="text-search__passages">
          <li v-for="hit in item.hits" :key="hit.segmentId">
            <button type="button" class="text-search__passage" @click="open(item.bookId, hit)">
              <template v-for="(run, r) in runs(hit)" :key="r"
                ><mark v-if="run.marked" class="text-search__mark">{{ run.text }}</mark
                ><template v-else>{{ run.text }}</template></template
              >
            </button>
          </li>
        </ul>
        <span v-if="item.more" class="text-search__more"
          >and {{ item.more }} more {{ item.more === 1 ? "passage" : "passages" }}</span
        >
      </li>
    </ul>
  </section>
</template>

<style scoped>
.text-search {
  display: grid;
  gap: var(--space-6);
}
.text-search__title {
  font-size: 15px;
  font-weight: 750;
}
.text-search__note {
  color: var(--dim);
  font-size: 13px;
}
.text-search__items {
  display: grid;
  gap: var(--space-8);
  margin: 0;
  padding: 0;
  list-style: none;
}
.text-search__item {
  display: grid;
  gap: var(--space-3);
}
.text-search__head {
  display: flex;
  align-items: baseline;
  gap: var(--space-4);
  min-width: 0;
}
.text-search__name {
  font-size: 14px;
  font-weight: 700;
}
.text-search__by {
  color: var(--dim);
  font-size: 12.5px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.text-search__passages {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}
.text-search__passage {
  display: block;
  width: 100%;
  padding: var(--space-3) var(--space-5);
  border-radius: var(--radius-md);
  color: var(--ink);
  font-family: var(--font-reading, inherit);
  font-size: 14px;
  line-height: 1.5;
  text-align: left;
}
.text-search__passage:hover,
.text-search__passage:focus-visible {
  background: var(--wash);
}
.text-search__mark {
  background: none;
  color: inherit;
  font-weight: 700;
  box-shadow: inset 0 -0.18em 0 var(--accent);
}
.text-search__more {
  padding: 0 var(--space-5);
  color: var(--dim);
  font-size: 12px;
}
</style>
