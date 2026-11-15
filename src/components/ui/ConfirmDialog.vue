<script setup lang="ts">
import UiButton from "@/components/ui/UiButton.vue";
import { useId } from "vue";
import UiDialog from "@/components/ui/UiDialog.vue";
import type { ConfirmOptions } from "@/stores/ui";

/** Asks one question with Cancel and a confirm button. Cancel has focus first, so Enter alone
 *  never confirms something destructive. */
defineProps<{ options: ConfirmOptions }>();
const emit = defineEmits<{ answer: [ok: boolean] }>();
const id = useId();
</script>

<template>
  <UiDialog
    :label="options.title"
    role="alertdialog"
    :describedby="options.body ? `${id}-body` : undefined"
    initial-focus="[data-confirm='no']"
    @close="emit('answer', false)"
  >
    <div class="confirm">
      <h2 class="confirm__title">{{ options.title }}</h2>
      <p v-if="options.body" :id="`${id}-body`" class="confirm__body">{{ options.body }}</p>
      <div class="confirm__actions">
        <UiButton variant="outline" class="confirm__button" data-confirm="no" @click="emit('answer', false)">
          {{ options.cancelLabel ?? "Cancel" }}
        </UiButton>
        <UiButton
          variant="primary"
          class="confirm__button confirm__button--yes"
          :danger="options.danger"
          data-confirm="yes"
          @click="emit('answer', true)"
        >
          {{ options.confirmLabel ?? "OK" }}
        </UiButton>
      </div>
    </div>
  </UiDialog>
</template>

<style scoped>
.confirm {
  width: min(420px, calc(100vw - 32px));
  display: grid;
  gap: var(--space-5);
  padding: var(--space-12);
}
.confirm__title {
  font-size: 18px;
  font-weight: 800;
  letter-spacing: -0.01em;
  line-height: 1.25;
}
.confirm__body {
  color: var(--dim);
  line-height: 1.5;
}
.confirm__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-4);
  margin-top: var(--space-5);
}
</style>
