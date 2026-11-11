<script setup lang="ts">
import { useId } from "vue";
import UiDialog from "./UiDialog.vue";
import type { ConfirmOptions } from "../../stores/ui";

/** Asks one question with Cancel and a confirm button. Cancel has focus first, so Enter alone
 *  never confirms something destructive. */
defineProps<{ options: ConfirmOptions }>();
const emit = defineEmits<{ answer: [ok: boolean] }>();
const id = useId();
</script>

<template>
  <UiDialog :label="options.title" role="alertdialog" :describedby="options.body ? `${id}-body` : undefined" initial-focus="[data-confirm='no']" @close="emit('answer', false)">
    <div class="confirm">
      <h2 class="confirm__title">{{ options.title }}</h2>
      <p v-if="options.body" :id="`${id}-body`" class="confirm__body">{{ options.body }}</p>
      <div class="confirm__actions">
        <button type="button" class="confirm__button" data-confirm="no" @click="emit('answer', false)">{{ options.cancelLabel ?? "Cancel" }}</button>
        <button type="button" class="confirm__button confirm__button--yes" :class="{ 'confirm__button--danger': options.danger }" data-confirm="yes" @click="emit('answer', true)">{{ options.confirmLabel ?? "OK" }}</button>
      </div>
    </div>
  </UiDialog>
</template>

<style scoped>
.confirm { width: min(420px, calc(100vw - 32px)); display: grid; gap: 10px; padding: 24px; }
.confirm__title { font-size: 18px; font-weight: 800; letter-spacing: -.01em; line-height: 1.25; }
.confirm__body { color: var(--dim); line-height: 1.5; }
.confirm__actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 10px; }
.confirm__button { height: 38px; padding: 0 18px; border-radius: 999px; background: var(--glass); box-shadow: inset 0 0 0 1px var(--line); font-weight: 700; }
.confirm__button--yes { background: var(--ink); color: var(--ground); box-shadow: none; }
.confirm__button--danger { background: var(--accent); color: var(--on-accent); }
</style>
