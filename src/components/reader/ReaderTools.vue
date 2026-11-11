<script setup lang="ts">
import { ref } from "vue";
import { Minus, Plus } from "@lucide/vue";
import type { ReaderMode } from "../../api/types";
import { TEXT_SCALES } from "../../theme";
import { useMenu } from "../../composables/useMenu";
import { usePreferencesStore } from "../../stores/preferences";

/** Text size ("Aa") and the Page / Lines switch. */
const preferences = usePreferencesStore();
const root = ref<HTMLElement>(), panel = ref<HTMLElement>(), button = ref<HTMLElement>();
const { open, onKeydown } = useMenu(root, panel, button);
const size = (delta: number) => preferences.update({ textSize: Math.min(TEXT_SCALES.length - 1, Math.max(0, preferences.appearance.textSize + delta)) });
const modes: { value: ReaderMode; label: string }[] = [{ value: "page", label: "Page" }, { value: "lines", label: "Lines" }];
</script>

<template>
  <div class="reader-tools">
    <div ref="root" class="reader-tools__size">
      <button ref="button" type="button" class="reader-tools__aa" aria-label="Text size" :aria-expanded="open" aria-haspopup="true" @click="open = !open">Aa</button>
      <div v-if="open" ref="panel" class="reader-tools__panel" role="group" aria-label="Text size" @keydown="onKeydown">
        <button type="button" class="reader-tools__step" aria-label="Smaller text" :disabled="preferences.appearance.textSize <= 0" @click="size(-1)"><Minus :size="15" /></button>
        <span class="reader-tools__dots" aria-hidden="true"><i v-for="(_, i) in TEXT_SCALES" :key="i" :class="{ 'reader-tools__dot--on': i === preferences.appearance.textSize }" class="reader-tools__dot" /></span>
        <button type="button" class="reader-tools__step" aria-label="Larger text" :disabled="preferences.appearance.textSize >= TEXT_SCALES.length - 1" @click="size(1)"><Plus :size="15" /></button>
      </div>
    </div>
    <div class="reader-tools__mode" role="group" aria-label="Reader mode">
      <button v-for="mode in modes" :key="mode.value" type="button" class="reader-tools__option" :class="{ 'reader-tools__option--on': preferences.appearance.readerMode === mode.value }" :aria-label="mode.label" :aria-pressed="preferences.appearance.readerMode === mode.value" @click="preferences.update({ readerMode: mode.value })">{{ mode.label }}</button>
    </div>
  </div>
</template>

<style scoped>
.reader-tools { display: flex; align-items: center; gap: 10px; }
.reader-tools__size { position: relative; }
.reader-tools__aa { display: grid; place-items: center; height: 30px; padding: 0 12px; border-radius: 999px; background: var(--ground); font: 500 15px/1 var(--font-read); }
.reader-tools__panel { position: absolute; right: 0; top: calc(100% + 6px); z-index: 7; display: flex; align-items: center; gap: 10px; padding: 6px; border-radius: 999px; background: var(--page); box-shadow: 0 14px 34px -14px var(--shadow), 0 0 0 1px var(--line); }
.reader-tools__step { display: grid; place-items: center; width: 30px; height: 30px; border-radius: 50%; background: var(--ground); }
.reader-tools__dots { display: flex; gap: 5px; }
.reader-tools__dot { width: 6px; height: 6px; border-radius: 50%; background: var(--line); }
.reader-tools__dot--on { background: var(--accent); }
.reader-tools__mode { display: flex; gap: 2px; padding: 3px; border-radius: 999px; background: var(--ground); font-size: 12px; font-weight: 700; }
.reader-tools__option { padding: 5px 12px; border-radius: 999px; color: var(--dim); }
.reader-tools__option--on { background: var(--ink); color: var(--ground); }
</style>
