<script setup lang="ts">
import { computed } from "vue";
import type { DayListening } from "../../api/types";
import { formatDuration, speechMs } from "../../estimate";

/** Time spent with one book: the total, the days it was listened on (a bar each, the latest
 *  fourteen), when that began and last happened, and roughly how much is left. */
const props = defineProps<{ days: DayListening[]; wordCount: number; fraction: number; speed: number }>();

const date = (day: string) => new Date(`${day}T12:00:00`);
const dayKey = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
function when(day: string) {
  const today = new Date(), yesterday = new Date(today.getTime() - 86_400_000);
  if (day === dayKey(today)) return "today";
  if (day === dayKey(yesterday)) return "yesterday";
  return date(day).toLocaleDateString(undefined, { day: "numeric", month: "short", year: day.slice(0, 4) === String(today.getFullYear()) ? undefined : "numeric" });
}

const total = computed(() => props.days.reduce((sum, d) => sum + d.ms, 0));
const recent = computed(() => props.days.slice(-14));
const tallest = computed(() => Math.max(1, ...recent.value.map(d => d.ms)));
const bars = computed(() => recent.value.map(d => ({ day: d.day, height: Math.max(8, Math.round((d.ms / tallest.value) * 100)), title: `${date(d.day).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" })}: ${formatDuration(d.ms)}` })));
const first = computed(() => props.days[0]?.day ?? null), last = computed(() => props.days.at(-1)?.day ?? null);
const left = computed(() => {
  if (props.fraction >= 1) return "Finished";
  if (!props.wordCount) return "";
  return `About ${formatDuration(speechMs(props.wordCount * (1 - props.fraction), props.speed))} left at ${props.speed}×`;
});
</script>

<template>
  <section class="book-listening" aria-label="Your listening">
    <h2 class="book-listening__heading">Your listening</h2>
    <p class="book-listening__summary"><b class="book-listening__total">{{ formatDuration(total) }}</b> <span>on {{ days.length }} {{ days.length === 1 ? "day" : "days" }}</span></p>
    <div class="book-listening__bars" role="img" :aria-label="`Listening per day, the latest ${bars.length}`">
      <i v-for="bar in bars" :key="bar.day" class="book-listening__bar" :style="{ height: `${bar.height}%` }" :title="bar.title" />
    </div>
    <p v-if="first && last" class="book-listening__dates">Started {{ when(first) }}<template v-if="last !== first"> · last listened {{ when(last) }}</template></p>
    <p v-if="left" class="book-listening__left">{{ left }}</p>
  </section>
</template>

<style scoped>
.book-listening { display: grid; gap: 8px; }
.book-listening__heading { font-size: 12px; font-weight: 700; letter-spacing: .06em; text-transform: uppercase; color: var(--dim); }
.book-listening__summary { margin: 0; color: var(--dim); font-size: 13px; }
.book-listening__total { font-size: 22px; font-weight: 800; letter-spacing: -.02em; color: var(--ink); }
.book-listening__bars { display: flex; align-items: flex-end; gap: 4px; height: 44px; padding: 2px 0; }
.book-listening__bar { flex: 1; max-width: 18px; min-width: 4px; border-radius: 4px 4px 2px 2px; background: var(--accent); opacity: .85; }
.book-listening__dates, .book-listening__left { margin: 0; font-size: 12.5px; color: var(--dim); }
</style>
