<script setup lang="ts">
import { computed } from "vue";
import { Settings } from "@lucide/vue";
import EnginePill from "./EnginePill.vue";
import { useNavigationStore, type View } from "../../stores/navigation";
import { usePlayerStore } from "../../stores/player";
import { useReaderStore } from "../../stores/reader";

const navigation = useNavigationStore(), player = usePlayerStore(), reader = useReaderStore();
const canPlay = computed(() => Boolean(reader.book || player.state.bookId));
async function nowPlaying() {
  if (reader.book) navigation.view = "reader";
  else if (player.state.bookId) await reader.open(player.state.bookId);
}
/** The wordmark goes to what you are listening to, or to the library when nothing is open. */
function home() { if (canPlay.value) void nowPlaying(); else navigation.view = "library"; }
const pills: { view: View; label: string }[] = [{ view: "reader", label: "Now Playing" }, { view: "library", label: "Library" }, { view: "stats", label: "Stats" }];
const current = (view: View) => navigation.view === view || (view === "library" && (navigation.view === "find" || navigation.view === "book"));
</script>

<template>
  <header class="top-bar">
    <button type="button" class="top-bar__brand" aria-label="Aoide home" :title="canPlay ? 'Now Playing' : 'Library'" @click="home">aoide</button>
    <nav class="top-bar__nav" aria-label="Main">
      <button v-for="pill in pills" :key="pill.view" type="button" class="top-bar__pill" :class="{ 'top-bar__pill--on': current(pill.view) }" :aria-label="pill.label" :aria-current="current(pill.view) ? 'page' : undefined" :disabled="pill.view === 'reader' && !canPlay" :title="pill.view === 'reader' && !canPlay ? 'Open a book to start listening' : undefined" @click="pill.view === 'reader' ? nowPlaying() : (navigation.view = pill.view)">{{ pill.label }}</button>
    </nav>
    <div class="top-bar__end">
      <EnginePill />
      <button type="button" class="top-bar__round" :class="{ 'top-bar__round--on': navigation.settingsOpen }" aria-label="Settings" aria-haspopup="dialog" :aria-expanded="navigation.settingsOpen" title="Settings" @click="navigation.openSettings()"><Settings :size="17" /></button>
    </div>
  </header>
</template>

<style scoped>
.top-bar { position: sticky; top: 0; z-index: 6; height: 64px; display: grid; grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr); align-items: center; gap: 16px; padding: 0 28px; background: var(--screen); background-attachment: fixed; }
.top-bar__brand { justify-self: start; padding: 2px 4px; margin-left: -4px; border-radius: 8px; font-weight: 800; font-size: 20px; letter-spacing: -.03em; }
.top-bar__nav { display: flex; gap: 2px; padding: 4px; border-radius: 999px; background: var(--glass); }
.top-bar__pill { padding: 7px 16px; border-radius: 999px; font-weight: 600; font-size: 13px; color: var(--dim); white-space: nowrap; }
.top-bar__pill:hover:not(:disabled):not(.top-bar__pill--on) { color: var(--ink); }
.top-bar__pill--on { background: var(--ink); color: var(--ground); }
.top-bar__end { display: flex; justify-content: flex-end; align-items: center; gap: 10px; min-width: 0; }
.top-bar__round { width: 36px; height: 36px; display: grid; place-items: center; border-radius: 50%; background: var(--glass); flex-shrink: 0; }
.top-bar__round--on { background: var(--ink); color: var(--ground); }
@media (max-width: 760px) {
  .top-bar { padding: 0 16px; gap: 10px; }
  .top-bar__pill { padding: 7px 11px; font-size: 12.5px; }
}
@media (max-width: 480px) { .top-bar__brand { font-size: 17px; } .top-bar__pill { padding: 6px 8px; } }
</style>
