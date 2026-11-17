<script setup lang="ts">
import UiSegmented from "@/components/ui/UiSegmented.vue";
import UiButton from "@/components/ui/UiButton.vue";
import SayAgainButton from "@/components/reader/SayAgainButton.vue";
import SayDifferently from "@/components/reader/SayDifferently.vue";
import { nextTick, onUnmounted, ref, watch } from "vue";
import type { ReaderMode } from "@/api/types";
import ReaderTypography from "@/components/reader/ReaderTypography.vue";
import { useMenu } from "@/composables/useMenu";
import { rootZoom } from "@/composables/usePopover";
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
/** The panel hangs from Aa's right edge; in a narrow window it is wider than the room left of
 *  Aa, so it moves right to stay inside the window. */
const shift = ref(0);
function place() {
  const box = root.value?.getBoundingClientRect();
  if (!box || !open.value) return;
  const margin = 8,
    zoom = rootZoom(),
    left = box.right - (panel.value?.offsetWidth ?? 0) * zoom;
  shift.value = left < margin ? Math.round((margin - left) / zoom) : 0;
}
watch(open, async (value) => {
  window.removeEventListener("resize", place);
  shift.value = 0;
  if (!value) return;
  await nextTick();
  place();
  window.addEventListener("resize", place);
});
onUnmounted(() => window.removeEventListener("resize", place));
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
      <div
        v-if="open"
        ref="panel"
        class="reader-tools__panel"
        role="group"
        aria-label="Text"
        :style="shift ? { transform: `translateX(${shift}px)` } : undefined"
        @keydown="onKeydown"
      >
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
