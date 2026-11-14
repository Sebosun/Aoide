import { defineStore } from "pinia";
import { ref } from "vue";

/** "reader" is the Now Playing screen; "book" is a book's details page (`bookId`). */
export type View = "library" | "reader" | "stats" | "find" | "book";
export type ToolsTab = "bookmarks" | "search" | "voice" | "details";
export type SettingsTab = "appearance" | "engine" | "recordings" | "library" | "highlighting" | "storage" | "about";
export const useNavigationStore = defineStore("navigation", () => {
  const view = ref<View>("library");
  const bookId = ref<string | null>(null);
  function openBook(id: string) { bookId.value = id; view.value = "book"; }
  /** Settings is a dialog over the current screen; it reopens on the last tab. */
  const settingsOpen = ref(false), settingsTab = ref<SettingsTab>("appearance");
  function openSettings(tab?: SettingsTab) { if (tab) settingsTab.value = tab; settingsOpen.value = true; }
  function closeSettings() { settingsOpen.value = false; }
  /** Book tools is a sheet over Now Playing. `findRequest` rises each time Ctrl F asks for the
   *  search field, and is 0 when the sheet was opened any other way. */
  const tools = ref<ToolsTab | null>(null), findRequest = ref(0);
  let finds = 0;
  function openTools(tab: ToolsTab = "bookmarks") { findRequest.value = 0; tools.value = tab; }
  function closeTools() { findRequest.value = 0; tools.value = null; }
  function findInBook() { tools.value = "search"; findRequest.value = ++finds; }
  return { view, bookId, openBook, settingsOpen, settingsTab, openSettings, closeSettings, tools, findRequest, openTools, closeTools, findInBook };
});
