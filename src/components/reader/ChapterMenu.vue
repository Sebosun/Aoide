<script setup lang="ts">
import { computed } from "vue";
import { AudioLines, ChevronDown, List, Sparkles } from "@lucide/vue";
import type { ChapterSummary } from "../../api/types";
import UiMenu from "../ui/UiMenu.vue";
import type { MenuEntry } from "../ui/types";

/** `link`: "Chapter 4 of 24 ▾" (Sheet margin); `pill`: a "Chapters" button (library hero);
 *  `chip`: "Chapter 4 · Title ▾" (Lines mode). With `prepare`, the list ends in "Prepare this
 *  chapter" for the current chapter. */
const props = withDefaults(defineProps<{ chapters: ChapterSummary[]; current: number; playing: number | null; variant?: "link" | "pill" | "chip"; prepare?: boolean }>(), { variant: "link", prepare: false });
const emit = defineEmits<{ pick: [index: number]; prepare: [index: number] }>();
const items = computed<MenuEntry[]>(() => {
  const list: MenuEntry[] = props.chapters.map((chapter, i) => ({ id: chapter.index, label: chapter.title, ariaLabel: `Chapter ${i + 1}: ${chapter.title}`, current: chapter.index === props.current, action: () => emit("pick", chapter.index) }));
  if (props.prepare) list.push({ kind: "separator" }, { id: "prepare", label: "Prepare this chapter", icon: Sparkles, action: () => emit("prepare", props.current) });
  return list;
});
const position = () => `Chapter ${props.chapters.findIndex(c => c.index === props.current) + 1} of ${props.chapters.length}`;
</script>

<template>
  <UiMenu class="chapter-menu" label="Chapters" :items="items" :width="300">
    <template #default="{ toggle, trigger }">
      <button v-if="variant === 'pill'" type="button" class="chapter-menu__pill" v-bind="trigger" aria-label="Chapters" @click="toggle"><List :size="17" /> Chapters</button>
      <button v-else-if="variant === 'chip'" type="button" class="chapter-menu__chip" v-bind="trigger" :aria-label="`${position()}, choose a chapter`" @click="toggle"><span class="chapter-menu__chip-text">Chapter {{ chapters.findIndex(c => c.index === current) + 1 }} · {{ chapters.find(c => c.index === current)?.title }}</span> <ChevronDown :size="15" /></button>
      <button v-else type="button" class="chapter-menu__button" v-bind="trigger" :aria-label="`${position()}, choose a chapter`" @click="toggle">{{ position() }} <ChevronDown :size="14" /></button>
    </template>
    <template #item="{ item, index }">
      <template v-if="item.id === 'prepare'"><Sparkles :size="15" class="chapter-menu__number" aria-hidden="true" /><span class="chapter-menu__title">{{ item.label }}</span></template>
      <template v-else><span class="chapter-menu__number">{{ index + 1 }}</span><span class="chapter-menu__title">{{ item.label }}</span><AudioLines v-if="item.id === playing" :size="15" class="chapter-menu__playing" aria-hidden="true" /></template>
    </template>
  </UiMenu>
</template>

<style scoped>
.chapter-menu { min-width: 0; max-width: 100%; }
.chapter-menu__button { display: inline-flex; align-items: center; gap: 6px; padding: 2px 0; font-size: 13px; font-weight: 600; color: var(--dim); text-align: left; }
.chapter-menu__button:hover { color: var(--ink); }
.chapter-menu__pill { display: inline-flex; align-items: center; gap: 8px; height: 46px; padding: 0 20px; border-radius: 999px; background: var(--glass); font-weight: 700; font-size: 14px; }
.chapter-menu__chip { display: inline-flex; align-items: center; gap: 8px; max-width: 100%; padding: 8px 14px; border-radius: 999px; background: var(--glass); font-weight: 700; font-size: 13px; }
.chapter-menu__chip-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.chapter-menu__number { flex: 0 0 22px; color: var(--faint); font-variant-numeric: tabular-nums; }
.chapter-menu__title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.chapter-menu__playing { color: var(--accent); }
</style>
