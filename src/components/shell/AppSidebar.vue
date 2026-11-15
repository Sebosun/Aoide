<script setup lang="ts">
import { computed } from "vue";
import { ChartNoAxesColumn, House, Library, Settings } from "@lucide/vue";
import AddBooksMenu from "../library/AddBooksMenu.vue";
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
const collections = computed(() =>
  library.collections.map((collection) => ({
    ...collection,
    count: library.books.filter((b) => b.status.kind !== "error" && b.collections.includes(collection.id)).length,
    current: navigation.view === "library" && library.collection?.id === collection.id,
  })),
);
function openCollection(id: number) {
  library.filter = `collection:${id}`;
  navigation.view = "library";
}
</script>

<template>
  <nav class="sidebar" aria-label="Main">
    <div class="sidebar__brand">aoide</div>
    <AddBooksMenu wide />
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
    <section v-if="collections.length" class="sidebar__collections" aria-labelledby="sidebar-collections">
      <h2 id="sidebar-collections" class="sidebar__heading">Collections</h2>
      <button
        v-for="collection in collections"
        :key="collection.id"
        type="button"
        class="sidebar__collection"
        :class="{ 'sidebar__link--on': collection.current }"
        :aria-current="collection.current ? 'page' : undefined"
        @click="openCollection(collection.id)"
      >
        <span class="sidebar__collection-name">{{ collection.name }}</span><span class="sidebar__count">{{
          collection.count
        }}</span>
      </button>
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
.sidebar__heading {
  margin: 0;
  padding: 0 12px 6px;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--dim);
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
