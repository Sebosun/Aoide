<script setup lang="ts">
import { Info, TriangleExclamationPoint } from "@lucide/vue";
/** A one-line explanation under a setting; `warn` for something the person should weigh. */
withDefaults(defineProps<{ tone?: "info" | "warn" }>(), { tone: "info" });
</script>

<template>
  <p class="settings-note" :class="`settings-note--${tone}`" role="note">
    <TriangleExclamationPoint v-if="tone === 'warn'" :size="15" aria-hidden="true" /><Info v-else :size="15" aria-hidden="true" />
    <span><slot /></span>
  </p>
</template>

<style scoped>
.settings-note { display: flex; gap: 8px; align-items: flex-start; font-size: 12.5px; line-height: 1.5; color: var(--dim); }
.settings-note svg { margin-top: 1px; color: var(--faint); }
.settings-note--warn { padding: 10px 12px; border-radius: 12px; background: var(--wash); color: var(--warn); font-weight: 600; }
.settings-note--warn svg { color: var(--warn); }
.settings-note :deep(code) { font-size: 12px; overflow-wrap: anywhere; }
</style>
