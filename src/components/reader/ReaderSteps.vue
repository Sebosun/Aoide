<script setup lang="ts">
import { Minus, Plus } from "@lucide/vue";
import UiButton from "@/components/ui/UiButton.vue";

/** A setting of a few steps: − and + with a dot for each step, the chosen one in accent.
 *  `less` and `more` name the buttons ("Smaller text", "Larger text"). */
const props = defineProps<{ label: string; count: number; modelValue: number; less: string; more: string }>();
const emit = defineEmits<{ "update:modelValue": [step: number] }>();
const step = (delta: number) =>
  emit("update:modelValue", Math.min(props.count - 1, Math.max(0, props.modelValue + delta)));
</script>

<template>
  <div class="reader-steps" role="group" :aria-label="label">
    <UiButton size="sm" icon :aria-label="less" :title="less" :disabled="modelValue <= 0" @click="step(-1)">
      <Minus :size="15" />
    </UiButton>
    <span class="reader-steps__dots" aria-hidden="true"
      ><i
        v-for="i in count"
        :key="i"
        class="reader-steps__dot"
        :class="{ 'reader-steps__dot--on': i - 1 === modelValue }"
    /></span>
    <UiButton size="sm" icon :aria-label="more" :title="more" :disabled="modelValue >= count - 1" @click="step(1)">
      <Plus :size="15" />
    </UiButton>
  </div>
</template>

<style scoped>
.reader-steps {
  display: inline-flex;
  align-items: center;
  gap: var(--space-5);
}
.reader-steps__dots {
  display: flex;
  gap: var(--space-2);
}
.reader-steps__dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--line);
}
.reader-steps__dot--on {
  background: var(--accent);
}
</style>
