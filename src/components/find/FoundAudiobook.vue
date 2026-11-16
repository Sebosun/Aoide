<script setup lang="ts">
import { Folder, Headphones } from "@lucide/vue";
import type { AudiobookSuggestion } from "@/api/types";
import { holds, nameOf } from "@/audiobookFinds";
import UiButton from "@/components/ui/UiButton.vue";

/** One audiobook found that matches no book, with a button to add it on its own. */
defineProps<{ found: AudiobookSuggestion; disabled: boolean }>();
const emit = defineEmits<{ add: [] }>();
</script>

<template>
  <li class="found-audiobook" :title="found.files.join('\n')">
    <component :is="found.folder ? Folder : Headphones" :size="14" class="found-audiobook__kind" aria-hidden="true" />
    <span class="found-audiobook__label"
      ><span class="found-audiobook__name">{{ nameOf(found) }}</span
      ><span class="found-audiobook__meta">{{ holds(found) }}</span></span
    >
    <UiButton size="sm" :aria-label="`Add ${nameOf(found)} on its own`" :disabled="disabled" @click="emit('add')">
      Add
    </UiButton>
  </li>
</template>

<style scoped>
.found-audiobook {
  display: flex;
  align-items: center;
  gap: var(--space-5);
  padding: var(--space-5) var(--space-6);
  border-radius: var(--radius-md);
  background: var(--page);
}
.found-audiobook__kind {
  flex: none;
  color: var(--dim);
}
.found-audiobook__label {
  flex: 1;
  display: grid;
  min-width: 0;
}
.found-audiobook__name {
  overflow-wrap: anywhere;
  color: var(--ink);
}
.found-audiobook__meta {
  font-size: 11px;
  font-weight: 600;
  color: var(--faint);
}
</style>
