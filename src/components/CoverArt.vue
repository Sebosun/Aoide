<script setup lang="ts">
import { computed } from "vue";

/** A book's cover, or generated art (two colours from a hash of the title, drawn as a sun and
 *  a horizon) when the EPUB has none. The generated colours are data, not theme colours.
 *  `portrait` frames are 2:3 like a printed cover, so real covers are shown nearly uncropped
 *  (cropping a portrait cover into a square enlarges it and looks soft); `square` is for
 *  small thumbnails. */
const props = withDefaults(defineProps<{ title: string; url?: string | null; label?: boolean; shape?: "portrait" | "square" }>(), { url: null, label: true, shape: "portrait" });
const art = computed(() => {
  let hash = 2166136261;
  for (const char of props.title) hash = Math.imul(hash ^ char.charCodeAt(0), 16777619) >>> 0;
  const hue = hash % 360, sun = (hue + 140 + (hash >> 9) % 80) % 360;
  return { "--art-1": `hsl(${hue} 42% 30%)`, "--art-2": `hsl(${sun} 78% 68%)`, "--art-3": `hsl(${hue} 46% 15%)`, "--art-ink": `hsl(${hue} 40% 97%)` };
});
</script>

<template>
  <span class="cover-art" :class="[`cover-art--${shape}`, { 'cover-art--image': url }]" :style="url ? undefined : art">
    <img v-if="url" class="cover-art__image" :src="url" alt="" loading="lazy" decoding="async" draggable="false" />
    <span v-else-if="label" class="cover-art__title">{{ title }}</span>
  </span>
</template>

<style scoped>
.cover-art { display: flex; align-items: flex-end; width: 100%; padding: 10%; overflow: hidden; border-radius: 14px; color: var(--art-ink); font-weight: 800; line-height: 1.1; letter-spacing: -.02em; overflow-wrap: anywhere;
  background:
    radial-gradient(circle at 72% 28%, var(--art-2) 0 20%, transparent 21%),
    radial-gradient(circle at 20% 110%, color-mix(in srgb, var(--art-2) 45%, transparent) 0 35%, transparent 36%),
    linear-gradient(155deg, var(--art-1), var(--art-3));
  box-shadow: 0 18px 40px -18px var(--shadow); }
.cover-art--portrait { aspect-ratio: 2 / 3; }
.cover-art--square { aspect-ratio: 1; }
.cover-art--image { padding: 0; background: var(--line); }
.cover-art__image { width: 100%; height: 100%; object-fit: cover; }
.cover-art__title { display: -webkit-box; -webkit-line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden; }
</style>
