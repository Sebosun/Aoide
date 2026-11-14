<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { Check, Download, Headphones } from "@lucide/vue";
import { api } from "../../api";
import type { ModelDownloadView, ModelSizeChoice, RecognitionDevice, RecognitionEngine, RecognitionOptions, RecognitionSettings } from "../../api/types";
import SettingsSection from "./SettingsSection.vue";
import SettingsField from "./SettingsField.vue";
import SettingsNote from "./SettingsNote.vue";
import SettingsActions from "./SettingsActions.vue";
import UiSelect from "../ui/UiSelect.vue";
import { useAppSettings } from "../../composables/useAppSettings";
import { formatBytes } from "../../bytes";

/** Speech recognition for syncing recordings: built in (the model, where it runs, and the models
 *  on this computer, downloaded only when asked and checked against fixed checksums), or a
 *  recognition server with the OpenAI transcription API. */
const { settings, error, message, save } = useAppSettings();
const options = ref<RecognitionOptions | null>(null), form = ref<RecognitionSettings | null>(null);
const downloads = ref<Record<string, ModelDownloadView>>({}), downloadError = ref("");
let unlisten: (() => void) | undefined, disposed = false;
async function load() { try { options.value = await api.recognitionOptions(); } catch (e) { downloadError.value = String(e); } }
onMounted(async () => {
  await load();
  const off = await api.onModelDownload(d => { downloads.value = { ...downloads.value, [d.id]: d }; if (d.stage === "done") void load(); });
  if (disposed) off(); else unlisten = off;
});
onUnmounted(() => { disposed = true; unlisten?.(); });
watch(settings, value => { if (value) form.value = { ...value.recognition, server: { ...value.recognition.server } }; }, { once: true });
const engines: { value: RecognitionEngine; label: string; hint: string }[] = [
  { value: "builtIn", label: "Built in", hint: "whisper.cpp inside Aoide" },
  { value: "server", label: "A recognition server", hint: "OpenAI-style, such as faster-whisper-server" },
];
const builtIn = computed(() => form.value?.engine === "builtIn");

const sizes: { value: ModelSizeChoice; label: string; hint: string }[] = [
  { value: "tiny", label: "Tiny", hint: "78 MB · fastest, hears less" },
  { value: "base", label: "Base", hint: "148 MB · recommended" },
  { value: "small", label: "Small", hint: "488 MB · hears more, slower, looser timing" },
];
const devices = computed(() => [
  { value: "auto" as RecognitionDevice, label: "Automatic", hint: options.value?.gpuBuild ? "The GPU" : "The CPU (this build has no GPU support)" },
  { value: "gpu" as RecognitionDevice, label: "GPU", hint: options.value?.gpuBuild ? "CUDA or Vulkan" : "Needs a build with GPU support", disabled: !options.value?.gpuBuild },
  { value: "cpu" as RecognitionDevice, label: "CPU", hint: "Works everywhere; about 25× real time on 8 threads" },
]);
const models = computed(() => (options.value?.models ?? []).filter(m => !form.value || m.size === form.value.modelSize));
async function download(id: string) {
  downloadError.value = "";
  downloads.value = { ...downloads.value, [id]: { id, stage: "downloading", doneBytes: 0, totalBytes: 0, error: null } };
  try { await api.downloadModel(id); await load(); }
  catch (e) { downloadError.value = String(e); }
}
function submit() { if (form.value) void save({ recognition: { ...form.value, threads: Math.max(0, Math.round(Number(form.value.threads) || 0)) } }); }
</script>

<template>
  <form v-if="form" class="recording-settings" @submit.prevent="submit">
    <SettingsSection title="Recordings" :icon="Headphones">
      <SettingsNote v-if="builtIn">To sync an audiobook with its book, Aoide listens to the narration with a speech-recognition model (whisper.cpp) on this computer. Nothing is sent anywhere.</SettingsNote>
      <SettingsNote v-else>Aoide sends the narration, a few minutes at a time, to this server's <code>/v1/audio/transcriptions</code> and needs word timestamps back (faster-whisper-server and LocalAI give them).</SettingsNote>
      <SettingsField label="Recognition" as="div"><UiSelect v-model="form.engine" label="What recognises the narration" :options="engines" /></SettingsField>
      <template v-if="builtIn">
        <SettingsField label="Model" as="div"><UiSelect v-model="form.modelSize" label="Speech-recognition model" :options="sizes" /></SettingsField>
        <SettingsField label="Runs on" as="div"><UiSelect v-model="form.device" label="Where recognition runs" :options="devices" /></SettingsField>
        <SettingsField label="CPU threads" sub="0 uses half the cores"><input v-model.number="form.threads" type="number" min="0" max="64" aria-label="CPU threads" /></SettingsField>
      </template>
      <template v-else>
        <SettingsField label="Address"><input v-model.trim="form.server.baseUrl" type="url" placeholder="http://127.0.0.1:8000" aria-label="Recognition server address" /></SettingsField>
        <SettingsField label="Model" sub="Empty sends whisper-1"><input v-model.trim="form.server.model" type="text" placeholder="Systran/faster-whisper-base.en" aria-label="Recognition server model" /></SettingsField>
        <label class="recording-settings__check"><input v-model="form.server.network" type="checkbox" true-value="privateNetwork" false-value="loopback" aria-label="Allow home-network addresses for recognition" /> Allow addresses on my home network</label>
        <SettingsNote v-if="form.server.network === 'privateNetwork'" tone="warn">The narration will be sent over your home network to this server. Use this only with a machine you trust.</SettingsNote>
      </template>
      <label class="recording-settings__check"><input v-model="form.resumeOnStart" type="checkbox" aria-label="Carry on syncing when Aoide starts" /> Carry on syncing recordings when Aoide starts</label>
    </SettingsSection>
    <SettingsActions :message="message" :error="error" />

    <SettingsSection v-if="builtIn" title="Models on this computer">
      <ul class="recording-settings__models" aria-label="Speech-recognition models">
        <li v-for="model in models" :key="model.id" class="recording-settings__model">
          <span class="recording-settings__name">{{ model.id }}<small>{{ model.englishOnly ? "English books" : "Other languages" }} · {{ formatBytes(model.bytes) }}</small></span>
          <span v-if="model.downloaded" class="recording-settings__have"><Check :size="14" aria-hidden="true" /> On this computer</span>
          <template v-else-if="downloads[model.id]?.stage === 'downloading'">
            <progress :value="downloads[model.id].doneBytes" :max="Math.max(1, downloads[model.id].totalBytes)" :aria-label="`Downloading ${model.id}`" />
          </template>
          <button v-else type="button" class="recording-settings__download" :aria-label="`Download ${model.id}`" @click="download(model.id)"><Download :size="14" /> Download</button>
        </li>
      </ul>
      <p v-if="downloadError" class="recording-settings__error" role="alert">{{ downloadError }}</p>
      <SettingsNote>Downloading is the only time recognition uses the network: one file from huggingface.co (the whisper.cpp project), checked against a fixed checksum before it is used. You can also put a <code>ggml-*.bin</code> model in <code>{{ options?.modelsFolder }}</code>.</SettingsNote>
    </SettingsSection>
  </form>
</template>

<style scoped>
.recording-settings { display: grid; gap: 22px; }
.recording-settings__check { display: flex; align-items: center; gap: 10px; font-size: 13px; }
.recording-settings__models { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
.recording-settings__model { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; gap: 12px; padding: 8px 0; border-bottom: 1px solid var(--line); font-size: 13px; }
.recording-settings__name { display: grid; font-weight: 700; }
.recording-settings__name small { font-weight: 500; color: var(--dim); font-size: 12px; }
.recording-settings__have { display: inline-flex; align-items: center; gap: 5px; color: var(--ok); font-weight: 600; font-size: 12.5px; }
.recording-settings__download { display: inline-flex; align-items: center; gap: 6px; border-radius: 999px; background: var(--ground); font-size: 12.5px; font-weight: 700; padding: 7px 14px; }
.recording-settings__model progress { width: 140px; height: 5px; }
.recording-settings__error { font-size: 12px; font-weight: 600; color: var(--accent); overflow-wrap: anywhere; }
.recording-settings :deep(code) { white-space: nowrap; }
</style>
