<script setup lang="ts">
import UiButton from "@/components/ui/UiButton.vue";
import { onMounted } from "vue";
import { FolderSearch, Library } from "@lucide/vue";
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
.library-settings__error {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--accent);
}
</style>
