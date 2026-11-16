<script setup lang="ts">
import UiButton from "@/components/ui/UiButton.vue";
import { computed, onMounted, ref } from "vue";
import { FolderSearch, Library, Sparkles } from "@lucide/vue";
import { api } from "@/api";
import type { TaggingSettings } from "@/api/types";
import SettingsSection from "@/components/settings/SettingsSection.vue";
import SettingsField from "@/components/settings/SettingsField.vue";
import SettingsNote from "@/components/settings/SettingsNote.vue";
import { useAppSettings } from "@/composables/useAppSettings";
import { useLibraryStore } from "@/stores/library";
import { useNavigationStore } from "@/stores/navigation";

/** Where the books live, finding more, and online details. Toggles save at once. */
const library = useLibraryStore(),
  navigation = useNavigationStore();
const { settings, error, save } = useAppSettings();
onMounted(() => {
  void library.start();
});
const tagging = computed(() => settings.value?.tagging ?? null);
/** Saves a change to the tagging model's settings. */
const saveTagging = (change: Partial<TaggingSettings>) =>
  tagging.value && save({ tagging: { ...tagging.value, ...change } });
const tagRun = ref(""),
  tagging_ = ref(false);
/** Asks the model for the tags of every item that has none. */
async function tagUntagged() {
  tagRun.value = "";
  tagging_.value = true;
  try {
    const run = await api.tagUntaggedWithModel();
    const items = (n: number) => `${n} ${n === 1 ? "item" : "items"}`;
    tagRun.value = `Tagged ${items(run.tagged)}${run.missed ? `; the model gave nothing for ${items(run.missed)}` : ""}.`;
  } catch (e) {
    tagRun.value = String(e);
  } finally {
    tagging_.value = false;
  }
}
function findBooks() {
  navigation.closeSettings();
  navigation.view = "find";
}
</script>

<template>
  <div class="library-settings">
    <SettingsSection title="Library" :icon="Library">
      <SettingsField label="Library folder" as="div"
        ><code class="library-settings__path">{{ library.folder || "…" }}</code></SettingsField
      >
      <SettingsNote
        >Books you drop on the window or import are copied here. Aoide never changes the EPUB files.</SettingsNote
      >
      <div>
        <UiButton size="sm" class="library-settings__button" @click="findBooks">
          <FolderSearch :size="15" /> Find books on this computer…
        </UiButton>
      </div>
    </SettingsSection>
    <SettingsSection title="Book details">
      <label v-if="settings" class="library-settings__check"
        ><input
          :checked="settings.metadataLookup"
          type="checkbox"
          aria-label="Look up book details online"
          @change="save({ metadataLookup: ($event.target as HTMLInputElement).checked })"
        />
        Look up book details on Open Library</label
      >
      <SettingsNote
        >Only the title, authors and ISBN are sent, and only when you ask for details. Nothing changes until you pick a
        match.</SettingsNote
      >
    </SettingsSection>
    <SettingsSection title="Tags" :icon="Sparkles" aria-label="Tagging with a model">
      <label v-if="tagging" class="library-settings__check"
        ><input
          :checked="tagging.enabled"
          type="checkbox"
          aria-label="Tag items with a model on this computer"
          @change="saveTagging({ enabled: ($event.target as HTMLInputElement).checked })"
        />
        Tag items with a language model on this computer</label
      >
      <template v-if="tagging?.enabled">
        <SettingsField label="Model server"
          ><input
            class="library-settings__field"
            :value="tagging.baseUrl"
            aria-label="Model server address"
            placeholder="http://127.0.0.1:11434"
            @change="saveTagging({ baseUrl: ($event.target as HTMLInputElement).value.trim() })"
        /></SettingsField>
        <SettingsField label="Model"
          ><input
            class="library-settings__field"
            :value="tagging.model"
            aria-label="Model to tag with"
            placeholder="qwen2.5:3b"
            @change="saveTagging({ model: ($event.target as HTMLInputElement).value.trim() })"
        /></SettingsField>
        <div class="library-settings__row">
          <UiButton
            size="sm"
            aria-label="Tag the items that have no tags"
            :disabled="tagging_ || !tagging.model"
            @click="tagUntagged"
          >
            <Sparkles :size="15" /> {{ tagging_ ? "Tagging…" : "Tag untagged items" }}
          </UiButton>
          <span v-if="tagRun" class="library-settings__result" role="status">{{ tagRun }}</span>
        </div>
      </template>
      <SettingsNote
        >An OpenAI-compatible server such as Ollama, llama.cpp or LM Studio, on this computer. Each item's title,
        authors and description (or its first pages) are sent to it; it answers with a few tags, the ones you use
        preferred. A tag you remove stays removed.</SettingsNote
      >
    </SettingsSection>
    <SettingsSection title="Listening">
      <label v-if="settings" class="library-settings__check"
        ><input
          :checked="settings.stepBackOnResume"
          type="checkbox"
          aria-label="Step back a little when playing after a break"
          @change="save({ stepBackOnResume: ($event.target as HTMLInputElement).checked })"
        />
        Step back a little when playing after a break</label
      >
      <SettingsNote
        >After ten seconds or more, playback starts again from the start of the sentence; after five minutes, a
        sentence earlier; after an hour, two. An audiobook without text goes back 3, 10 or 20 seconds.</SettingsNote
      >
    </SettingsSection>
    <SettingsSection title="Preparing audio">
      <label v-if="settings" class="library-settings__check"
        ><input
          :checked="settings.resumePrepare"
          type="checkbox"
          aria-label="Continue preparing books when Aoide starts"
          @change="save({ resumePrepare: ($event.target as HTMLInputElement).checked })"
        />
        Continue preparing books when Aoide starts</label
      >
      <SettingsNote
        >When this is off, books still waiting stay in the queue and start again once you play something or prepare
        another book.</SettingsNote
      >
    </SettingsSection>
    <p v-if="error" class="library-settings__error" role="alert">{{ error }}</p>
  </div>
</template>

<style scoped>
.library-settings {
  display: grid;
  gap: var(--space-11);
}
.library-settings__path {
  padding: var(--space-4) var(--space-6);
  border-radius: var(--radius-md);
  background: var(--ground);
  font-size: 12.5px;
  overflow-wrap: anywhere;
}
.library-settings__check {
  display: flex;
  align-items: center;
  gap: var(--space-5);
  font-size: 13px;
}
.library-settings__field {
  width: 100%;
  height: var(--control-sm);
  padding: 0 var(--space-5);
  border-radius: var(--radius-md);
  background: var(--ground);
  color: var(--ink);
  font-size: 13px;
}
.library-settings__row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-5);
}
.library-settings__result {
  font-size: 12.5px;
  color: var(--dim);
}
.library-settings__error {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--accent);
}
</style>
