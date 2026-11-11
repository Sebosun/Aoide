<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api } from "../../api";
import type { ListeningStats } from "../../api/types";
import { formatDuration } from "../../estimate";
import { useNavigationStore } from "../../stores/navigation";

const navigation = useNavigationStore();
const stats = ref<ListeningStats | null>(null);
onMounted(async () => { try { stats.value = await api.listeningStats(); } catch { stats.value = null; } });
const key = (date: Date) => `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
const days = computed(() => {
  const byDay = new Map(stats.value?.byDay.map(d => [d.day, d.ms]) ?? []);
  const today = new Date();
  return Array.from({ length: 7 }, (_, i) => {
    const date = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 6 + i, 12);
    return { key: key(date), ms: byDay.get(key(date)) ?? 0, label: date.toLocaleDateString(undefined, { weekday: "narrow" }), name: date.toLocaleDateString(undefined, { weekday: "long" }), today: i === 6 };
  });
});
const total = computed(() => days.value.reduce((n, d) => n + d.ms, 0));
const max = computed(() => Math.max(1, ...days.value.map(d => d.ms)));
</script>

<template>
  <section class="week-card" aria-label="Last 7 days">
    <button type="button" class="week-card__eyebrow" title="Listening stats" @click="navigation.view = 'stats'">Last 7 days</button>
    <b class="week-card__total">{{ total ? formatDuration(total) : "0 min" }}</b>
    <ol class="week-card__bars">
      <li v-for="day in days" :key="day.key" class="week-card__day" :title="`${day.name}: ${day.ms ? formatDuration(day.ms) : 'nothing'}`">
        <i class="week-card__bar" :class="{ 'week-card__bar--today': day.today }" :style="{ height: `${Math.max(4, (day.ms / max) * 100)}%` }" />
        <span class="week-card__label">{{ day.label }}</span>
      </li>
    </ol>
  </section>
</template>

<style scoped>
.week-card { display: grid; gap: 12px; align-content: start; padding: 20px; border-radius: 22px; background: var(--page); }
.week-card__eyebrow { justify-self: start; font-size: 12px; font-weight: 700; letter-spacing: .06em; text-transform: uppercase; color: var(--dim); }
.week-card__eyebrow:hover { color: var(--ink); }
.week-card__total { font-size: 30px; font-weight: 800; letter-spacing: -.03em; font-variant-numeric: tabular-nums; }
.week-card__bars { display: grid; grid-template-columns: repeat(7, 1fr); gap: 8px; padding: 0; list-style: none; }
.week-card__day { display: grid; grid-template-rows: 84px auto; gap: 6px; align-items: end; text-align: center; }
.week-card__bar { display: block; min-height: 4px; border-radius: 5px; background: var(--line); }
.week-card__bar--today { background: var(--accent); }
.week-card__label { font-size: 11px; font-weight: 600; color: var(--dim); }
</style>
