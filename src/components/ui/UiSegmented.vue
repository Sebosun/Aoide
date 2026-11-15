<script setup lang="ts" generic="T extends string | number">
/** A row of options of which one is chosen, drawn as a pill track with the chosen one in ink:
 *  Page or Lines, Light or Dark, a range of days, a kind of item. Each option is a toggle
 *  button (`aria-pressed`) in a labelled group. The `option` slot draws an option's content
 *  (a count beside its label, say); by default it is the label. `tone` matches the track to
 *  what it sits on. */
export interface SegmentedOption<V> {
  value: V;
  label: string;
  /** Read out instead of the content, when the content is not the label alone. */
  ariaLabel?: string;
}

withDefaults(
  defineProps<{
    options: SegmentedOption<T>[];
    modelValue: T;
    /** Names the group. */
    label: string;
    tone?: "glass" | "ground" | "wash";
    size?: "sm" | "md";
    /** How the chosen option stands out: in ink, or as a page-coloured pill (beside other
     *  ink controls, as in the reader's header). */
    chosen?: "ink" | "page";
  }>(),
  { tone: "glass", size: "sm", chosen: "ink" },
);
const emit = defineEmits<{ "update:modelValue": [value: T] }>();
</script>

<template>
  <div
    class="ui-segmented"
    :class="[`ui-segmented--${tone}`, `ui-segmented--${size}`, `ui-segmented--chosen-${chosen}`]"
    role="group"
    :aria-label="label"
  >
    <button
      v-for="option in options"
      :key="option.value"
      type="button"
      class="ui-segmented__option"
      :class="{ 'ui-segmented__option--on': option.value === modelValue }"
      :aria-label="option.ariaLabel"
      :aria-pressed="option.value === modelValue"
      @click="emit('update:modelValue', option.value)"
    >
      <slot name="option" :option="option">{{ option.label }}</slot>
    </button>
  </div>
</template>

<style scoped>
.ui-segmented {
  display: inline-flex;
  flex-wrap: wrap;
  gap: var(--space-1);
  padding: var(--space-1);
  border-radius: var(--radius-pill);
}
.ui-segmented--glass {
  background: var(--glass);
}
.ui-segmented--ground {
  background: var(--ground);
}
.ui-segmented--wash {
  background: var(--wash);
}
.ui-segmented__option {
  display: inline-flex;
  align-items: center;
  gap: var(--space-3);
  border-radius: var(--radius-pill);
  font-weight: 700;
  color: var(--dim);
  white-space: nowrap;
  transition:
    background-color var(--dur-1) var(--ease),
    color var(--dur-1) var(--ease);
}
.ui-segmented--sm .ui-segmented__option {
  padding: var(--space-3) var(--space-7);
  font-size: 13px;
}
.ui-segmented--md .ui-segmented__option {
  height: var(--control-md);
  padding: 0 var(--space-8);
}
.ui-segmented__option:not(.ui-segmented__option--on):hover {
  color: var(--ink);
}
.ui-segmented--chosen-ink .ui-segmented__option--on {
  background: var(--ink);
  color: var(--ground);
}
.ui-segmented--chosen-page .ui-segmented__option--on {
  background: var(--page);
  color: var(--ink);
}
</style>
