<script setup lang="ts">
import { computed } from "vue";
import { Headphones, Mic, Settings2 } from "@lucide/vue";
import UiMenu from "@/components/ui/UiMenu.vue";
import type { MenuEntry } from "@/components/ui/types";
import { api } from "@/api";
import { useLibraryStore } from "@/stores/library";
import { useNavigationStore } from "@/stores/navigation";
import { usePlayerStore } from "@/stores/player";
import { useReaderStore } from "@/stores/reader";
import { useRecordingsStore } from "@/stores/recordings";

/** Listen / Listening in the reader's header: turns listening to the open book on (playing
 *  from where the user is reading) and off (reading on silently). When nothing could narrate
 *  the book (the speech engine is offline or failing and it has no recording), it offers the
 *  ways to give it a voice instead of failing. */
const reader = useReaderStore(),
  library = useLibraryStore(),
  player = usePlayerStore(),
  recordings = useRecordingsStore(),
  navigation = useNavigationStore();
const listening = computed(() => reader.shape === "readAlong");
const summary = computed(() => library.books.find((b) => b.id === reader.book?.id));
const voiceless = computed(
  () => summary.value?.audio !== "recording" && (player.engine.state === "offline" || player.engine.state === "error"),
);
const engineLine = computed(() =>
  player.engine.state === "error"
    ? `${player.engine.profileName} has a problem`
    : `${player.engine.profileName} is not running`,
);
async function pair() {
  const id = reader.book?.id;
  const paths = await api.chooseAudio(false);
  if (id && paths?.length) recordings.openPairing(paths, id);
}
const choices = computed<MenuEntry[]>(() => [
  { kind: "note", label: engineLine.value },
  { id: "narrate", label: "Narrate with a voice", icon: Headphones, action: () => void reader.listen() },
  { id: "pair", label: "Pair an audiobook…", icon: Mic, action: () => void pair() },
  { kind: "separator" },
  { id: "engine", label: "Speech engine settings", icon: Settings2, action: () => navigation.openSettings("engine") },
]);
async function toggle() {
  if (listening.value) await reader.setMode("read");
  else await reader.listen();
}
</script>

<template>
  <UiMenu v-if="!listening && voiceless" label="Listen" :items="choices" align="end" :width="260">
    <template #default="{ toggle: open, trigger }">
      <button type="button" class="listen-button" v-bind="trigger" @click="open">
        <Headphones :size="18" aria-hidden="true" />Listen
      </button>
    </template>
  </UiMenu>
  <button
    v-else
    type="button"
    class="listen-button"
    :class="{ 'listen-button--on': listening }"
    :aria-pressed="listening"
    :title="listening ? 'Stop listening and read on' : 'Listen from here'"
    @click="toggle"
  >
    <Headphones :size="18" aria-hidden="true" />{{ listening ? "Listening" : "Listen" }}
  </button>
</template>

<style scoped>
.listen-button {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 44px;
  padding: 0 16px;
  border-radius: 22px;
  background: var(--glass);
  font-weight: 600;
}
.listen-button--on {
  background: var(--ink);
  color: var(--ground);
}
</style>
