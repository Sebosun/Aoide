<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { AudioLines, Headphones } from "@lucide/vue";
import { api } from "../../api";
import type { Narration } from "../../api/types";
import BookVoice from "./BookVoice.vue";
import RecordingPanel from "../recording/RecordingPanel.vue";
import { useRecordingsStore } from "../../stores/recordings";
import { useReaderStore } from "../../stores/reader";
import { recordingStatus } from "../../recording";

/** Who reads the book: its paired recording (synced, or while it syncs) or a speech engine, with each one's
 *  settings. Switching restarts playback at the current sentence. */
const props = defineProps<{ bookId: string }>();
const recordings = useRecordingsStore(), reader = useReaderStore();
const narration = ref<Narration>("speech"), error = ref("");
const recording = computed(() => recordings.byBook[props.bookId] ?? null);
const ready = computed(() => Boolean(recording.value && recordingStatus(recording.value).listenable));
/** What actually plays: the recording only when chosen and (partly) synced. */
const playing = computed<Narration>(() => narration.value === "recording" && ready.value ? "recording" : "speech");
onMounted(async () => {
  try { narration.value = (await api.bookSettings(props.bookId)).narration ?? "speech"; } catch (e) { error.value = String(e); }
});
async function choose(next: Narration) {
  if (next === playing.value) return;
  error.value = "";
  try {
    await recordings.setNarration(props.bookId, next);
    narration.value = next;
    // Footnotes are read only with a speech engine, so the chapters' segments may change.
    if (reader.book?.id === props.bookId) {
      reader.cache = Object.fromEntries(Object.entries(reader.cache).filter(([key]) => !key.startsWith(`${props.bookId}:`)));
      await reader.loadChapter(reader.currentChapter);
    }
  } catch (e) { error.value = String(e); }
}
</script>

<template>
  <div class="book-narration">
    <div v-if="recording" class="book-narration__switch" role="radiogroup" aria-label="Narration">
      <button type="button" role="radio" class="book-narration__option" :class="{ 'book-narration__option--on': playing === 'recording' }" :aria-checked="playing === 'recording'" :disabled="!ready" :title="ready ? 'The paired recording' : 'Available once part of the recording is synced'" @click="choose('recording')"><Headphones :size="15" aria-hidden="true" /> Recording</button>
      <button type="button" role="radio" class="book-narration__option" :class="{ 'book-narration__option--on': playing === 'speech' }" :aria-checked="playing === 'speech'" @click="choose('speech')"><AudioLines :size="15" aria-hidden="true" /> Speech engine</button>
    </div>
    <p v-if="error" class="book-narration__error" role="alert">{{ error }}</p>
    <RecordingPanel :book-id="bookId" />
    <div class="book-narration__speech">
      <BookVoice v-if="playing === 'speech'" :book-id="bookId" />
      <p v-else class="book-narration__hint">The narrator's recording plays for this book. Choose <em>Speech engine</em> above for a voice, footnotes read aloud and preparing audio ahead.</p>
    </div>
  </div>
</template>

<style scoped>
.book-narration { display: grid; gap: 20px; }
.book-narration__switch { justify-self: start; display: flex; gap: 2px; padding: 3px; border-radius: 999px; background: var(--ground); }
.book-narration__option { display: inline-flex; align-items: center; gap: 6px; padding: 7px 14px; border-radius: 999px; font-size: 13px; font-weight: 700; color: var(--dim); }
.book-narration__option--on { background: var(--ink); color: var(--ground); }
.book-narration__option:disabled { opacity: .45; }
.book-narration__speech { padding-top: 20px; border-top: 1px solid var(--line); }
.book-narration__hint { font-size: 12.5px; color: var(--dim); line-height: 1.5; }
.book-narration__error { font-size: 12px; font-weight: 600; color: var(--accent); }
</style>
