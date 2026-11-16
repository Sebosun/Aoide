<script setup lang="ts">
import { X } from "@lucide/vue";
import UiButton from "@/components/ui/UiButton.vue";
import UiDialog from "@/components/ui/UiDialog.vue";
import BookNarration from "@/components/book/BookNarration.vue";

/** Who reads the open book, as a dialog of its own over the reader (opened from its Narration
 *  button, or the margin's voice warning): its recording or a voice, with each one's settings. */
defineProps<{ bookId: string; title: string }>();
const emit = defineEmits<{ close: [] }>();
</script>

<template>
  <UiDialog label="Narration" return-focus='.reader__actions [aria-label="Narration"]' @close="emit('close')">
    <div class="narration-dialog">
      <header class="narration-dialog__head">
        <div class="narration-dialog__titles">
          <h1 class="narration-dialog__title">Narration</h1>
          <span class="narration-dialog__book" :title="title">{{ title }}</span>
        </div>
        <UiButton variant="glass" size="sm" icon aria-label="Close narration" @click="emit('close')">
          <X :size="17" />
        </UiButton>
      </header>
      <BookNarration :key="bookId" :book-id="bookId" />
    </div>
  </UiDialog>
</template>

<style scoped>
.narration-dialog {
  width: 560px;
  max-width: 100%;
  max-height: 86vh;
  display: flex;
  flex-direction: column;
  gap: var(--space-8);
  padding: var(--space-12);
  overflow-y: auto;
}
.narration-dialog__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-6);
}
.narration-dialog__titles {
  display: grid;
  gap: var(--space-1);
  min-width: 0;
}
.narration-dialog__title {
  margin: 0;
  font-size: 20px;
  font-weight: 700;
}
.narration-dialog__book {
  color: var(--dim);
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
