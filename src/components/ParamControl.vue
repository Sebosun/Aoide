<script setup lang="ts">
import { computed } from "vue";
import type { ParamSpec, ParamValue } from "../api/types";
import UiSelect from "./ui/UiSelect.vue";
const props = defineProps<{ spec: ParamSpec; modelValue: ParamValue | undefined }>();
const emit = defineEmits<{ "update:modelValue": [value: ParamValue | undefined] }>();
const value = computed(() => props.modelValue ?? props.spec.default);
const overridden = computed(() => props.modelValue !== undefined && props.modelValue !== props.spec.default);
function number(event: Event) {
  const raw = (event.target as HTMLInputElement).value;
  if (raw === "") return emit("update:modelValue", undefined);
  const parsed = Number(raw);
  if (Number.isFinite(parsed)) emit("update:modelValue", parsed);
}
const shown = computed(() => typeof value.value === "number" && props.spec.kind.type === "float" ? value.value.toFixed(2) : String(value.value));
</script>

<template>
  <div class="param">
    <span class="param__label">{{ spec.label }}</span>
    <template v-if="spec.kind.type === 'float'">
      <input class="param__range" type="range" :aria-label="spec.label" :min="spec.kind.min" :max="spec.kind.max" :step="spec.kind.step" :value="value" @input="number" />
      <output class="param__value">{{ shown }}</output>
    </template>
    <input v-else-if="spec.kind.type === 'int'" class="param__number" type="number" :aria-label="spec.label" :min="spec.kind.min" :max="spec.kind.max" step="1" :value="value" @change="number" />
    <UiSelect v-else-if="spec.kind.type === 'choice'" class="param__choice" :label="spec.label" :options="spec.kind.options" :model-value="String(value)" @update:model-value="emit('update:modelValue', $event)" />
    <input v-else type="checkbox" :aria-label="spec.label" :checked="Boolean(value)" @change="emit('update:modelValue', ($event.target as HTMLInputElement).checked)" />
    <button v-if="overridden" type="button" class="param__reset" :aria-label="`Reset ${spec.label}`" title="Use the default" @click="emit('update:modelValue', undefined)">Reset</button>
  </div>
</template>

<style scoped>
.param { display: grid; grid-template-columns: minmax(110px, 160px) minmax(0, 1fr) auto auto; align-items: center; gap: 12px; font-size: 13px; }
.param__label { color: var(--dim); }
.param__range { width: 100%; accent-color: var(--accent); }
.param__value { font-variant-numeric: tabular-nums; color: var(--dim); min-width: 3.5em; text-align: right; }
.param__number { width: 100%; }
.param__choice { justify-self: start; min-width: 160px; }
.param__reset { font-size: 11px; color: var(--accent); text-decoration: underline; }
@media (max-width: 480px) { .param { grid-template-columns: 1fr auto; } .param__range, .param__number { grid-column: 1 / -1; } }
</style>
