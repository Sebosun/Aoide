<script setup lang="ts">
import { computed } from "vue";
import { Plus, FilePlus, FolderSearch, Headphones, RefreshCw, Upload } from "@lucide/vue";
import UiMenu from "../ui/UiMenu.vue";
import type { MenuEntry } from "../ui/types";
import { useLibraryStore } from "../../stores/library";
import { useNavigationStore } from "../../stores/navigation";

const library = useLibraryStore(),
  navigation = useNavigationStore();
const items = computed<MenuEntry[]>(() => [
  {
    id: "choose",
    label: "Choose EPUB files…",
    ariaLabel: "Choose EPUB files…",
    icon: FilePlus,
    action: () => {
      void library.chooseFiles();
    },
  },
  {
    id: "find",
    label: "Find books on this computer",
    ariaLabel: "Find books on this computer",
    icon: FolderSearch,
    action: () => {
      navigation.view = "find";
    },
  },
  {
    id: "audiobooks",
    label: "Find audiobooks for your books",
    ariaLabel: "Find audiobooks for your books",
    icon: Headphones,
    action: () => {
      navigation.view = "audiobooks";
    },
  },
  {
    id: "rescan",
    label: "Rescan library folder",
    ariaLabel: "Rescan library folder",
    icon: RefreshCw,
    disabled: library.loading,
    action: () => {
      void library.rescan();
    },
  },
  { kind: "separator" },
  { kind: "note", label: "Or drop EPUB files anywhere on this window.", icon: Upload },
]);
</script>

<template>
  <UiMenu v-slot="{ toggle, trigger }" label="Add books" :items="items" align="end" :width="300">
    <button type="button" class="add-books__button" aria-label="Add books" v-bind="trigger" @click="toggle">
      <Plus :size="16" /> Add
    </button>
  </UiMenu>
</template>

<style scoped>
.add-books__button {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 36px;
  padding: 0 16px;
  border-radius: 999px;
  background: var(--glass);
  font-weight: 700;
  font-size: 13px;
  white-space: nowrap;
}
</style>
