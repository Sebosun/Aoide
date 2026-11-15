<script setup lang="ts">
import type { Component } from "vue";
/** One titled group of settings inside a settings tab. */
defineProps<{ title: string; icon?: Component }>();
</script>

<template>
  <section class="settings-section">
    <header class="settings-section__head">
      <h2 class="settings-section__title">
        <component :is="icon" v-if="icon" :size="17" aria-hidden="true" /> {{ title }}
      </h2>
      <slot name="aside" />
    </header>
    <div class="settings-section__body"><slot /></div>
  </section>
</template>

<style scoped>
.settings-section {
  display: grid;
  gap: var(--space-7);
}
.settings-section + .settings-section {
  padding-top: var(--space-11);
  border-top: 1px solid var(--line);
}
.settings-section__head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-5);
}
.settings-section__title {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  font-size: 16px;
  font-weight: 800;
  letter-spacing: -0.01em;
}
.settings-section__body {
  display: grid;
  gap: var(--space-7);
}
</style>
