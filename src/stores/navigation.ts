import { defineStore } from "pinia";
import { ref, watch } from "vue";

/** The screens. "reader" is the reader, which takes the whole window; "item" is one item's page
 *  (`bookId`); "find" and "audiobooks" find books and audiobooks on this computer. */
export type View = "home" | "library" | "feeds" | "activity" | "item" | "reader" | "find" | "audiobooks";
export type ToolsTab = "bookmarks" | "search" | "details";
export type SettingsTab =
  "appearance" | "engine" | "recordings" | "library" | "feeds" | "sites" | "highlighting" | "storage" | "about";
/** The names the sidebar and the reader's back button give each screen. */
export const VIEW_NAMES: Record<View, string> = {
  home: "Home",
  library: "Library",
  feeds: "Feeds",
  activity: "Activity",
  item: "Details",
  reader: "Reader",
  find: "Find books",
  audiobooks: "Find audiobooks",
};
export const useNavigationStore = defineStore("navigation", () => {
  const view = ref<View>("home");
  const bookId = ref<string | null>(null);
  /** The screen the reader was opened from, where its back button returns. */
  const readerFrom = ref<View>("home");
  watch(
    view,
    (to, from) => {
      if (to === "reader" && from !== "reader") readerFrom.value = from;
    },
    { flush: "sync" },
  );
  function openItem(id: string) {
    bookId.value = id;
    view.value = "item";
  }
  function leaveReader() {
    view.value = readerFrom.value === "item" && !bookId.value ? "home" : readerFrom.value;
  }
  /** The Add dialog, over the current screen. */
  const addOpen = ref(false),
    /** A link to fill in when the dialog opens (one pasted or dropped on the window). */
    addLink = ref(""),
    /** "subscribe" when it was opened to subscribe to a feed (from Feeds). */
    addMode = ref<"add" | "subscribe">("add");
  const openAdd = (link = "", mode: "add" | "subscribe" = "add") => {
    addLink.value = link;
    addMode.value = mode;
    addOpen.value = true;
  };
  const closeAdd = () => (addOpen.value = false);
  /** The list of keyboard shortcuts, over the current screen (the ? key). */
  const shortcutsOpen = ref(false);
  const openShortcuts = () => (shortcutsOpen.value = true);
  const closeShortcuts = () => (shortcutsOpen.value = false);
  /** Settings is a dialog over the current screen; it reopens on the last tab. */
  const settingsOpen = ref(false),
    settingsTab = ref<SettingsTab>("appearance");
  function openSettings(tab?: SettingsTab) {
    if (tab) settingsTab.value = tab;
    settingsOpen.value = true;
  }
  function closeSettings() {
    settingsOpen.value = false;
  }
  /** Book tools is a sheet over the reader. `findRequest` rises each time Ctrl F asks for the
   *  search field, and is 0 when the sheet was opened any other way. */
  const tools = ref<ToolsTab | null>(null),
    findRequest = ref(0);
  let finds = 0;
  function openTools(tab: ToolsTab = "bookmarks") {
    findRequest.value = 0;
    tools.value = tab;
  }
  function closeTools() {
    findRequest.value = 0;
    tools.value = null;
  }
  function findInBook() {
    tools.value = "search";
    findRequest.value = ++finds;
  }
  /** Who reads the open book (its recording or a voice) is a dialog of its own over the reader. */
  const narrationOpen = ref(false);
  function openNarration() {
    closeTools();
    narrationOpen.value = true;
  }
  function closeNarration() {
    narrationOpen.value = false;
  }
  return {
    view,
    bookId,
    readerFrom,
    openItem,
    leaveReader,
    addOpen,
    addLink,
    addMode,
    openAdd,
    closeAdd,
    shortcutsOpen,
    openShortcuts,
    closeShortcuts,
    settingsOpen,
    settingsTab,
    openSettings,
    closeSettings,
    tools,
    findRequest,
    openTools,
    closeTools,
    findInBook,
    narrationOpen,
    openNarration,
    closeNarration,
  };
});
