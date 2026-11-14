<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { Bookmark, Info, Search, SlidersHorizontal, X } from "@lucide/vue";
import BookSearch from "./BookSearch.vue";
import BookBookmarks from "./BookBookmarks.vue";
import BookNarration from "./BookNarration.vue";
import BookInfo from "./BookInfo.vue";
import UiDialog from "../ui/UiDialog.vue";
import { useReaderStore } from "../../stores/reader";
import { useNavigationStore, type ToolsTab } from "../../stores/navigation";

export type { ToolsTab };
/** Book tools as a modal bottom sheet: Bookmarks, Search, Narration (the recording, or the
 *  speech engine's voice with audio, preparation and export) and Details (with extraction notes). Escape or the backdrop closes it; focus
 *  returns to the opener, or to the dock's tools button when WebKit had not focused it. */
const props = withDefaults(defineProps<{ initialTab?: ToolsTab }>(), { initialTab: "bookmarks" });
const emit = defineEmits<{ close: [] }>();
const reader = useReaderStore(), navigation = useNavigationStore();
const tab = ref<ToolsTab>(props.initialTab);
const tabList = ref<HTMLElement>(), body = ref<HTMLElement>();
const SEARCH_FIELD = 'input[type="search"]';
// Ctrl F: opens on the search field, or moves to it when the sheet is already open.
const initialFocus = navigation.findRequest ? SEARCH_FIELD : '[role="tab"][aria-selected="true"]';
watch(() => navigation.findRequest, async request => {
  if (!request) return;
  tab.value = "search";
  await nextTick();
  body.value?.querySelector<HTMLElement>(SEARCH_FIELD)?.focus();
});
const tabs = [
  { id: "bookmarks", label: "Bookmarks", icon: Bookmark },
  { id: "search", label: "Search", icon: Search },
  { id: "voice", label: "Narration", icon: SlidersHorizontal },
  { id: "details", label: "Details", icon: Info },
] as const;
async function focusTab() { await nextTick(); tabList.value?.querySelector<HTMLElement>('[aria-selected="true"]')?.focus(); }
watch(tab, () => { if (tabList.value?.contains(document.activeElement)) void focusTab(); });
function moveTab(delta: number) {
  const at = tabs.findIndex(t => t.id === tab.value);
  tab.value = tabs[(at + delta + tabs.length) % tabs.length].id;
  void focusTab();
}
</script>

<template>
  <UiDialog v-if="reader.book" label="Book tools" variant="sheet" :initial-focus="initialFocus" return-focus='[aria-label="Player"] [aria-label="Book tools"]' @close="emit('close')">
    <div class="book-tools">
      <header class="book-tools__head">
        <div ref="tabList" class="book-tools__tabs" role="tablist" aria-label="Book tools">
          <button v-for="t in tabs" :id="`book-tools-tab-${t.id}`" :key="t.id" type="button" role="tab" class="book-tools__tab" :class="{ 'book-tools__tab--on': tab === t.id }" :aria-selected="tab === t.id" :aria-controls="`book-tools-${t.id}`" :tabindex="tab === t.id ? 0 : -1" @click="tab = t.id" @keydown.right.prevent="moveTab(1)" @keydown.left.prevent="moveTab(-1)"><component :is="t.icon" :size="15" aria-hidden="true" /><span>{{ t.label }}</span></button>
        </div>
        <span class="book-tools__book" :title="reader.book.title">{{ reader.book.title }}</span>
        <button type="button" class="book-tools__close" aria-label="Close book tools" @click="emit('close')"><X :size="17" /></button>
      </header>
      <div :id="`book-tools-${tab}`" ref="body" class="book-tools__body" role="tabpanel" :aria-labelledby="`book-tools-tab-${tab}`">
        <BookBookmarks v-if="tab === 'bookmarks'" :key="`b:${reader.book.id}`" :book-id="reader.book.id" />
        <BookSearch v-else-if="tab === 'search'" :key="`s:${reader.book.id}`" :book-id="reader.book.id" />
        <BookNarration v-else-if="tab === 'voice'" :key="`v:${reader.book.id}`" :book-id="reader.book.id" />
        <BookInfo v-else :key="`d:${reader.book.id}`" :book-id="reader.book.id" :notes-open="props.initialTab === 'details'" />
      </div>
    </div>
  </UiDialog>
</template>

<style scoped>
.book-tools { width: min(780px, 100vw); height: min(72vh, 680px); display: grid; grid-template-rows: auto minmax(0, 1fr); grid-template-columns: minmax(0, 1fr); }
.book-tools__head { display: flex; align-items: center; gap: 12px; padding: 14px 14px 12px 20px; border-bottom: 1px solid var(--line); }
.book-tools__tabs { flex-shrink: 0; display: flex; gap: 2px; padding: 3px; border-radius: 999px; background: var(--ground); }
.book-tools__tab { display: inline-flex; align-items: center; gap: 6px; padding: 7px 14px; border-radius: 999px; font-size: 13px; font-weight: 700; color: var(--dim); }
.book-tools__tab--on { background: var(--ink); color: var(--ground); }
.book-tools__book { flex: 1; min-width: 0; font-size: 13px; font-weight: 600; color: var(--dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; text-align: right; }
.book-tools__close { display: grid; place-items: center; width: 36px; height: 36px; border-radius: 50%; background: var(--glass); flex-shrink: 0; }
.book-tools__body { overflow-y: auto; padding: 20px 24px 28px; }
@media (max-width: 600px) {
  .book-tools { height: 86vh; }
  .book-tools__book { display: none; }
  .book-tools__tabs { flex: 1; }
  .book-tools__tab { flex: 1; justify-content: center; padding: 8px; }
  .book-tools__tab span { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); }
  .book-tools__body { padding: 16px; }
}
</style>
