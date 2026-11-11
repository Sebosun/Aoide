<script setup lang="ts">
import { ref, watch } from "vue";
import { Highlighter } from "@lucide/vue";
import type { AppSettings } from "../../api/types";
import SettingsSection from "./SettingsSection.vue";
import SettingsField from "./SettingsField.vue";
import SettingsNote from "./SettingsNote.vue";
import SettingsActions from "./SettingsActions.vue";
import { useAppSettings } from "../../composables/useAppSettings";

/** How the spoken word is found: Aoide's estimate, or a local forced aligner. */
const { settings, error, message, save } = useAppSettings();
const aligned = ref(false), url = ref("http://127.0.0.1:8890"), program = ref(""), args = ref(""), dir = ref("");
watch(settings, value => {
  const aligner = value?.aligner;
  aligned.value = Boolean(aligner);
  if (!aligner) return;
  url.value = aligner.baseUrl;
  program.value = aligner.launch?.program ?? "";
  args.value = aligner.launch?.args.join("\n") ?? "";
  dir.value = aligner.launch?.workingDir ?? "";
}, { once: true });
function submit() {
  const command = program.value.trim();
  const aligner: AppSettings["aligner"] = aligned.value ? {
    baseUrl: url.value.trim(),
    network: settings.value?.aligner?.network ?? "loopback",
    launch: command ? { program: command, args: args.value.split("\n").map(a => a.trim()).filter(Boolean), workingDir: dir.value.trim() || null } : null,
  } : null;
  void save({ aligner });
}
</script>

<template>
  <form v-if="settings" class="word-timing" @submit.prevent="submit">
    <SettingsSection title="Word highlighting" :icon="Highlighter">
      <label class="word-timing__check"><input v-model="aligned" type="checkbox" aria-label="Exact word timing" /> Exact word timing with a local aligner</label>
      <SettingsNote>Without it, Aoide fits word timings to the pauses in the audio. The aligner (<code>tools/aligner/aoide_aligner.py</code>, needs torch and torchaudio) places every word exactly; each sentence is aligned once and remembered.</SettingsNote>
      <template v-if="aligned">
        <SettingsField label="Aligner address"><input v-model="url" type="text" aria-label="Aligner address" spellcheck="false" /></SettingsField>
        <SettingsField label="Launch program"><input v-model="program" type="text" aria-label="Aligner launch program" placeholder="/path/to/venv/bin/python" spellcheck="false" /></SettingsField>
        <SettingsField label="Arguments" sub="one per line" top><textarea v-model="args" aria-label="Aligner launch arguments" rows="3" spellcheck="false" /></SettingsField>
        <SettingsField label="Working folder"><input v-model="dir" type="text" aria-label="Aligner working folder" spellcheck="false" /></SettingsField>
      </template>
    </SettingsSection>
    <SettingsActions :message="message" :error="error" />
  </form>
</template>

<style scoped>
.word-timing { display: grid; gap: 22px; }
.word-timing__check { display: flex; align-items: center; gap: 10px; font-size: 13px; }
</style>
