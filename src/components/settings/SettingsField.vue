<script setup lang="ts">
/** A labelled settings row: the label on the left, the control (and any small buttons after
 *  it) on the right; the first control takes the free width. */
withDefaults(defineProps<{ label: string; sub?: string; top?: boolean; as?: "label" | "div" }>(), { as: "label" });
</script>

<template>
  <component :is="as" class="settings-field" :class="{ 'settings-field--top': top }">
    <span class="settings-field__label">{{ label }}<small v-if="sub">{{ sub }}</small></span>
    <span class="settings-field__control"><slot /></span>
  </component>
</template>

<style scoped>
.settings-field { display: grid; grid-template-columns: 160px minmax(0, 1fr); align-items: center; gap: 12px; font-size: 13px; }
.settings-field--top { align-items: start; }
.settings-field__label { display: grid; color: var(--dim); font-weight: 600; }
.settings-field__label small { color: var(--faint); font-weight: 500; }
.settings-field__control { display: grid; grid-auto-flow: column; grid-template-columns: minmax(0, 1fr); grid-auto-columns: auto; align-items: center; gap: 8px; min-width: 0; }
@media (max-width: 560px) { .settings-field { grid-template-columns: minmax(0, 1fr); gap: 6px; } }
</style>
