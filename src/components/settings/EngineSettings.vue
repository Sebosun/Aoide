<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { AudioLines, RefreshCw } from "@lucide/vue";
import { api } from "../../api";
import type { EngineDescriptor, EngineProfile, ParamValue, Voice } from "../../api/types";
import ParamControl from "../ParamControl.vue";
import UiSelect from "../ui/UiSelect.vue";
import { voiceOptions } from "../ui/voiceOptions";
import SettingsSection from "./SettingsSection.vue";
import SettingsField from "./SettingsField.vue";
import SettingsNote from "./SettingsNote.vue";
import SettingsActions from "./SettingsActions.vue";
import VoicePreview from "./VoicePreview.vue";
import { usePlayerStore } from "../../stores/player";

/** The speech engine profile. Edits stay a draft (which Preview speaks with) until saved. */
const player = usePlayerStore();
const descriptors = ref<EngineDescriptor[]>([]);
const profile = ref<EngineProfile | null>(null);
const voices = ref<Voice[] | null>(null), voiceError = ref("");
const launchArgs = ref(""), launchProgram = ref(""), launchDir = ref("");
const message = ref(""), error = ref(""), saving = ref(false), loadError = ref("");

const descriptor = computed(() => descriptors.value.find(d => d.kind === profile.value?.kind));
const languages = computed(() => descriptor.value?.languages?.join(", ") ?? "any language");

onMounted(async () => {
  try {
    const options = await api.engineOptions();
    descriptors.value = options.descriptors;
    profile.value = options.profile;
    launchProgram.value = options.profile.launch?.program ?? "";
    launchArgs.value = options.profile.launch?.args.join("\n") ?? "";
    launchDir.value = options.profile.launch?.workingDir ?? "";
  } catch (e) { loadError.value = String(e); }
  void loadVoices();
});

async function loadVoices() {
  voiceError.value = "";
  try { voices.value = await api.engineVoices(); }
  catch (e) { voices.value = null; voiceError.value = String(e); }
}

watch(() => profile.value?.kind, (kind, previous) => {
  if (!profile.value || !previous || kind === previous) return;
  const keys = new Set(descriptor.value?.params.map(p => p.key));
  profile.value.params = Object.fromEntries(Object.entries(profile.value.params).filter(([key]) => keys.has(key)));
});

function setParam(key: string, value: ParamValue | undefined) {
  if (!profile.value) return;
  const params = { ...profile.value.params };
  if (value === undefined) delete params[key]; else params[key] = value;
  profile.value.params = params;
}

/** The profile as it would be saved, which is also what Preview speaks with. */
const draft = computed<EngineProfile | null>(() => {
  if (!profile.value) return null;
  const program = launchProgram.value.trim();
  return {
    ...profile.value,
    name: profile.value.name.trim() || descriptor.value?.name || "Engine",
    baseUrl: profile.value.baseUrl.trim(),
    model: profile.value.model?.trim() || null,
    voiceId: profile.value.voiceId?.trim() || null,
    launch: program ? { program, args: launchArgs.value.split("\n").map(a => a.trim()).filter(Boolean), workingDir: launchDir.value.trim() || null } : null,
  };
});

async function save() {
  const next = draft.value;
  if (!next) return;
  message.value = ""; error.value = ""; saving.value = true;
  try {
    await api.saveEngineProfile(next);
    profile.value = next;
    message.value = "Saved. Upcoming audio will use these settings.";
    void loadVoices();
  } catch (e) { error.value = String(e); }
  finally { saving.value = false; }
}
</script>

<template>
  <p v-if="loadError" class="engine-settings__error" role="alert">{{ loadError }}</p>
  <form v-if="profile && draft" class="engine-settings" @submit.prevent="save">
    <SettingsSection title="Speech engine" :icon="AudioLines">
      <template #aside><span class="engine-settings__status" :class="`engine-settings__status--${player.engine.state}`">{{ player.engine.profileName }} · {{ player.engine.state }}</span></template>
      <SettingsNote>Aoide sends one sentence at a time to a speech server on this computer. The controls below come from the selected engine.</SettingsNote>
      <SettingsField label="Profile name"><input v-model="profile.name" type="text" aria-label="Profile name" /></SettingsField>
      <SettingsField label="Engine type" as="div"><UiSelect v-model="profile.kind" label="Engine type" :options="descriptors.map(d => ({ value: d.kind, label: d.name }))" /></SettingsField>
      <SettingsField label="Server address"><input v-model="profile.baseUrl" type="text" aria-label="Server address" placeholder="http://127.0.0.1:8004" spellcheck="false" /></SettingsField>
      <SettingsField label="Model"><input :value="profile.model ?? ''" type="text" aria-label="Model" placeholder="Server default" spellcheck="false" @input="profile.model = ($event.target as HTMLInputElement).value" /></SettingsField>
      <label class="engine-settings__check"><input v-model="profile.network" type="checkbox" true-value="privateNetwork" false-value="loopback" aria-label="Allow home-network addresses" /> Allow addresses on my home network</label>
      <SettingsNote v-if="profile.network === 'privateNetwork'" tone="warn">Your book text will be sent over your home network to this server. Use this only with a machine you trust.</SettingsNote>
    </SettingsSection>

    <SettingsSection title="Voice">
      <SettingsField label="Default voice" as="div">
        <UiSelect v-if="voices" label="Default voice" :options="voiceOptions(voices, profile.voiceId, 'Server default')" :model-value="profile.voiceId ?? ''" @update:model-value="id => profile!.voiceId = id || null" />
        <input v-else :value="profile.voiceId ?? ''" type="text" aria-label="Default voice" placeholder="Voice id" @input="profile.voiceId = ($event.target as HTMLInputElement).value" />
        <button type="button" class="engine-settings__icon" aria-label="Reload voices" title="Reload voices" @click="loadVoices"><RefreshCw :size="14" /></button>
      </SettingsField>
      <SettingsNote v-if="voiceError">Voices are listed once the engine is running.</SettingsNote>
      <VoicePreview :request="{ profile: draft, voiceId: draft.voiceId, params: {}, language: null }" />
      <ParamControl v-for="spec in descriptor?.params ?? []" :key="spec.key" :spec="spec" :model-value="profile.params[spec.key]" @update:model-value="setParam(spec.key, $event)" />
      <SettingsNote>Languages: {{ languages }}.</SettingsNote>
    </SettingsSection>

    <SettingsSection title="Start the server automatically">
      <SettingsNote>When the engine is offline, Aoide runs this command, and stops it again when Aoide quits.</SettingsNote>
      <SettingsField label="Program"><input v-model="launchProgram" type="text" aria-label="Launch program" placeholder="/path/to/python" spellcheck="false" /></SettingsField>
      <SettingsField label="Arguments" sub="one per line" top><textarea v-model="launchArgs" aria-label="Launch arguments" rows="3" spellcheck="false" /></SettingsField>
      <SettingsField label="Working folder"><input v-model="launchDir" type="text" aria-label="Launch working folder" spellcheck="false" /></SettingsField>
    </SettingsSection>

    <SettingsActions label="Save engine" :busy="saving" :message="message" :error="error" />
  </form>
</template>

<style scoped>
.engine-settings { display: grid; gap: 22px; }
.engine-settings__status { font-size: 12px; font-weight: 600; padding: 5px 11px; border-radius: 999px; background: var(--ground); color: var(--dim); }
.engine-settings__status--ready { color: var(--ok); }
.engine-settings__status--error { color: var(--accent); }
.engine-settings__status--busy, .engine-settings__status--loading { color: var(--warn); }
.engine-settings__check { display: flex; align-items: center; gap: 10px; font-size: 13px; }
.engine-settings__icon { display: grid; place-items: center; width: 34px; height: 34px; border-radius: 50%; background: var(--ground); color: var(--dim); }
.engine-settings__error { font-size: 12.5px; font-weight: 600; color: var(--accent); }
</style>
