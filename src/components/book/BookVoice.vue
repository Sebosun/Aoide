<script setup lang="ts">
import UiButton from "@/components/ui/UiButton.vue";
import { computed, onMounted, onUnmounted, ref } from "vue";
import { Download, RotateCcw } from "@lucide/vue";
import { api } from "@/api";
import type { BookSettings, EngineDescriptor, EngineProfile, ExportProgress, ParamValue, Voice } from "@/api/types";
import ParamControl from "@/components/ParamControl.vue";
import UiSelect from "@/components/ui/UiSelect.vue";
import { voiceOptions } from "@/components/ui/voiceOptions";
import VoicePreview from "@/components/settings/VoicePreview.vue";
import { useSample } from "@/composables/useVoicePreview";
import PronunciationList from "@/components/settings/PronunciationList.vue";
import { useLibraryStore } from "@/stores/library";
import { useReaderStore } from "@/stores/reader";
import { useUiStore } from "@/stores/ui";
import { formatBytes } from "@/bytes";
import {
  ACTION_ICONS,
  ACTION_LABELS,
  describePreparation,
  preparationStatus,
  runPrepareAction,
  type PrepareAction,
} from "@/preparation";
const props = defineProps<{ bookId: string }>();
const library = useLibraryStore(),
  reader = useReaderStore();
const settings = ref<BookSettings | null>(null),
  profile = ref<EngineProfile | null>(null),
  descriptor = ref<EngineDescriptor | null>(null);
const voices = ref<Voice[] | null>(null);
const message = ref(""),
  error = ref(""),
  exporting = ref<ExportProgress | null>(null);
const summary = computed(() => library.books.find((b) => b.id === props.bookId));
let unlisten: (() => void) | undefined,
  disposed = false;

onMounted(async () => {
  void library.start(); // Its summary gives the audio size, preparation and language.
  try {
    const [book, options] = await Promise.all([api.bookSettings({ bookId: props.bookId }), api.engineOptions()]);
    settings.value = book;
    profile.value = options.profile;
    descriptor.value = options.descriptors.find((d) => d.kind === options.profile.kind) ?? null;
  } catch (e) {
    error.value = String(e);
  }
  try {
    voices.value = await api.engineVoices();
  } catch {
    voices.value = null;
  }
  const off = await api.onExportProgress((p) => {
    if (p.bookId === props.bookId) exporting.value = p;
  });
  if (disposed) off();
  else unlisten = off;
});
onUnmounted(() => {
  disposed = true;
  unlisten?.();
});

function setParam(key: string, value: ParamValue | undefined) {
  if (!settings.value) return;
  const params = { ...settings.value.params };
  if (value === undefined) delete params[key];
  else params[key] = value;
  settings.value.params = params;
}
async function save() {
  if (!settings.value) return;
  message.value = "";
  error.value = "";
  const next: BookSettings = {
    ...settings.value,
    voiceId: settings.value.voiceId?.trim() || null,
    language: settings.value.language?.trim() || null,
  };
  try {
    await api.saveBookSettings({ bookId: props.bookId, settings: next });
    settings.value = next;
    message.value = "Saved. Audio from the playhead on uses these settings.";
    if (reader.book?.id === props.bookId)
      reader.cache = Object.fromEntries(
        Object.entries(reader.cache).filter(([key]) => !key.startsWith(`${props.bookId}:`)),
      );
    if (reader.book?.id === props.bookId) await reader.loadChapter(reader.currentChapter);
  } catch (e) {
    error.value = String(e);
  }
}
const ui = useUiStore();
async function regenerate() {
  const ok = await ui.confirm({
    title: "Regenerate this book's audio?",
    body: "Aoide deletes the audio made so far and will generate it again with the current voice.",
    confirmLabel: "Regenerate",
    danger: true,
  });
  if (ok) await run(() => api.regenerateBook({ bookId: props.bookId }));
}
async function run(action: () => Promise<unknown>) {
  error.value = "";
  try {
    await action();
    await library.refresh();
  } catch (e) {
    error.value = String(e);
  }
}
async function exportBook() {
  error.value = "";
  exporting.value = { bookId: props.bookId, stage: "generating", done: 0, total: 0, path: null, error: null };
  try {
    const path = await api.exportBook({ bookId: props.bookId });
    exporting.value = { ...exporting.value!, stage: "done", path };
  } catch (e) {
    exporting.value = { ...exporting.value!, stage: "error", error: String(e) };
  }
}
/** Both voices as the book will have them: a narration sentence, then a quoted one, from it. */
const dialogueSample = useSample();
function previewDialogue() {
  const s = settings.value;
  if (!s?.dialogueVoiceId || !profile.value) return;
  const request = {
    bookId: props.bookId,
    profile: profile.value,
    voiceId: s.voiceId?.trim() || null,
    dialogueVoiceId: s.dialogueVoiceId,
    params: s.params,
    language: s.language?.trim() || summary.value?.language || null,
  };
  void dialogueSample.play(null, () => api.previewDialogue(request));
}
/** The engine may load each voice anew, so switching for every quote can be slow. */
const slowSwitch = computed(() => descriptor.value !== null && !descriptor.value.cheapVoiceSwitch);
const preparation = computed(() => preparationStatus(summary.value?.prepare ?? null));
async function prepareAction(action: PrepareAction) {
  await runPrepareAction(props.bookId, action);
  await library.refresh();
}
</script>

<template>
  <p v-if="error" class="book-voice__error" role="alert">{{ error }}</p>
  <form v-if="settings" class="book-voice__form" @submit.prevent="save">
    <h3 class="book-voice__heading">Voice for this book</h3>
    <p class="book-voice__hint">Overrides {{ profile?.name ?? "the engine profile" }} for this book only.</p>
    <div class="book-voice__field">
      <span>Voice</span>
      <UiSelect
        v-if="voices"
        label="Voice for this book"
        :options="
          voiceOptions(voices, settings.voiceId, `Profile default${profile?.voiceId ? ` (${profile.voiceId})` : ''}`)
        "
        :model-value="settings.voiceId ?? ''"
        @update:model-value="(id) => (settings!.voiceId = id || null)"
      />
      <input
        v-else
        :value="settings.voiceId ?? ''"
        type="text"
        aria-label="Voice for this book"
        placeholder="Profile default"
        @input="settings.voiceId = ($event.target as HTMLInputElement).value"
      />
    </div>
    <label class="book-voice__field"
      ><span>Language</span
      ><input
        :value="settings.language ?? ''"
        type="text"
        aria-label="Language"
        :placeholder="summary?.language ?? 'From the book'"
        spellcheck="false"
        @input="settings.language = ($event.target as HTMLInputElement).value"
    /></label>
    <VoicePreview
      v-if="profile"
      :request="{
        profile,
        voiceId: settings.voiceId?.trim() || null,
        params: settings.params,
        language: settings.language?.trim() || summary?.language || null,
      }"
    />
    <div class="book-voice__field">
      <span>Voice for dialogue</span>
      <UiSelect
        v-if="voices"
        label="Voice for dialogue"
        :options="voiceOptions(voices, settings.dialogueVoiceId, 'The same voice')"
        :model-value="settings.dialogueVoiceId ?? ''"
        @update:model-value="(id) => (settings!.dialogueVoiceId = id || null)"
      />
      <input
        v-else
        :value="settings.dialogueVoiceId ?? ''"
        type="text"
        aria-label="Voice for dialogue"
        placeholder="The same voice"
        @input="settings.dialogueVoiceId = ($event.target as HTMLInputElement).value.trim() || null"
      />
    </div>
    <template v-if="settings.dialogueVoiceId">
      <p class="book-voice__hint">
        What characters say in quotation marks is read in this voice, the narration in the one above. Saving segments
        the book anew: sentences with speech in them are made again, the rest keep their audio.
      </p>
      <p v-if="slowSwitch" class="book-voice__warning" role="status">
        {{ descriptor?.name }} may load each voice anew, so switching between two can make narration slow.
      </p>
      <div class="book-voice__row">
        <UiButton
          size="sm"
          aria-label="Preview both voices"
          :disabled="dialogueSample.state.value === 'loading'"
          @click="previewDialogue"
        >
          {{ dialogueSample.state.value === "loading" ? "Preparing…" : "Preview both voices" }}
        </UiButton>
        <span v-if="dialogueSample.error.value" class="book-voice__error" role="alert">{{
          dialogueSample.error.value
        }}</span>
      </div>
    </template>
    <label class="book-voice__check"
      ><input v-model="settings.readNotes" type="checkbox" aria-label="Read footnotes aloud" /> Read footnotes aloud
      after their paragraph</label
    >
    <ParamControl
      v-for="spec in descriptor?.params ?? []"
      :key="spec.key"
      :spec="spec"
      :model-value="settings.params[spec.key] ?? profile?.params[spec.key]"
      @update:model-value="setParam(spec.key, $event)"
    />
    <div class="book-voice__actions">
      <UiButton variant="accent" size="sm" type="submit" class="book-voice__primary">Save</UiButton
      ><span v-if="message" class="book-voice__ok" role="status">{{ message }}</span>
    </div>
  </form>

  <section class="book-voice__words">
    <h3 class="book-voice__heading">Pronunciation</h3>
    <p class="book-voice__hint">
      How to say a word of this book: “Hermione” as “her-MY-oh-nee”. The page keeps its spelling; only the sentences
      with the word are made again. This list wins over the one for all books in Settings › Speech engine.
    </p>
    <PronunciationList :book-id="bookId" />
  </section>

  <section class="book-voice__audio">
    <h3 class="book-voice__heading">Audio</h3>
    <p class="book-voice__hint">
      {{
        summary?.audioBytes
          ? `${formatBytes(summary.audioBytes)} generated for this book.`
          : "No audio generated for this book yet."
      }}
    </p>
    <template v-if="preparation.label && preparation.state !== 'done'">
      <progress :value="preparation.percent" max="100" aria-label="Preparation progress" />
      <p class="book-voice__hint">
        {{ describePreparation(preparation)
        }}<template v-if="preparation.count"> segments ({{ preparation.percent }}%)</template>
      </p>
    </template>
    <p v-else-if="preparation.state === 'done'" class="book-voice__ok">The whole book is ready to play offline.</p>
    <div v-if="preparation.actions.length" class="book-voice__actions">
      <UiButton
        size="sm"
        v-for="action in preparation.actions"
        :key="action"
        class="book-voice__button"
        :aria-label="ACTION_LABELS[action]"
        @click="prepareAction(action)"
      >
        <component :is="ACTION_ICONS[action]" :size="14" /> {{ ACTION_LABELS[action] }}
      </UiButton>
    </div>

    <button type="button" class="book-voice__button" aria-label="Regenerate audio" @click="regenerate">
      <RotateCcw :size="14" /> Regenerate audio</button
    ><UiButton size="sm" class="book-voice__button" aria-label="Regenerate audio" @click="regenerate">
      <RotateCcw :size="14" /> Regenerate audio
    </UiButton>

    <UiButton
      size="sm"
      class="book-voice__button"
      aria-label="Export audiobook"
      :disabled="exporting?.stage === 'generating' || exporting?.stage === 'encoding'"
      @click="exportBook"
    >
      <Download :size="14" /> Export as M4B audiobook
    </UiButton>
    <template v-if="exporting">
      <template v-if="exporting.stage === 'generating' || exporting.stage === 'encoding'">
        <progress :value="exporting.done" :max="Math.max(1, exporting.total)" aria-label="Export progress" />
        <p class="book-voice__hint">
          {{ exporting.stage === "generating" ? "Generating audio" : "Encoding chapters" }} · {{ exporting.done }} /
          {{ exporting.total }}
        </p>
      </template>
      <p v-else-if="exporting.stage === 'done'" class="book-voice__ok">
        Saved to <code>{{ exporting.path }}</code>
      </p>
      <p v-else class="book-voice__error" role="alert">{{ exporting.error }}</p>
    </template>
  </section>
</template>

<style scoped>
.book-voice__warning {
  color: var(--warn);
  font-size: 12.5px;
  font-weight: 600;
}
.book-voice__row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-4);
}
.book-voice__form,
.book-voice__words,
.book-voice__audio {
  display: grid;
  gap: var(--space-6);
}
.book-voice__words,
.book-voice__audio {
  margin-top: var(--space-14);
  padding-top: var(--space-11);
  border-top: 1px solid var(--line);
  justify-items: start;
}
.book-voice__words {
  justify-items: stretch;
}
.book-voice__audio progress {
  width: 100%;
  height: 5px;
}
.book-voice__heading {
  font-size: 14px;
  margin: 0;
}
.book-voice__hint {
  font-size: 11px;
  color: var(--dim);
  margin: calc(-1 * var(--space-2)) 0 0;
}
.book-voice__field {
  display: grid;
  grid-template-columns: 90px minmax(0, 1fr);
  gap: var(--space-5);
  align-items: center;
  font-size: 13px;
}
.book-voice__field span {
  color: var(--dim);
}
.book-voice__check {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  font-size: 13px;
}
.book-voice__actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-5);
}
.book-voice__ok {
  font-size: 12px;
  color: var(--accent);
  overflow-wrap: anywhere;
}
.book-voice__error {
  font-size: 12px;
  color: var(--accent);
  overflow-wrap: anywhere;
}
</style>
