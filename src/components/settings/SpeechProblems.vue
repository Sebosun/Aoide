<script setup lang="ts">
import { onMounted, ref } from "vue";
import { CircleAlert } from "@lucide/vue";
import { api } from "@/api";
import type { SpeechProblem } from "@/api/types";
import UiButton from "@/components/ui/UiButton.vue";
import SettingsSection from "@/components/settings/SettingsSection.vue";

/** What went wrong with the speech engine lately (core/speech_problems.rs), newest first. The
 *  player says it in a few words; here each problem has its whole error, to fix the server. */
const problems = ref<SpeechProblem[]>([]),
  error = ref("");
async function load() {
  try {
    problems.value = await api.speechProblems();
  } catch (e) {
    error.value = String(e);
  }
}
async function clear() {
  try {
    await api.clearSpeechProblems();
    problems.value = [];
  } catch (e) {
    error.value = String(e);
  }
}
onMounted(load);
const time = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
const when = (p: SpeechProblem) =>
  p.times > 1 ? `${p.times} times, last at ${time(p.lastMs)}` : `at ${time(p.lastMs)}`;
</script>

<template>
  <SettingsSection title="Problems" :icon="CircleAlert">
    <template #aside>
      <UiButton v-if="problems.length" size="sm" @click="clear">Clear</UiButton>
    </template>
    <p v-if="error" class="speech-problems__error" role="alert">{{ error }}</p>
    <p v-if="!problems.length" class="speech-problems__none">No problems with the speech engine since Aoide started.</p>
    <ol v-else class="speech-problems" aria-label="Speech engine problems">
      <li v-for="p in problems" :key="`${p.firstMs}:${p.detail}`" class="speech-problems__item">
        <span class="speech-problems__head"
          ><b>{{ p.message }}</b
          ><span class="speech-problems__when">{{ when(p) }}</span></span
        >
        <code class="speech-problems__detail">{{ p.detail }}</code>
      </li>
    </ol>
  </SettingsSection>
</template>

<style scoped>
.speech-problems {
  display: grid;
  gap: var(--space-4);
  margin: 0;
  padding: 0;
  list-style: none;
}
.speech-problems__item {
  display: grid;
  gap: var(--space-2);
  padding: var(--space-5) var(--space-6);
  border-radius: var(--radius-md);
  background: var(--wash);
}
.speech-problems__head {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: var(--space-4);
  font-size: 13px;
}
.speech-problems__when,
.speech-problems__none {
  color: var(--dim);
  font-size: 12.5px;
}
.speech-problems__none {
  margin: 0;
}
.speech-problems__detail {
  font-size: 12px;
  color: var(--dim);
  overflow-wrap: anywhere;
}
.speech-problems__error {
  margin: 0;
  color: var(--accent);
}
</style>
