<script setup lang="ts">
import { computed } from "vue";
import { ChartNoAxesColumn, Ellipsis, House, Library, Pencil, Plus, Settings, Trash2 } from "@lucide/vue";
import type { Collection } from "../../api/types";
import UiMenu from "../ui/UiMenu.vue";
import type { MenuEntry } from "../ui/types";
import { useCollections } from "../../composables/useCollections";
import EnginePill from "./EnginePill.vue";
import MiniPlayer from "../player/MiniPlayer.vue";
import { useLibraryStore } from "../../stores/library";
import { useNavigationStore, type View } from "../../stores/navigation";

/** The app's left column: wordmark, Add, the main screens, collections, and at the bottom what
 *  is playing and Settings. Below 720 px it stacks above the screen. */
const navigation = useNavigationStore(),
  library = useLibraryStore();
const links: { view: View; label: string; icon: typeof House }[] = [
  { view: "home", label: "Home", icon: House },
  { view: "library", label: "Library", icon: Library },
  { view: "activity", label: "Activity", icon: ChartNoAxesColumn },
];
/** Library stays marked while one of its books or its finders is open, unless a collection is. */
const current = (view: View) =>
  view === "library"
    ? !library.collection && ["library", "find", "item", "audiobooks"].includes(navigation.view)
    : navigation.view === view;
function go(view: View) {
  if (view === "library" && library.collection) library.filter = "all";
  navigation.view = view;
}
const shelves = useCollections();
const collections = computed(() =>
  library.collections.map((collection) => ({
    ...collection,
    current: navigation.view === "library" && library.collection?.id === collection.id,
  })),
);
function openCollection(id: number) {
  library.filter = `collection:${id}`;
  navigation.view = "library";
}
const actions = (collection: Collection): MenuEntry[] => [
  { id: "rename", label: "Rename…", icon: Pencil, action: () => shelves.rename(collection) },
  { id: "delete", label: "Delete…", icon: Trash2, danger: true, action: () => shelves.remove(collection) },
];
</script>

<template>
  <nav class="sidebar" aria-label="Main">
    <div class="sidebar__brand">aoide</div>
    <button
      type="button"
      class="sidebar__add"
      aria-label="Add"
      aria-haspopup="dialog"
      title="Add books or audiobooks (Ctrl N)"
      @click="navigation.openAdd()"
    >
      <Plus :size="16" aria-hidden="true" /> Add
    </button>
    <div class="sidebar__links">
      <button
        v-for="link in links"
        :key="link.view"
        type="button"
        class="sidebar__link"
        :class="{ 'sidebar__link--on': current(link.view) }"
        :aria-label="link.label"
        :aria-current="current(link.view) ? 'page' : undefined"
        @click="go(link.view)"
      >
        <component :is="link.icon" :size="18" aria-hidden="true" />{{ link.label }}
      </button>
    </div>
    <section class="sidebar__collections" aria-labelledby="sidebar-collections">
      <div class="sidebar__heading-row">
        <h2 id="sidebar-collections" class="sidebar__heading">Collections</h2>
        <button
          type="button"
          class="sidebar__icon"
          aria-label="New collection"
          title="New collection"
          @click="shelves.create()"
        >
          <Plus :size="15" aria-hidden="true" />
        </button>
      </div>
      <div
        v-for="collection in collections"
        :key="collection.id"
        class="sidebar__shelf"
        :class="{ 'sidebar__shelf--on': collection.current }"
      >
        <button
          type="button"
          class="sidebar__collection"
          :aria-current="collection.current ? 'page' : undefined"
          @click="openCollection(collection.id)"
        >
          <span class="sidebar__collection-name">{{ collection.name }}</span><span class="sidebar__count">{{
            collection.books
          }}</span>
        </button>
        <UiMenu
          v-slot="{ toggle, trigger }"
          class="sidebar__more"
          :label="`Actions for ${collection.name}`"
          :items="actions(collection)"
          align="end"
        >
          <button
            type="button"
            class="sidebar__icon"
            :aria-label="`More for ${collection.name}`"
            v-bind="trigger"
            @click="toggle"
          >
            <Ellipsis :size="15" aria-hidden="true" />
          </button>
        </UiMenu>
      </div>
    </section>
    <div class="sidebar__end">
      <EnginePill />
      <MiniPlayer />
      <button
        type="button"
        class="sidebar__collection sidebar__settings"
        aria-label="Settings"
        aria-haspopup="dialog"
        :aria-expanded="navigation.settingsOpen"
        @click="navigation.openSettings()"
      >
        <Settings :size="18" aria-hidden="true" /><span class="sidebar__collection-name">Settings</span>
      </button>
    </div>
  </nav>
</template>

<style scoped>
.sidebar {
  position: sticky;
  top: 0;
  height: 100vh;
  width: 236px;
  flex: none;
  display: flex;
  flex-direction: column;
  gap: 22px;
  padding: 22px 16px;
  border-right: 1px solid var(--line);
  overflow-y: auto;
}
.sidebar__brand {
  padding: 0 10px;
  font-size: 21px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.sidebar__add {
  height: 44px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  border-radius: 22px;
  background: var(--ink);
  color: var(--ground);
  font-weight: 600;
}
.sidebar__links,
.sidebar__collections {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.sidebar__link,
.sidebar__collection {
  height: 44px;
  padding: 0 12px;
  display: flex;
  align-items: center;
  gap: 12px;
  border-radius: 12px;
  color: var(--dim);
  font-weight: 500;
  text-align: left;
}
.sidebar__collection {
  height: 40px;
  justify-content: space-between;
}
.sidebar__link:hover,
.sidebar__collection:hover {
  color: var(--ink);
  background: var(--glass);
}
.sidebar__link--on,
.sidebar__link--on:hover {
  background: var(--page);
  color: var(--ink);
  font-weight: 700;
}
.sidebar__heading-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 4px 4px 0;
}
.sidebar__heading {
  margin: 0;
  padding: 0 12px;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--dim);
}
.sidebar__shelf {
  position: relative;
  display: flex;
  align-items: center;
  border-radius: 12px;
}
.sidebar__shelf .sidebar__collection {
  flex: 1;
  min-width: 0;
}
.sidebar__shelf:hover,
.sidebar__shelf:focus-within {
  background: var(--glass);
}
.sidebar__shelf:hover .sidebar__collection,
.sidebar__shelf:focus-within .sidebar__collection {
  background: none;
  color: var(--ink);
}
.sidebar__shelf--on,
.sidebar__shelf--on:hover,
.sidebar__shelf--on:focus-within {
  background: var(--page);
}
.sidebar__shelf--on .sidebar__collection {
  color: var(--ink);
  font-weight: 700;
}
.sidebar__more {
  position: absolute;
  right: 4px;
  opacity: 0;
}
.sidebar__shelf:hover .sidebar__more,
.sidebar__shelf:focus-within .sidebar__more {
  opacity: 1;
}
.sidebar__shelf:hover .sidebar__count,
.sidebar__shelf:focus-within .sidebar__count {
  visibility: hidden;
}
.sidebar__icon {
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
  border-radius: 50%;
  color: var(--dim);
}
.sidebar__icon:hover,
.sidebar__icon[aria-expanded="true"] {
  color: var(--ink);
  background: var(--wash);
}
.sidebar__collection-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.sidebar__count {
  color: var(--dim);
  font-variant-numeric: tabular-nums;
}
.sidebar__end {
  margin-top: auto;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.sidebar__settings {
  justify-content: flex-start;
}
@media (max-width: 719px) {
  .sidebar {
    position: static;
    height: auto;
    width: auto;
    gap: 14px;
    padding: 14px 16px;
    border-right: 0;
    border-bottom: 1px solid var(--line);
  }
  .sidebar__links {
    flex-direction: row;
    flex-wrap: wrap;
  }
  .sidebar__end {
    margin-top: 0;
  }
}
</style>
