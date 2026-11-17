<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { CircleCheck } from "@lucide/vue";
import { api } from "@/api";
import type { SeriesShelfView } from "@/api/contract";
import { useLibraryStore } from "@/stores/library";
import { useNavigationStore } from "@/stores/navigation";

/** An item's place in its series, with the series' other books in the library, in order. The
 *  core decides what is in the series; this shows it. Nothing shows for an item in none. */
const props = defineProps<{ bookId: string }>();
const library = useLibraryStore(),
  navigation = useNavigationStore();
const shelf = ref<SeriesShelfView | null>(null);
// Asked again whenever the library's series change (an edit, a scan).
const seriesNow = computed(() => JSON.stringify(library.books.map((b) => [b.id, b.series])));
watch(
  seriesNow,
  async () => {
    shelf.value = await api.series({ bookId: props.bookId }).catch(() => null);
  },
  { immediate: true },
);
const place = computed(() => shelf.value?.items.find((i) => i.bookId === props.bookId)?.position ?? null);
const titleOf = (id: string) => library.books.find((b) => b.id === id)?.title ?? "";
</script>

<template>
  <nav v-if="shelf" class="series-strip" :aria-label="`Series: ${shelf.name}`">
    <p class="series-strip__where">
      <template v-if="place !== null">Book {{ place }} of </template><template v-else>In </template
      ><em>{{ shelf.name }}</em>
    </p>
    <ol v-if="shelf.items.length > 1" class="series-strip__list">
      <li v-for="item in shelf.items" :key="item.bookId">
        <button
          type="button"
          class="series-strip__item"
          :class="{ 'series-strip__item--here': item.bookId === bookId }"
          :aria-current="item.bookId === bookId ? 'page' : undefined"
          :title="item.state === 'finished' ? `${titleOf(item.bookId)} (finished)` : titleOf(item.bookId)"
          @click="item.bookId !== bookId && navigation.openItem(item.bookId)"
        >
          <span class="series-strip__place">{{ item.position ?? "" }}</span><span class="series-strip__title">{{
            titleOf(item.bookId)
          }}</span
          ><CircleCheck v-if="item.state === 'finished'" class="series-strip__done" :size="13" aria-hidden="true" />
        </button>
      </li>
    </ol>
  </nav>
</template>

<style scoped>
.series-strip {
  display: grid;
  gap: var(--space-3);
}
.series-strip__where {
  margin: 0;
  font-size: 13px;
  color: var(--dim);
}
.series-strip__where em {
  font-style: italic;
  color: var(--ink);
}
.series-strip__list {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}
.series-strip__item {
  display: inline-flex;
  align-items: center;
  gap: var(--space-3);
  max-width: 18em;
  padding: var(--space-2) var(--space-5) var(--space-2) var(--space-2);
  border-radius: var(--radius-pill);
  background: var(--glass);
  font-size: 12px;
  color: var(--dim);
}
.series-strip__item:hover {
  color: var(--ink);
}
.series-strip__item--here {
  background: var(--ink);
  color: var(--page);
  cursor: default;
}
.series-strip__item--here:hover {
  color: var(--page);
}
.series-strip__place {
  display: grid;
  place-items: center;
  min-width: var(--space-9);
  height: var(--space-9);
  padding: 0 var(--space-2);
  border-radius: var(--radius-pill);
  background: color-mix(in srgb, currentColor 14%, transparent);
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}
.series-strip__title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.series-strip__done {
  flex: none;
  color: var(--accent);
}
.series-strip__item--here .series-strip__done {
  color: inherit;
}
</style>
