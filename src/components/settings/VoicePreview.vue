<script setup lang="ts">
import UiButton from "@/components/ui/UiButton.vue";
import { LoaderCircle, Square, Volume2 } from "@lucide/vue";
import { useVoicePreview, type PreviewRequest } from "@/composables/useVoicePreview";

/** "Preview" next to a voice picker: speaks a sample sentence with the settings as they are on
 *  screen, saved or not. */
const props = defineProps<{ request: PreviewRequest }>();
const { state, error, play, stop } = useVoicePreview();
</script>

<template>
  <div class="voice-preview">
    <UiButton
      size="sm"
      v-if="state === 'idle'"
      class="voice-preview__button"
      aria-label="Preview voice"
      @click="play(props.request)"
    >
      <Volume2 :size="15" /> Preview voice
    </UiButton>
    <UiButton
      variant="primary"
      size="sm"
      v-else
      class="voice-preview__button voice-preview__button--on"
      aria-label="Stop preview"
      @click="stop"
    >
      <LoaderCircle v-if="state === 'loading'" :size="15" class="voice-preview__spin" /><Square
        v-else
        :size="13"
        fill="currentColor"
      />
      {{ state === "loading" ? "Preparing…" : "Stop" }}
    </UiButton>
    <span v-if="error" class="voice-preview__error" role="alert">Preview failed: {{ error }}</span>
    <span v-else-if="state === 'loading'" class="voice-preview__hint"
      >The engine speaks one request at a time, so this may wait for audio being generated.</span
    >
    <span v-else class="voice-preview__hint">Hear a sample with the settings on screen, saved or not.</span>
  </div>
</template>

<style scoped>
.voice-preview {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-4) var(--space-6);
}
.voice-preview__spin {
  animation: voice-preview-spin 1s linear infinite;
}
@keyframes voice-preview-spin {
  to {
    transform: rotate(1turn);
  }
}
.voice-preview__hint {
  font-size: 12px;
  color: var(--dim);
}
.voice-preview__error {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--accent);
  overflow-wrap: anywhere;
}
</style>
