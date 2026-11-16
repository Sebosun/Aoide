<script setup lang="ts">
import { SHORTCUTS, keyLabels } from "@/shortcuts";

/** The keyboard shortcuts as a list of keys and what they do (Settings › About, and the ? sheet). */
</script>

<template>
  <dl class="shortcut-list" aria-label="Keyboard shortcuts">
    <div v-for="shortcut in SHORTCUTS" :key="shortcut.id" class="shortcut-list__row">
      <dt class="shortcut-list__keys">
        <template v-for="(combo, i) in shortcut.keys" :key="i"
          ><span v-if="i" class="shortcut-list__or">or</span
          ><kbd v-for="key in keyLabels(combo)" :key="key" class="shortcut-list__key">{{ key }}</kbd></template
        >
      </dt>
      <dd class="shortcut-list__does">{{ shortcut.label }}</dd>
    </div>
  </dl>
</template>

<style scoped>
.shortcut-list {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  font-size: 13px;
}
.shortcut-list__row {
  display: grid;
  grid-template-columns: 180px minmax(0, 1fr);
  align-items: center;
  gap: var(--space-6);
  padding: var(--space-2) 0;
}
.shortcut-list__keys {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
}
.shortcut-list__does {
  margin: 0;
  color: var(--dim);
  font-weight: 600;
}
.shortcut-list__key {
  min-width: 26px;
  padding: var(--space-1) var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--ground);
  box-shadow: inset 0 -1px 0 var(--line);
  font: 700 12px/1.4 var(--font-ui);
  text-align: center;
}
.shortcut-list__or {
  margin: 0 var(--space-1);
  font-size: 12px;
  color: var(--faint);
}
</style>
