<script setup lang="ts">
import { Upload } from "@lucide/vue";
</script>

<template>
  <div class="drop-overlay" aria-hidden="true">
    <span class="drop-overlay__icon"><Upload :size="30" /></span>
    <strong class="drop-overlay__title">Drop to add to your library</strong>
    <span class="drop-overlay__hint">EPUB files are added to your library</span>
  </div>
</template>

<style scoped>
.drop-overlay { position: fixed; inset: 14px; z-index: 20; display: grid; place-content: center; justify-items: center; gap: 10px; border: 2px dashed var(--accent); border-radius: 22px; background: var(--glass); backdrop-filter: blur(6px); pointer-events: none; text-align: center; }
.drop-overlay__icon { width: 64px; height: 64px; display: grid; place-items: center; border-radius: 50%; background: var(--accent); color: var(--on-accent); }
.drop-overlay__title { font-size: 20px; font-weight: 800; letter-spacing: -.02em; }
.drop-overlay__hint { color: var(--dim); }
</style>
