<script setup lang="ts">
import { X } from "@lucide/vue";
import { useUiStore } from "../../stores/ui";
const ui = useUiStore();
</script>

<template>
  <div class="toasts" role="status" aria-live="polite">
    <TransitionGroup name="toasts__item">
      <p v-for="toast in ui.toasts" :key="toast.id" class="toasts__item" :class="`toasts__item--${toast.tone}`">
        <span>{{ toast.text }}</span>
        <button type="button" class="toasts__close" aria-label="Dismiss" @click="ui.dismiss(toast.id)"><X :size="14" /></button>
      </p>
    </TransitionGroup>
  </div>
</template>

<style scoped>
.toasts { position: fixed; z-index: 50; left: 50%; top: 72px; translate: -50% 0; display: grid; justify-items: center; gap: 8px; pointer-events: none; }
.toasts__item { display: inline-flex; align-items: center; gap: 10px; max-width: min(460px, calc(100vw - 32px)); padding: 9px 10px 9px 16px; border-radius: 999px; background: var(--ink); color: var(--ground); font-weight: 700; font-size: 13px; box-shadow: 0 14px 34px -14px var(--shadow); pointer-events: auto; }
.toasts__item--error { background: var(--accent); color: var(--on-accent); }
.toasts__close { display: grid; place-items: center; width: 22px; height: 22px; border-radius: 50%; opacity: .7; }
.toasts__close:hover { opacity: 1; }
.toasts__item-enter-active, .toasts__item-leave-active { transition: opacity var(--dur-2) var(--ease), transform var(--dur-2) var(--ease); }
.toasts__item-enter-from, .toasts__item-leave-to { opacity: 0; transform: translateY(-8px); }
</style>
