<script setup lang="ts">
/** The submit button of a settings form, with its result. */
withDefaults(defineProps<{ label?: string; busy?: boolean; message?: string; error?: string }>(), { label: "Save", busy: false, message: "", error: "" });
</script>

<template>
  <div class="settings-actions">
    <button class="settings-actions__save" type="submit" :disabled="busy">{{ label }}</button>
    <slot />
    <span v-if="message" class="settings-actions__ok" role="status">{{ message }}</span>
    <span v-if="error" class="settings-actions__error" role="alert">{{ error }}</span>
  </div>
</template>

<style scoped>
.settings-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; }
.settings-actions__save { height: 38px; padding: 0 20px; border-radius: 999px; background: var(--accent); color: var(--on-accent); font-weight: 800; }
.settings-actions__ok { font-size: 12.5px; font-weight: 600; color: var(--ok); }
.settings-actions__error { font-size: 12.5px; font-weight: 600; color: var(--accent); overflow-wrap: anywhere; }
</style>
