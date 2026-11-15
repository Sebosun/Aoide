<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api } from "@/api";
import type { ActivityStats } from "@/api/types";
import { formatDuration } from "@/estimate";
import { lastDays, series } from "@/stats";
import { useNavigationStore } from "@/stores/navigation";

/** The last seven days on Home: time read and time listened, and a bar per day with both
 *  stacked (reading dark, listening light). */
const navigation = useNavigationStore();
const stats = ref<ActivityStats | null>(null);
onMounted(async () => {
  try {
    stats.value = await api.activityStats();
  } catch {
    stats.value = null;
  }
});
const days = computed(() => {
  const all = series(stats.value?.byDay ?? []),
    today = new Date();
  const reading = lastDays(all.reading, today, 7),
    listening = lastDays(all.listening, today, 7);
  return reading.map((d, i) => {
    const date = new Date(`${d.day}T12:00:00`);
    return {
      key: d.day,
      reading: d.ms,
      listening: listening[i].ms,
      label: date.toLocaleDateString(undefined, { weekday: "narrow" }),
      name: date.toLocaleDateString(undefined, { weekday: "long" }),
      today: i === 6,
    };
  });
});
const reading = computed(() => days.value.reduce((n, d) => n + d.reading, 0));
const listening = computed(() => days.value.reduce((n, d) => n + d.listening, 0));
const max = computed(() => Math.max(1, ...days.value.map((d) => d.reading + d.listening)));
const time = (ms: number) => (ms ? formatDuration(ms) : "0 min");
const height = (ms: number) => `${(ms / max.value) * 100}%`;
</script>

<template>
  <section class="week-card" aria-label="Last 7 days">
    <button type="button" class="week-card__eyebrow" title="Activity" @click="navigation.view = 'activity'">
      Last 7 days
    </button>
    <div class="week-card__totals">
      <p class="week-card__total">
        <b>{{ time(reading) }}</b>{{ " " }}<span><i class="week-card__key week-card__key--reading" />reading</span>
      </p>
      <p class="week-card__total">
        <b>{{ time(listening) }}</b>{{ " "
        }}<span><i class="week-card__key week-card__key--listening" />listening</span>
      </p>
    </div>
    <ol class="week-card__bars" aria-label="Reading and listening per day">
      <li
        v-for="day in days"
        :key="day.key"
        class="week-card__day"
        :class="{ 'week-card__day--today': day.today }"
        :title="`${day.name}: ${time(day.reading)} reading, ${time(day.listening)} listening`"
      >
        <span class="week-card__stack">
          <i
            v-if="day.listening"
            class="week-card__bar week-card__bar--listening"
            :style="{ height: height(day.listening) }"
          />
          <i
            v-if="day.reading"
            class="week-card__bar week-card__bar--reading"
            :style="{ height: height(day.reading) }"
          />
          <i v-if="!day.reading && !day.listening" class="week-card__bar week-card__bar--none" />
        </span>
        <span class="week-card__label">{{ day.label }}</span>
      </li>
    </ol>
  </section>
</template>

<style scoped>
.week-card {
  display: grid;
  gap: 12px;
  align-content: start;
  padding: 20px;
  border-radius: 22px;
  background: var(--page);
}
.week-card__eyebrow {
  justify-self: start;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--dim);
}
.week-card__eyebrow:hover {
  color: var(--ink);
}
.week-card__totals {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 28px;
}
.week-card__total {
  display: grid;
  margin: 0;
}
.week-card__total b {
  font-size: 24px;
  font-weight: 800;
  letter-spacing: -0.03em;
  font-variant-numeric: tabular-nums;
}
.week-card__total span {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--dim);
}
.week-card__key {
  width: 8px;
  height: 8px;
  border-radius: 2px;
}
.week-card__key--reading,
.week-card__bar--reading {
  background: var(--ink);
}
.week-card__key--listening,
.week-card__bar--listening {
  background: var(--faint);
}
.week-card__bars {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: 8px;
  padding: 0;
  list-style: none;
}
.week-card__day {
  display: grid;
  grid-template-rows: 84px auto;
  gap: 6px;
  text-align: center;
}
.week-card__stack {
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  gap: 2px;
}
.week-card__bar {
  display: block;
  min-height: 3px;
  border-radius: 3px;
}
.week-card__bar--none {
  height: 3px;
  background: var(--line);
}
.week-card__label {
  font-size: 11px;
  font-weight: 600;
  color: var(--dim);
}
.week-card__day--today .week-card__label {
  color: var(--ink);
  font-weight: 800;
}
</style>
