<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import type { AppearanceMode, Theme } from "@/api/types";

/** The colour themes to choose from, each a swatch in its own colours (for the light or dark
 *  variant now shown) with its title: Aoide's own, those that come with it and the user's
 *  files. A theme chosen whose file is gone is listed as such. */
const props = defineProps<{ themes: Theme[]; modelValue: string; mode: AppearanceMode }>();
const emit = defineEmits<{ "update:modelValue": [name: string] }>();

const SWATCH = ["ground", "page", "ink", "accent"] as const;
const query = typeof window.matchMedia === "function" ? window.matchMedia("(prefers-color-scheme: dark)") : null;
const systemDark = ref(query?.matches ?? false);
const follow = (e: MediaQueryListEvent) => (systemDark.value = e.matches);
query?.addEventListener("change", follow);
onBeforeUnmount(() => query?.removeEventListener("change", follow));
const dark = computed(() => props.mode === "dark" || (props.mode === "auto" && systemDark.value));

interface Choice {
  name: string;
  title: string;
  style: Record<string, string>;
}
/** A token the theme leaves out keeps Aoide's colour, as it does when applied. */
const style = (tokens: Record<string, string>) =>
  Object.fromEntries(SWATCH.map((t) => [`--swatch-${t}`, tokens[t] ?? `var(--aoide-${t})`]));
const choices = computed<Choice[]>(() => {
  const listed = props.themes.map((t) => ({
    name: t.name,
    title: t.title,
    style: style(dark.value ? t.dark : t.light),
  }));
  const missing =
    props.modelValue !== "default" && !props.themes.some((t) => t.name === props.modelValue)
      ? [{ name: props.modelValue, title: `${props.modelValue} (file not found)`, style: style({}) }]
      : [];
  return [{ name: "default", title: "Aoide", style: style({}) }, ...missing, ...listed];
});
</script>

<template>
  <div class="theme-picker" role="radiogroup" aria-label="Colour theme">
    <label
      v-for="c in choices"
      :key="c.name"
      class="theme-picker__choice"
      :class="{ 'theme-picker__choice--on': c.name === modelValue }"
    >
      <input
        class="visually-hidden"
        type="radio"
        name="colour-theme"
        :value="c.name"
        :checked="c.name === modelValue"
        @change="emit('update:modelValue', c.name)"
      />
      <span class="theme-picker__swatch" :style="c.style" aria-hidden="true"
        ><span class="theme-picker__page"
          ><span class="theme-picker__line"></span
          ><span class="theme-picker__line theme-picker__line--short"></span></span
        ><span class="theme-picker__dot"></span
      ></span>
      <span class="theme-picker__title">{{ c.title }}</span>
    </label>
  </div>
</template>

<style scoped>
.theme-picker {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
  gap: var(--space-4);
}
.theme-picker__choice {
  display: grid;
  gap: var(--space-2);
  padding: var(--space-2);
  border-radius: var(--radius-md);
  cursor: pointer;
  font-size: 12.5px;
  color: var(--dim);
}
.theme-picker__choice:hover {
  background: var(--wash);
}
.theme-picker__choice--on {
  color: var(--ink);
  font-weight: 700;
}
.theme-picker__choice:focus-within {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}
.theme-picker__swatch {
  position: relative;
  display: block;
  aspect-ratio: 16 / 10;
  padding: var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--swatch-ground);
  box-shadow: inset 0 0 0 1px var(--line);
}
.theme-picker__choice--on .theme-picker__swatch {
  box-shadow:
    inset 0 0 0 1px var(--line),
    0 0 0 2px var(--accent);
}
.theme-picker__page {
  display: grid;
  align-content: start;
  gap: var(--space-2);
  height: 100%;
  padding: var(--space-3);
  border-radius: var(--radius-xs);
  background: var(--swatch-page);
}
.theme-picker__line {
  display: block;
  height: var(--space-1);
  border-radius: var(--radius-xs);
  background: var(--swatch-ink);
}
.theme-picker__line--short {
  width: 60%;
  opacity: 0.6;
}
.theme-picker__dot {
  position: absolute;
  right: var(--space-4);
  bottom: var(--space-4);
  width: var(--space-5);
  height: var(--space-5);
  border-radius: var(--radius-pill);
  background: var(--swatch-accent);
}
.theme-picker__title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
