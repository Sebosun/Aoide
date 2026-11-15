<script setup lang="ts">
import UiSegmented from "@/components/ui/UiSegmented.vue";
import UiButton from "@/components/ui/UiButton.vue";
import { ref } from "vue";
import { Minus, Plus } from "@lucide/vue";
import type { ReaderMode } from "@/api/types";
import { TEXT_SCALES } from "@/theme";
import { useMenu } from "@/composables/useMenu";
import { usePreferencesStore } from "@/stores/preferences";

/** Text size ("Aa") and, with `lines`, the Page / Lines switch. */
defineProps<{ lines: boolean }>();
const preferences = usePreferencesStore();
const root = ref<HTMLElement>(),
  panel = ref<HTMLElement>(),
  button = ref<HTMLElement>();
const { open, onKeydown } = useMenu(root, panel, button);
const size = (delta: number) =>
  preferences.update({
    textSize: Math.min(TEXT_SCALES.length - 1, Math.max(0, preferences.appearance.textSize + delta)),
  });
const modes: { value: ReaderMode; label: string }[] = [
  { value: "page", label: "Page" },
  { value: "lines", label: "Lines" },
];
</script>

<template>
  <div class="reader-tools">
    <div ref="root" class="reader-tools__size">
      <UiButton
        variant="glass"
        size="lg"
        icon
        ref="button"
        class="reader-tools__aa"
        aria-label="Text size"
        title="Text size"
        :aria-expanded="open"
        aria-haspopup="true"
        @click="open = !open"
      >
        Aa
      </UiButton>
      <div v-if="open" ref="panel" class="reader-tools__panel" role="group" aria-label="Text size" @keydown="onKeydown">
        <UiButton
          size="sm"
          icon
          class="reader-tools__step"
          aria-label="Smaller text"
          :disabled="preferences.appearance.textSize <= 0"
          @click="size(-1)"
        >
          <Minus :size="15" />
        </UiButton>
        <span class="reader-tools__dots" aria-hidden="true"
          ><i
            v-for="(_, i) in TEXT_SCALES"
            :key="i"
            :class="{ 'reader-tools__dot--on': i === preferences.appearance.textSize }"
            class="reader-tools__dot"
        /></span>
        <UiButton
          size="sm"
          icon
          class="reader-tools__step"
          aria-label="Larger text"
          :disabled="preferences.appearance.textSize >= TEXT_SCALES.length - 1"
          @click="size(1)"
        >
          <Plus :size="15" />
        </UiButton>
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
  display: flex;
  align-items: center;
  gap: var(--space-5);
  padding: var(--space-3);
  border-radius: var(--radius-pill);
  background: var(--page);
  box-shadow:
    0 14px 34px -14px var(--shadow),
    0 0 0 1px var(--line);
}
.reader-tools__dots {
  display: flex;
  gap: var(--space-2);
}
.reader-tools__dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--line);
}
.reader-tools__dot--on {
  background: var(--accent);
}
</style>
