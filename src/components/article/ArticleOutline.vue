<script setup lang="ts">
import { computed } from "vue";
import { ChevronDown } from "@lucide/vue";
import type { ChapterView } from "@/api/types";
import UiMenu from "@/components/ui/UiMenu.vue";
import type { MenuEntry } from "@/components/ui/types";

/** An article's headings, in the reader's header where a book has its chapters: picking one
 *  scrolls the page to it. Only the title shows when the article has no headings. */
const props = defineProps<{ title: string; chapter?: ChapterView | null }>();
const headings = computed(() =>
  (props.chapter?.blocks ?? [])
    .filter((block) => block.kind === "heading" && block.segments.length)
    .map((block) => ({
      id: block.segments[0].id,
      label: block.segments.map((segment) => segment.words.join(" ")).join(" "),
    })),
);
function jump(id: string) {
  document.querySelector(`[data-segment-id="${id}"]`)?.scrollIntoView({ block: "start", behavior: "smooth" });
}
const items = computed<MenuEntry[]>(() =>
  headings.value.map((heading) => ({ id: heading.id, label: heading.label, action: () => jump(heading.id) })),
);
</script>

<template>
  <UiMenu v-if="items.length" class="article-outline" label="Outline" :items="items" :width="300">
    <template #default="{ toggle, trigger }">
      <button type="button" class="article-outline__button" v-bind="trigger" aria-label="Outline" @click="toggle">
        <span class="article-outline__title">{{ title }}</span>
        <ChevronDown :size="14" aria-hidden="true" />
      </button>
    </template>
  </UiMenu>
  <span v-else class="article-outline__title article-outline__title--plain">{{ title }}</span>
</template>

<style scoped>
.article-outline {
  min-width: 0;
  max-width: 100%;
}
.article-outline__button {
  display: inline-flex;
  align-items: center;
  gap: var(--space-4);
  max-width: 100%;
  height: 44px;
  padding: 0 var(--space-8);
  border-radius: var(--radius-xl);
  font-weight: 600;
}
.article-outline__button:hover {
  background: var(--glass);
}
.article-outline__title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.article-outline__title--plain {
  min-width: 0;
  padding: 0 var(--space-8);
  font-weight: 600;
}
</style>
