<script setup lang="ts">
import { onMounted, ref } from "vue";
import UiDialog from "@/components/ui/UiDialog.vue";
import type { PromptOptions } from "@/stores/ui";

/** Asks for one line of text, such as a collection name. */
const props = defineProps<{ options: PromptOptions }>();
const emit = defineEmits<{ answer: [text: string | null] }>();
const text = ref(props.options.value ?? ""),
  input = ref<HTMLInputElement>();
onMounted(() => requestAnimationFrame(() => input.value?.select()));
</script>

<template>
  <UiDialog :label="options.title" initial-focus="input" @close="emit('answer', null)">
    <form class="prompt" @submit.prevent="text.trim() && emit('answer', text)">
      <h2 class="prompt__title">{{ options.title }}</h2>
      <label class="prompt__field"
        ><span>{{ options.label }}</span
        ><input
          ref="input"
          v-model="text"
          type="text"
          :aria-label="options.label"
          :placeholder="options.placeholder"
          maxlength="80"
          spellcheck="false"
      /></label>
      <div class="prompt__actions">
        <button type="button" class="prompt__button" @click="emit('answer', null)">Cancel</button>
        <button type="submit" class="prompt__button prompt__button--yes" :disabled="!text.trim()">
          {{ options.confirmLabel ?? "OK" }}
        </button>
      </div>
    </form>
  </UiDialog>
</template>

<style scoped>
.prompt {
  width: min(420px, calc(100vw - 32px));
  display: grid;
  gap: var(--space-7);
  padding: var(--space-12);
}
.prompt__title {
  font-size: 18px;
  font-weight: 800;
  letter-spacing: -0.01em;
}
.prompt__field {
  display: grid;
  gap: var(--space-3);
  font-size: 13px;
  font-weight: 600;
  color: var(--dim);
}
.prompt__field input {
  color: var(--ink);
  font-size: 14px;
  padding: var(--space-5) var(--space-6);
}
.prompt__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-4);
  margin-top: var(--space-2);
}
.prompt__button {
  height: 38px;
  padding: 0 var(--space-9);
  border-radius: var(--radius-pill);
  background: var(--glass);
  box-shadow: inset 0 0 0 1px var(--line);
  font-weight: 700;
}
.prompt__button--yes {
  background: var(--ink);
  color: var(--ground);
  box-shadow: none;
}
</style>
