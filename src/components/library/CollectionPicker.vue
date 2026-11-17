<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Check, FolderPlus, X } from "@lucide/vue";
import UiButton from "@/components/ui/UiButton.vue";
import UiDialog from "@/components/ui/UiDialog.vue";
import type { Collection } from "@/api/types";
import { useLibraryStore } from "@/stores/library";
import { useUiStore } from "@/stores/ui";

/** Puts an item on a collection, or takes it off, in a few keys: the letters typed narrow the
 *  collections, and the last row creates one named so. Enter chooses the row in focus (the first
 *  one left, unless the arrows moved) and closes; a click chooses a row and stays, to choose
 *  several. Each choice is saved at once. */
const props = defineProps<{ bookId: string }>();
const emit = defineEmits<{ close: [] }>();
const library = useLibraryStore(),
  ui = useUiStore();
const book = computed(() => library.books.find((b) => b.id === props.bookId) ?? null);
const title = computed(() => book.value?.title ?? "");
const typed = ref("");
const name = computed(() => typed.value.trim());
const rows = computed(() => library.collections.filter((c) => c.name.toLowerCase().includes(name.value.toLowerCase())));
/** The name typed, when no collection has it (ignoring case): offered last, to create. */
const creating = computed(() =>
  name.value && !library.collections.some((c) => c.name.toLowerCase() === name.value.toLowerCase()) ? name.value : null,
);
const count = computed(() => rows.value.length + (creating.value ? 1 : 0));
/** The row Enter chooses: the first once something is typed; none before, until an arrow moves. */
const active = ref(-1);
watch(name, (text) => (active.value = text ? 0 : -1));
const on = (collection: Collection) => book.value?.collections.includes(collection.id) ?? false;

async function save(change: () => Promise<string>, close: boolean) {
  try {
    const said = await change();
    if (close) {
      emit("close");
      ui.toast(said);
    }
  } catch (e) {
    ui.toast(String(e), "error");
  }
}
function toggle(collection: Collection, close = false) {
  const was = book.value?.collections ?? [];
  const ids = on(collection) ? was.filter((id) => id !== collection.id) : [...was, collection.id];
  const said = on(collection) ? `Taken out of ${collection.name}` : `Added to ${collection.name}`;
  return save(async () => {
    await library.setBookCollections(props.bookId, ids);
    return said;
  }, close);
}
function create(close = false) {
  const named = creating.value;
  if (!named) return;
  return save(async () => {
    const id = await library.createCollection(named);
    await library.setBookCollections(props.bookId, [...(book.value?.collections ?? []), id]);
    typed.value = "";
    return `Added to ${named}`;
  }, close);
}
function choose(index: number, close: boolean) {
  const collection = rows.value[index];
  if (collection) void toggle(collection, close);
  else if (index === rows.value.length) void create(close);
}
function onKeydown(event: KeyboardEvent) {
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    if (!count.value) return;
    const step = event.key === "ArrowDown" ? 1 : -1;
    active.value = (active.value + step + count.value) % count.value;
  } else if (event.key === "Enter") {
    event.preventDefault();
    if (active.value >= 0) choose(active.value, true);
  }
}
const rowId = (index: number) => `collection-picker-row-${index}`;
</script>

<template>
  <UiDialog :label="`Collections for ${title}`" initial-focus=".collection-picker__field" @close="emit('close')">
    <div class="collection-picker">
      <header class="collection-picker__head">
        <div class="collection-picker__titles">
          <h1 class="collection-picker__title">Add to collection</h1>
          <span class="collection-picker__book" :title="title">{{ title }}</span>
        </div>
        <UiButton variant="glass" size="sm" icon aria-label="Close collections" @click="emit('close')">
          <X :size="17" />
        </UiButton>
      </header>
      <input
        v-model="typed"
        class="collection-picker__field"
        type="text"
        role="combobox"
        aria-label="Find or create a collection"
        aria-autocomplete="list"
        aria-controls="collection-picker-list"
        :aria-expanded="count > 0"
        :aria-activedescendant="active >= 0 ? rowId(active) : undefined"
        placeholder="Find or create a collection"
        autocomplete="off"
        spellcheck="false"
        @keydown="onKeydown"
      />
      <ul
        v-if="count"
        id="collection-picker-list"
        class="collection-picker__list"
        role="listbox"
        aria-label="Collections"
        aria-multiselectable="true"
      >
        <li
          v-for="(collection, index) in rows"
          :id="rowId(index)"
          :key="collection.id"
          class="collection-picker__row"
          :class="{ 'collection-picker__row--active': index === active }"
          role="option"
          :aria-label="collection.name"
          :aria-selected="on(collection)"
          @mousemove="active = index"
          @click="choose(index, false)"
        >
          <span class="collection-picker__check" aria-hidden="true"><Check v-if="on(collection)" :size="15" /></span>
          <span class="collection-picker__name">{{ collection.name }}</span>
          <span class="collection-picker__count">{{ collection.books }}</span>
        </li>
        <li
          v-if="creating"
          :id="rowId(rows.length)"
          class="collection-picker__row collection-picker__row--create"
          :class="{ 'collection-picker__row--active': rows.length === active }"
          role="option"
          aria-selected="false"
          @mousemove="active = rows.length"
          @click="choose(rows.length, false)"
        >
          <span class="collection-picker__check" aria-hidden="true"><FolderPlus :size="15" /></span>
          <span class="collection-picker__name">Create “{{ creating }}”</span>
        </li>
      </ul>
      <p v-else class="collection-picker__none">No collections yet. Type a name to create one.</p>
    </div>
  </UiDialog>
</template>

<style scoped>
.collection-picker {
  width: 400px;
  max-width: 100%;
  display: grid;
  gap: var(--space-6);
  padding: var(--space-12);
}
.collection-picker__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-6);
}
.collection-picker__titles {
  display: grid;
  gap: var(--space-1);
  min-width: 0;
}
.collection-picker__title {
  margin: 0;
  font-size: 20px;
  font-weight: 700;
}
.collection-picker__book {
  color: var(--dim);
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.collection-picker__field {
  height: var(--control-md);
  padding: 0 var(--space-6);
  border-radius: var(--radius-md);
  background: var(--wash);
  color: var(--ink);
}
.collection-picker__list {
  display: grid;
  gap: var(--space-1);
  max-height: 320px;
  margin: 0;
  padding: 0;
  overflow-y: auto;
  list-style: none;
}
.collection-picker__row {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  min-height: var(--control-sm);
  padding: var(--space-2) var(--space-5);
  border-radius: var(--radius-sm);
  font-size: 14px;
  cursor: pointer;
}
.collection-picker__row--active {
  background: var(--wash);
}
.collection-picker__row--create {
  color: var(--dim);
}
.collection-picker__check {
  display: grid;
  place-items: center;
  width: var(--space-8);
  color: var(--accent);
}
.collection-picker__row--create .collection-picker__check {
  color: inherit;
}
.collection-picker__name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.collection-picker__count {
  color: var(--dim);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}
.collection-picker__none {
  margin: 0;
  color: var(--dim);
  font-size: 13px;
}
</style>
