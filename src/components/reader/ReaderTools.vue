<script setup lang="ts">
import UiSegmented from "@/components/ui/UiSegmented.vue";
import UiButton from "@/components/ui/UiButton.vue";
import SayAgainButton from "@/components/reader/SayAgainButton.vue";
import SayDifferently from "@/components/reader/SayDifferently.vue";
import { ref } from "vue";
import type { ReaderMode } from "@/api/types";
import ReaderTypography from "@/components/reader/ReaderTypography.vue";
import { useMenu } from "@/composables/useMenu";
import { usePreferencesStore } from "@/stores/preferences";

/** Say this differently (while words are selected), Say this again (for the sentence being
 *  spoken), Text ("Aa": size, font, spacing, width, justification, hyphenation) and, with
 *  `lines`, the Page / Lines switch. */
defineProps<{ lines: boolean }>();
const preferences = usePreferencesStore();
const root = ref<HTMLElement>(),
  panel = ref<HTMLElement>(),
  button = ref<HTMLElement>();
const { open, onKeydown } = useMenu(root, panel, button);
const modes: { value: ReaderMode; label: string }[] = [
  { value: "page", label: "Page" },
  { value: "lines", label: "Lines" },
];
</script>

<template>
  <div class="reader-tools">
    <SayDifferently />
    <SayAgainButton />
    <div ref="root" class="reader-tools__size">
      <UiButton
        variant="glass"
        size="lg"
        icon
        ref="button"
        class="reader-tools__aa"
        aria-label="Text"
        title="Text: size, font, spacing, width"
        :aria-expanded="open"
        aria-haspopup="true"
        @click="open = !open"
      >
        Aa
      </UiButton>
      <div v-if="open" ref="panel" class="reader-tools__panel" role="group" aria-label="Text" @keydown="onKeydown">
        <ReaderTypography />
      </div>
    </div>
    <UiSegmented
      v-if="lines"
      label="Reader mode"
      size="md"
      chosen="page"
      :options="modes"
      :model-value="preferences.appearance.readerMode"
      @update:model-value="preferences.update({ readerMode: $event })"
    />
  </div>
</template>

<style scoped>
.reader-tools {
  display: flex;
  align-items: center;
  gap: var(--space-4);
}
.reader-tools__size {
  position: relative;
}
.reader-tools__panel {
  position: absolute;
  right: 0;
  top: calc(100% + 6px);
  z-index: 7;
  width: max-content;
  padding: var(--space-7) var(--space-8);
  border-radius: var(--radius-lg);
  background: var(--page);
  box-shadow:
    0 14px 34px -14px var(--shadow),
    0 0 0 1px var(--line);
}
</style>
