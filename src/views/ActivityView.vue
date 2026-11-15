<script setup lang="ts">
import UiCard from "@/components/ui/UiCard.vue";
import UiSegmented from "@/components/ui/UiSegmented.vue";
import { computed, onMounted, ref } from "vue";
import { BookOpen, Flame, Gauge, Headphones } from "@lucide/vue";
import { api } from "@/api";
import type { ActivityStats } from "@/api/types";
import CoverArt from "@/components/CoverArt.vue";
import UiSkeleton from "@/components/ui/UiSkeleton.vue";
import { formatDuration } from "@/estimate";
import { dayKey, heatmap, lastDays, lastMonths, series as split, streaks, summary, weekdays } from "@/stats";
import { useLibraryStore } from "@/stores/library";
import { useNavigationStore } from "@/stores/navigation";

/** Reading and listening over a chosen range: a total of each, a streak, a bar per day (or
 *  month) with both stacked, a calendar of the last half year, weekdays, and time per book.
 *  Streaks, the calendar and weekdays count any time spent. */
type Range = "week" | "month" | "year" | "all";
const RANGES: { id: Range; label: string }[] = [
  { id: "week", label: "7 days" },
  { id: "month", label: "30 days" },
  { id: "year", label: "12 months" },
  { id: "all", label: "All" },
];
const library = useLibraryStore(),
  navigation = useNavigationStore();
const stats = ref<ActivityStats | null>(null),
  error = ref(""),
  range = ref<Range>("week");
onMounted(async () => {
  void library.start();
  try {
    stats.value = await api.activityStats();
  } catch (e) {
    error.value = String(e);
  }
});

const today = new Date();
const all = computed(() => split(stats.value?.byDay ?? []));
const days = computed(() => all.value.both);
const first = computed(() => days.value.find((d) => d.ms > 0)?.day ?? dayKey(today));
const spanDays = computed(() =>
  Math.max(
    1,
    Math.round(
      (new Date(`${dayKey(today)}T12:00:00`).getTime() - new Date(`${first.value}T12:00:00`).getTime()) / 86_400_000,
    ) + 1,
  ),
);
const count = computed(() =>
  range.value === "week" ? 7 : range.value === "month" ? 30 : range.value === "year" ? 365 : spanDays.value,
);
const series = computed(() => lastDays(days.value, today, count.value));
/** Over 12 months, averages count from the first day of listening, so a new library is not
 *  diluted by months before it existed; 7 and 30 days are always whole. */
const counted = computed(() =>
  range.value === "year" ? series.value.filter((d) => d.day >= first.value) : series.value,
);
const totals = computed(() => summary(counted.value.length ? counted.value : series.value));
/** Reading and listening in the range. */
const sums = computed(() => {
  const from = (counted.value.length ? counted.value : series.value)[0]?.day ?? dayKey(today);
  const sum = (list: { day: string; ms: number }[]) => list.filter((d) => d.day >= from).reduce((n, d) => n + d.ms, 0);
  return { reading: sum(all.value.reading), listening: sum(all.value.listening) };
});
const streak = computed(() => streaks(days.value, today));
const monthly = computed(() => range.value === "year" || (range.value === "all" && spanDays.value > 62));
const bars = computed(() => {
  if (!monthly.value) {
    const reading = lastDays(all.value.reading, today, count.value),
      listening = lastDays(all.value.listening, today, count.value);
    return reading.map((d, i, list) => ({
      key: d.day,
      reading: d.ms,
      listening: listening[i].ms,
      label: dayLabel(d.day, list.length - 1 - i),
      title: new Date(`${d.day}T12:00:00`).toLocaleDateString(undefined, {
        weekday: "short",
        day: "numeric",
        month: "short",
      }),
      now: d.day === dayKey(today),
    }));
  }
  const months = range.value === "year" ? 12 : Math.ceil(spanDays.value / 30.4) + 1;
  const listening = lastMonths(all.value.listening, today, months);
  return lastMonths(all.value.reading, today, months).map((m, i) => {
    const date = new Date(`${m.month}-15T12:00:00`);
    return {
      key: m.month,
      reading: m.ms,
      listening: listening[i].ms,
      label: date.toLocaleDateString(undefined, { month: "narrow" }),
      title: date.toLocaleDateString(undefined, { month: "long", year: "numeric" }),
      now: m.month === dayKey(today).slice(0, 7),
    };
  });
});
const barMax = computed(() => Math.max(1, ...bars.value.map((b) => b.reading + b.listening)));
const barHeight = (ms: number) => (ms ? `${Math.max(2, (ms / barMax.value) * 100)}%` : "0");
/** Weekday initials for a week; for longer ranges the date of today and every seventh day back. */
function dayLabel(day: string, daysAgo: number) {
  const date = new Date(`${day}T12:00:00`);
  if (range.value === "week") return date.toLocaleDateString(undefined, { weekday: "narrow" });
  return daysAgo % 7 === 0 ? date.toLocaleDateString(undefined, { day: "numeric", month: "short" }) : "";
}
const calendar = computed(() => heatmap(days.value, today, 26));
const week = computed(() => weekdays(days.value));
const weekMax = computed(() => Math.max(1, ...week.value.map((w) => w.ms)));
const favourite = computed(() => {
  const best = [...week.value].sort((a, b) => b.ms - a.ms)[0];
  if (!best?.ms) return "";
  const names: Record<string, string> = {
    Mon: "Mondays",
    Tue: "Tuesdays",
    Wed: "Wednesdays",
    Thu: "Thursdays",
    Fri: "Fridays",
    Sat: "Saturdays",
    Sun: "Sundays",
  };
  return names[best.label];
});
const books = computed(() =>
  (stats.value?.byBook ?? []).map((entry) => {
    const book = library.books.find((b) => b.id === entry.bookId);
    return {
      ...entry,
      ms: entry.readingMs + entry.listeningMs,
      book,
      title: book?.title ?? "Removed book",
      state: !book
        ? "Removed from the library"
        : book.readState === "finished"
          ? "Finished"
          : `${Math.round((book.progress?.fraction ?? 0) * 100)}%`,
    };
  }),
);
const bookMax = computed(() => Math.max(1, ...books.value.map((b) => b.ms)));
const time = (ms: number) => (ms ? formatDuration(ms) : "0 min");
const cellTitle = (cell: { day: string | null; ms: number }) =>
  cell.day
    ? `${new Date(`${cell.day}T12:00:00`).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" })}: ${cell.ms ? formatDuration(cell.ms) : "nothing"}`
    : "";
</script>

<template>
  <section class="activity" :aria-busy="!stats && !error">
    <header class="activity__header">
      <h1 class="activity__title">Activity</h1>
      <UiSegmented v-model="range" label="Range" :options="RANGES.map((r) => ({ value: r.id, label: r.label }))" />
    </header>
    <p v-if="error" class="activity__error" role="alert">{{ error }}</p>

    <template v-if="!stats && !error">
      <p class="visually-hidden" role="status" aria-label="Loading activity">Loading activity…</p>
      <div class="activity__tiles" aria-hidden="true">
        <UiSkeleton v-for="n in 4" :key="n" class="activity__ghost-tile" />
      </div>
      <UiSkeleton class="activity__ghost-card" />
      <div class="activity__pair" aria-hidden="true">
        <UiSkeleton class="activity__ghost-card" /><UiSkeleton class="activity__ghost-card" />
      </div>
    </template>
    <template v-if="stats">
      <div class="activity__tiles">
        <div class="activity__tile activity__tile--hero" role="group" aria-label="Read">
          <BookOpen :size="18" /><span>Read</span><strong>{{ time(sums.reading) }}</strong
          ><small><i class="activity__key activity__key--reading" />reading</small>
        </div>
        <div class="activity__tile activity__tile--hero" role="group" aria-label="Listened">
          <Headphones :size="18" /><span>Listened</span><strong>{{ time(sums.listening) }}</strong
          ><small><i class="activity__key activity__key--listening" />listening</small>
        </div>
        <div class="activity__tile" role="group" aria-label="Daily average">
          <Gauge :size="18" /><span>Daily average</span><strong>{{ time(totals.averageMs) }}</strong
          ><small>{{ totals.activeDays }} of {{ counted.length || series.length }} days</small>
        </div>
        <div class="activity__tile" role="group" aria-label="Streak">
          <Flame :size="18" /><span>Streak</span
          ><strong>{{ streak.current }} {{ streak.current === 1 ? "day" : "days" }}</strong
          ><small>best {{ streak.best }}</small>
        </div>
      </div>

      <UiCard as="p" pad="lg" v-if="!stats.readingMs && !stats.listeningMs" class="activity__empty">
        Nothing yet. Time appears here as you read and listen.
      </UiCard>
      <template v-else>
        <UiCard class="activity__card activity__chart">
          <ol
            class="activity__bars"
            :class="{ 'activity__bars--dense': bars.length > 14 }"
            :aria-label="monthly ? 'Reading and listening by month' : 'Reading and listening by day'"
          >
            <li
              v-for="bar in bars"
              :key="bar.key"
              class="activity__bar"
              :class="{ 'activity__bar--now': bar.now }"
              :title="`${bar.title}: ${time(bar.reading)} reading, ${time(bar.listening)} listening`"
              :aria-label="`${bar.title}: ${time(bar.reading)} reading, ${time(bar.listening)} listening`"
            >
              <span class="activity__stack"
                ><span
                  v-if="bar.listening"
                  class="activity__bar-fill activity__bar-fill--listening"
                  :style="{ height: barHeight(bar.listening) }" /><span
                  v-if="bar.reading"
                  class="activity__bar-fill activity__bar-fill--reading"
                  :style="{ height: barHeight(bar.reading) }"
              /></span>
              <span class="activity__bar-label" aria-hidden="true">{{ bar.label }}</span>
            </li>
          </ol>
        </UiCard>

        <div class="activity__pair">
          <UiCard class="activity__card">
            <h2 class="activity__heading">Last 26 weeks</h2>
            <div class="activity__calendar" role="img" aria-label="Last 26 weeks">
              <div v-for="(column, w) in calendar" :key="w" class="activity__week">
                <span
                  v-for="(cell, d) in column"
                  :key="d"
                  class="activity__cell"
                  :class="[`activity__cell--${cell.level}`, { 'activity__cell--future': !cell.day }]"
                  :data-day="cell.day ?? undefined"
                  :title="cellTitle(cell)"
                />
              </div>
            </div>
            <div class="activity__scale" aria-hidden="true">
              Less <i v-for="n in 5" :key="n" :class="`activity__cell activity__cell--${n - 1}`" /> More
            </div>
          </UiCard>
          <UiCard class="activity__card" aria-label="By weekday">
            <h2 class="activity__heading">By weekday</h2>
            <p v-if="favourite" class="activity__note">Most on {{ favourite }}</p>
            <ul class="activity__weekdays">
              <li v-for="w in week" :key="w.label" :title="time(w.ms)">
                <span>{{ w.label }}</span><span class="activity__track"
                  ><i :style="{ width: `${(w.ms / weekMax) * 100}%` }" /></span
                ><b>{{ time(w.ms) }}</b>
              </li>
            </ul>
          </UiCard>
        </div>

        <UiCard class="activity__card">
          <h2 class="activity__heading">By book <small>all time</small></h2>
          <ul class="activity__books" aria-label="By book">
            <li v-for="row in books" :key="row.bookId">
              <button
                type="button"
                class="activity__book"
                :disabled="!row.book"
                @click="row.book && navigation.openItem(row.bookId)"
              >
                <CoverArt
                  class="activity__thumb"
                  :title="row.title"
                  :url="row.book?.coverUrl"
                  :label="false"
                  shape="square"
                />
                <span class="activity__book-text"
                  ><b>{{ row.title }}</b
                  ><small>{{ row.state }}</small></span
                >
                <span
                  class="activity__track activity__track--book activity__track--split"
                  :title="`${time(row.readingMs)} reading, ${time(row.listeningMs)} listening`"
                  ><i
                    v-if="row.readingMs"
                    class="activity__part--reading"
                    :style="{ width: `${(row.readingMs / bookMax) * 100}%` }" /><i
                    v-if="row.listeningMs"
                    class="activity__part--listening"
                    :style="{ width: `${(row.listeningMs / bookMax) * 100}%` }"
                /></span>
                <span class="activity__book-time">{{ time(row.ms) }}</span>
              </button>
            </li>
          </ul>
        </UiCard>
      </template>
    </template>
  </section>
</template>

<style scoped>
.activity {
  max-width: 1120px;
  margin: 0 auto;
  padding: var(--space-4) var(--space-14) var(--space-20);
  display: grid;
  gap: var(--space-9);
}
.activity__header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-6);
}
.activity__title {
  font-size: 32px;
  font-weight: 800;
  letter-spacing: -0.03em;
}
.activity__error {
  color: var(--accent);
  font-weight: 600;
}
.activity__tiles {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: var(--space-6);
}
.activity__tile {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: center;
  gap: var(--space-2) var(--space-5);
  padding: var(--space-8) var(--space-9);
  border-radius: var(--radius-lg);
  background: var(--page);
  color: var(--dim);
  font-size: 12.5px;
  font-weight: 600;
}
.activity__tile strong {
  grid-column: 1 / -1;
  font-size: 26px;
  font-weight: 800;
  letter-spacing: -0.03em;
  color: var(--ink);
  font-variant-numeric: tabular-nums;
}
.activity__tile small {
  grid-column: 1 / -1;
  margin-top: calc(-1 * var(--space-2));
  font-size: 12px;
  color: var(--faint);
}
.activity__tile--hero {
  background: var(--hero);
}
.activity__tile--hero svg {
  color: var(--accent);
}
.activity__ghost-tile {
  height: 96px;
  border-radius: var(--radius-lg);
}
.activity__ghost-card {
  height: 240px;
  border-radius: var(--radius-lg);
}
.activity__empty {
  color: var(--dim);
  text-align: center;
}
.activity__card {
  display: grid;
  gap: var(--space-6);
  align-content: start;
  min-width: 0;
}
.activity__heading {
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--dim);
}
.activity__heading small {
  margin-left: var(--space-3);
  font-weight: 600;
  color: var(--faint);
  text-transform: none;
  letter-spacing: 0;
}
.activity__note {
  margin-top: calc(-1 * var(--space-3));
  font-size: 13px;
  font-weight: 700;
}
.activity__bars {
  display: flex;
  align-items: stretch;
  gap: var(--space-4);
  height: 200px;
  padding: 0;
  margin: 0;
  list-style: none;
}
.activity__bars--dense {
  gap: var(--space-1);
}
.activity__bar {
  flex: 1;
  min-width: 0;
  display: grid;
  grid-template-rows: minmax(0, 1fr) 18px;
  align-items: end;
  justify-items: center;
  gap: var(--space-3);
}
.activity__stack {
  width: 100%;
  max-width: 46px;
  height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  gap: var(--space-1);
}
.activity__bar-fill {
  display: block;
  border-radius: var(--radius-xs);
  transition: height var(--dur-2) var(--ease);
}
.activity__bar-fill--reading,
.activity__key--reading,
.activity__part--reading {
  background: var(--ink);
}
.activity__bar-fill--listening,
.activity__key--listening,
.activity__part--listening {
  background: var(--faint);
}
.activity__bar:hover .activity__stack {
  opacity: 0.8;
}
.activity__bar--now .activity__bar-label {
  color: var(--ink);
  font-weight: 800;
}
.activity__key {
  display: inline-block;
  width: 8px;
  height: 8px;
  margin-right: var(--space-3);
  border-radius: var(--radius-xs);
}
.activity__track--split {
  display: flex;
  gap: var(--space-1);
}
.activity__bar-label {
  font-size: 11px;
  font-weight: 600;
  color: var(--dim);
  white-space: nowrap;
  overflow: visible;
}
.activity__pair {
  display: grid;
  grid-template-columns: minmax(0, 1.5fr) minmax(0, 1fr);
  gap: var(--space-9);
}
.activity__calendar {
  display: grid;
  grid-template-columns: repeat(26, minmax(0, 1fr));
  gap: var(--space-1);
}
.activity__week {
  display: grid;
  grid-template-rows: repeat(7, auto);
  gap: var(--space-1);
}
.activity__cell {
  display: block;
  width: 100%;
  aspect-ratio: 1;
  border-radius: var(--radius-xs);
  background: var(--wash);
}
.activity__cell--1 {
  background: color-mix(in srgb, var(--accent) 30%, var(--wash));
}
.activity__cell--2 {
  background: color-mix(in srgb, var(--accent) 55%, var(--wash));
}
.activity__cell--3 {
  background: color-mix(in srgb, var(--accent) 78%, var(--wash));
}
.activity__cell--4 {
  background: var(--accent);
}
.activity__cell--future {
  background: none;
}
.activity__scale {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  font-size: 11px;
  color: var(--dim);
}
.activity__scale i {
  width: 11px;
  height: 11px;
  aspect-ratio: auto;
}
.activity__weekdays {
  display: grid;
  gap: var(--space-3);
  padding: 0;
  margin: 0;
  list-style: none;
  font-size: 12.5px;
}
.activity__weekdays li {
  display: grid;
  grid-template-columns: 34px minmax(0, 1fr) 76px;
  align-items: center;
  gap: var(--space-5);
  color: var(--dim);
  font-weight: 600;
}
.activity__weekdays b {
  text-align: right;
  color: var(--ink);
  font-variant-numeric: tabular-nums;
}
.activity__track {
  position: relative;
  height: 8px;
  border-radius: var(--radius-xs);
  background: var(--wash);
  overflow: hidden;
}
.activity__track i {
  position: absolute;
  inset: 0 auto 0 0;
  border-radius: var(--radius-xs);
  background: var(--ink);
}
.activity__track--split i {
  position: static;
  flex: none;
}
.activity__track--split .activity__part--reading {
  background: var(--ink);
}
.activity__track--split .activity__part--listening {
  background: var(--faint);
}
.activity__books {
  display: grid;
  gap: var(--space-2);
  padding: 0;
  margin: 0;
  list-style: none;
}
.activity__book {
  display: grid;
  grid-template-columns: 40px minmax(0, 1.2fr) minmax(0, 1fr) 90px;
  align-items: center;
  gap: var(--space-7);
  width: 100%;
  padding: var(--space-4);
  border-radius: var(--radius-md);
  text-align: left;
}
.activity__book:hover:not(:disabled) {
  background: var(--wash);
}
.activity__book:disabled {
  opacity: 1;
  cursor: default;
}
.activity__thumb {
  border-radius: var(--radius-sm);
  box-shadow: none;
}
.activity__book-text {
  display: grid;
  gap: var(--space-1);
  min-width: 0;
}
.activity__book-text b {
  font-weight: 700;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.activity__book-text small {
  color: var(--dim);
  font-weight: 500;
}
.activity__book-time {
  text-align: right;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}
@media (max-width: 860px) {
  .activity {
    padding: var(--space-4) var(--space-8) var(--space-16);
  }
  .activity__tiles {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .activity__pair {
    grid-template-columns: minmax(0, 1fr);
  }
  .activity__book {
    grid-template-columns: 40px minmax(0, 1fr) 76px;
  }
  .activity__track--book {
    display: none;
  }
}
</style>
