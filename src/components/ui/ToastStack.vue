<script setup lang="ts">
import { X } from "@lucide/vue";
import { useUiStore } from "@/stores/ui";
const ui = useUiStore();
</script>

<template>
  <div class="toasts" role="status" aria-live="polite">
    <TransitionGroup name="toasts__item">
      <p v-for="toast in ui.toasts" :key="toast.id" class="toasts__item" :class="`toasts__item--${toast.tone}`">
        <span>{{ toast.text }}</span>
        <button
          v-if="toast.action"
          type="button"
          class="toasts__action"
          @click="
            ui.dismiss(toast.id);
            toast.action.run();
          "
        >
          {{ toast.action.label }}
        </button>
        <button type="button" class="toasts__close" aria-label="Dismiss" @click="ui.dismiss(toast.id)">
          <X :size="14" />
        </button>
      </p>
    </TransitionGroup>
  </div>
</template>

<style scoped>
.toasts {
  position: fixed;
  z-index: 50;
  left: 50%;
  top: 72px;
  translate: -50% 0;
  display: grid;
  justify-items: center;
  gap: var(--space-4);
  pointer-events: none;
}
.toasts__item {
  display: inline-flex;
  align-items: center;
  gap: var(--space-5);
  max-width: min(460px, calc(100vw - 32px));
  padding: var(--space-4) var(--space-5) var(--space-4) var(--space-8);
  border-radius: var(--radius-pill);
  background: var(--ink);
  color: var(--ground);
  font-weight: 700;
  font-size: 13px;
  box-shadow: 0 14px 34px -14px var(--shadow);
  pointer-events: auto;
}
.toasts__item--error {
  background: var(--accent);
  color: var(--on-accent);
}
.toasts__action {
  padding: var(--space-1) var(--space-4);
  border-radius: var(--radius-pill);
  color: inherit;
  font: inherit;
  text-decoration: underline;
  text-underline-offset: 3px;
}
.toasts__action:hover {
  background: color-mix(in srgb, currentColor 14%, transparent);
}
.toasts__close {
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  opacity: 0.7;
}
.toasts__close:hover {
  opacity: 1;
}
.toasts__item-enter-active,
.toasts__item-leave-active {
  transition:
    opacity var(--dur-2) var(--ease),
    transform var(--dur-2) var(--ease);
}
.toasts__item-enter-from,
.toasts__item-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}
</style>
