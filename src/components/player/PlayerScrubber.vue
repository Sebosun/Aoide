<script setup lang="ts">
/** Chapter progress: played part in coral, generated-but-unplayed audio as a lighter band.
 *  Clicking or arrow keys move the playhead by sentence (the parent decides where). */
const props = withDefaults(defineProps<{ played: number; ahead?: number; start?: string; end?: string; disabled?: boolean }>(), { ahead: 0, start: "", end: "", disabled: false });
const emit = defineEmits<{ seek: [fraction: number]; step: [delta: number] }>();
function click(event: MouseEvent) {
  if (props.disabled) return;
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
  if (box.width) emit("seek", (event.clientX - box.left) / box.width);
}
function key(event: KeyboardEvent) {
  if (props.disabled) return;
  const actions: Record<string, () => void> = {
    ArrowRight: () => emit("step", 1), ArrowUp: () => emit("step", 1),
    ArrowLeft: () => emit("step", -1), ArrowDown: () => emit("step", -1),
    Home: () => emit("seek", 0), End: () => emit("seek", 1),
  };
  const action = actions[event.key];
  if (action) { event.preventDefault(); action(); }
}
</script>

<template>
  <div class="scrubber">
    <span class="scrubber__label">{{ start }}</span>
    <span class="scrubber__track" role="slider" :tabindex="disabled ? -1 : 0" aria-label="Chapter position" aria-valuemin="0" aria-valuemax="100" :aria-valuenow="Math.round(played * 100)" :aria-valuetext="`${Math.round(played * 100)}%${end ? `, ${end}` : ''}`" :aria-disabled="disabled || undefined" @click="click" @keydown="key">
      <i class="scrubber__played" :style="{ width: `${played * 100}%` }" />
      <i class="scrubber__ahead" :style="{ left: `${played * 100}%`, width: `${ahead * 100}%` }" />
    </span>
    <span class="scrubber__label scrubber__label--end">{{ end }}</span>
  </div>
</template>

<style scoped>
.scrubber { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; gap: 14px; align-items: center; font-size: 12.5px; font-weight: 600; color: var(--dim); font-variant-numeric: tabular-nums; }
.scrubber__label { min-width: 3ch; white-space: nowrap; }
.scrubber__label--end { text-align: right; }
.scrubber__track { position: relative; height: 6px; border-radius: 3px; background: var(--line); overflow: hidden; cursor: pointer; }
.scrubber__track[aria-disabled="true"] { cursor: default; }
.scrubber__track:focus-visible { outline-offset: 4px; }
.scrubber__played, .scrubber__ahead { position: absolute; top: 0; bottom: 0; left: 0; }
.scrubber__played { background: var(--accent); border-radius: 3px; }
.scrubber__ahead { background: var(--ink); opacity: .26; }
</style>
