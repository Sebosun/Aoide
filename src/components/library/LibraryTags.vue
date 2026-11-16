<script setup lang="ts">
import { computed, ref } from "vue";
import { Ban, Check } from "@lucide/vue";
import { useLibraryStore } from "@/stores/library";

/** The library's tags as chips, the most used first: a click shows only the items with the
 *  tag, a second leaves them out, a third clears it. The filter is saved, so what is left out
 *  stays out. A dozen are shown until "More" is pressed; those in the filter always are. */
const library = useLibraryStore();
const SHOWN = 12;
const all = ref(false);
const tags = computed(() => {
  const list = library.tagCounts;
  if (all.value || list.length <= SHOWN) return list;
  return list.filter((t, i) => i < SHOWN || library.tagState(t.name) !== "off");
});
const hidden = computed(() => library.tagCounts.length - tags.value.length);
const SAYS = { include: "only these are shown", exclude: "left out", off: "not filtered" } as const;
</script>

<template>
  <div v-if="library.tagCounts.length" class="library-tags" role="group" aria-label="Filter by tag">
    <button
      v-for="t in tags"
      :key="t.name"
      type="button"
      class="library-tags__tag"
      :class="`library-tags__tag--${library.tagState(t.name)}`"
      :aria-label="`${t.name}: ${SAYS[library.tagState(t.name)]}`"
      title="Show only these, then leave them out, then neither"
      @click="library.cycleTag(t.name)"
    >
      <Check v-if="library.tagState(t.name) === 'include'" :size="13" aria-hidden="true" />
      <Ban v-else-if="library.tagState(t.name) === 'exclude'" :size="13" aria-hidden="true" />
      {{ t.name }} <span class="library-tags__count">{{ t.books }}</span>
    </button>
    <button v-if="hidden > 0 || all" type="button" class="library-tags__more" @click="all = !all">
      {{ all ? "Fewer" : `More (${hidden})` }}
    </button>
  </div>
</template>

<style scoped>
.library-tags {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
}
.library-tags__tag,
.library-tags__more {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  height: var(--control-sm);
  padding: 0 var(--space-5);
  border-radius: var(--radius-pill);
  background: var(--wash);
  color: var(--dim);
  font-size: 12.5px;
  font-weight: 500;
}
.library-tags__tag:hover,
.library-tags__more:hover {
  color: var(--ink);
}
.library-tags__tag--include {
  background: var(--ink);
  color: var(--ground);
  font-weight: 600;
}
.library-tags__tag--include:hover {
  color: var(--ground);
}
.library-tags__tag--exclude {
  background: transparent;
  box-shadow: inset 0 0 0 1px var(--line);
  color: var(--faint);
  text-decoration: line-through;
}
.library-tags__count {
  font-variant-numeric: tabular-nums;
  opacity: 0.7;
}
.library-tags__more {
  background: transparent;
}
</style>
