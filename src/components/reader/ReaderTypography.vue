<script setup lang="ts">
import UiSegmented from "@/components/ui/UiSegmented.vue";
import ReaderSteps from "@/components/reader/ReaderSteps.vue";
import type { ReaderFont } from "@/api/types";
import { LINE_HEIGHTS, TEXT_SCALES, TEXT_WIDTHS } from "@/theme";
import { usePreferencesStore } from "@/stores/preferences";

/** The reader's typography, as the reader's Text popover and Settings › Appearance show it:
 *  text size, font, line spacing, column width, justification and hyphenation. Each change is
 *  saved at once. A parent lines its labels up with `--reader-typography-label`. */
withDefaults(defineProps<{ tone?: "glass" | "ground" }>(), { tone: "glass" });
const preferences = usePreferencesStore();
const fonts: { value: ReaderFont; label: string }[] = [
  { value: "serif", label: "Serif" },
  { value: "sans", label: "Sans" },
  { value: "legible", label: "Legible" },
  { value: "system", label: "System" },
];
const titles: Record<ReaderFont, string> = {
  serif: "Newsreader",
  sans: "Manrope, the interface font",
  legible: "Atkinson Hyperlegible, made to tell letters apart",
  system: "Your desktop's font",
};
</script>

<template>
  <div class="reader-typography">
    <span class="reader-typography__label">Size</span>
    <ReaderSteps
      label="Text size"
      less="Smaller text"
      more="Larger text"
      :count="TEXT_SCALES.length"
      :model-value="preferences.appearance.textSize"
      @update:model-value="preferences.update({ textSize: $event })"
    />
    <span class="reader-typography__label">Font</span>
    <UiSegmented
      class="reader-typography__fonts"
      label="Reader font"
      :tone="tone"
      :options="fonts"
      :model-value="preferences.appearance.readerFont"
      @update:model-value="preferences.update({ readerFont: $event })"
    >
      <template #option="{ option }"
        ><span :title="titles[option.value]">{{ option.label }}</span></template
      >
    </UiSegmented>
    <span class="reader-typography__label">Spacing</span>
    <ReaderSteps
      label="Line spacing"
      less="Less line spacing"
      more="More line spacing"
      :count="LINE_HEIGHTS.length"
      :model-value="preferences.appearance.lineSpacing"
      @update:model-value="preferences.update({ lineSpacing: $event })"
    />
    <span class="reader-typography__label">Width</span>
    <ReaderSteps
      label="Text width"
      less="Narrower text"
      more="Wider text"
      :count="TEXT_WIDTHS.length"
      :model-value="preferences.appearance.textWidth"
      @update:model-value="preferences.update({ textWidth: $event })"
    />
    <span class="reader-typography__checks">
      <label class="reader-typography__check"
        ><input
          type="checkbox"
          aria-label="Justify"
          :checked="preferences.appearance.justify"
          @change="preferences.update({ justify: ($event.target as HTMLInputElement).checked })"
        />
        Justify</label
      >
      <label class="reader-typography__check" title="In the book's language, where your system has its dictionary"
        ><input
          type="checkbox"
          aria-label="Hyphenate"
          :checked="preferences.appearance.hyphenate"
          @change="preferences.update({ hyphenate: ($event.target as HTMLInputElement).checked })"
        />
        Hyphenate</label
      >
    </span>
  </div>
</template>

<style scoped>
.reader-typography {
  display: grid;
  grid-template-columns: var(--reader-typography-label, auto) minmax(0, 1fr);
  align-items: center;
  justify-items: start;
  gap: var(--space-5) var(--space-6);
  font-size: 13px;
}
.reader-typography__label {
  color: var(--dim);
  font-weight: 600;
}
.reader-typography__checks {
  grid-column: 2;
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-4) var(--space-9);
}
.reader-typography__check {
  display: inline-flex;
  align-items: center;
  gap: var(--space-3);
  font-weight: 600;
}
</style>
