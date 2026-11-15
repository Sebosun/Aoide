<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { CalendarDays, Clock3, Flame, Gauge } from "@lucide/vue";
import { api } from "../api";
import type { ListeningStats } from "../api/types";
import CoverArt from "../components/CoverArt.vue";
import UiSkeleton from "../components/ui/UiSkeleton.vue";
import { formatDuration } from "../estimate";
import { dayKey, heatmap, lastDays, lastMonths, streaks, summary, weekdays, type Day } from "../stats";
import { useLibraryStore } from "../stores/library";
import { useNavigationStore } from "../stores/navigation";

/** Listening over a chosen range: totals, a streak, a bar per day (or month), a calendar of
 *  the last half year, weekdays, and time per book. */
type Range = "week" | "month" | "year" | "all";
const RANGES: { id: Range; label: string }[] = [
  { id: "week", label: "7 days" },
  { id: "month", label: "30 days" },
  { id: "year", label: "12 months" },
  { id: "all", label: "All" },
];
const library = useLibraryStore(),
  navigation = useNavigationStore();
const stats = ref<ListeningStats | null>(null),
  error = ref(""),
  range = ref<Range>("week");
onMounted(async () => {
  void library.start();
  try {
    stats.value = await api.listeningStats();
  } catch (e) {
    error.value = String(e);
  }
});

const today = new Date();
const days = computed<Day[]>(() => stats.value?.byDay ?? []);
const first = computed(() => days.value.find((d) => d.ms > 0)?.day ?? dayKey(today));
const spanDays = computed(() =>
  Math.max(
    1,
    Math.round(
      (new Date(`${dayKey(today)}T12:00:00`).getTime() - new Date(`${first.value}T12:00:00`).getTime()) / 86_400_000,
    ) + 1,
  ),
);
const series = computed(() =>
  lastDays(
    days.value,
    today,
    range.value === "week" ? 7 : range.value === "month" ? 30 : range.value === "year" ? 365 : spanDays.value,
  ),
);
/** Over 12 months, averages count from the first day of listening, so a new library is not
 *  diluted by months before it existed; 7 and 30 days are always whole. */
const counted = computed(() =>
  range.value === "year" ? series.value.filter((d) => d.day >= first.value) : series.value,
);
const totals = computed(() => summary(counted.value.length ? counted.value : series.value));
const streak = computed(() => streaks(days.value, today));
const monthly = computed(() => range.value === "year" || (range.value === "all" && spanDays.value > 62));
const bars = computed(() => {
  if (!monthly.value)
    return series.value.map((d, i, all) => ({
      key: d.day,
      ms: d.ms,
      label: dayLabel(d.day, all.length - 1 - i),
      title: new Date(`${d.day}T12:00:00`).toLocaleDateString(undefined, {
        weekday: "short",
        day: "numeric",
        month: "short",
      }),
      now: d.day === dayKey(today),
    }));
  const count = range.value === "year" ? 12 : Math.ceil(spanDays.value / 30.4) + 1;
  return lastMonths(days.value, today, count).map((m) => {
    const date = new Date(`${m.month}-15T12:00:00`);
    return {
      key: m.month,
      ms: m.ms,
      label: date.toLocaleDateString(undefined, { month: "narrow" }),
      title: date.toLocaleDateString(undefined, { month: "long", year: "numeric" }),
      now: m.month === dayKey(today).slice(0, 7),
    };
  });
});
const barMax = computed(() => Math.max(1, ...bars.value.map((b) => b.ms)));
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
      book,
      title: book?.title ?? entry.title,
      percent: Math.round((book?.progress?.fraction ?? 0) * 100),
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
      <div class="activity__ranges" role="group" aria-label="Range">
        <button
          v-for="r in RANGES"
          :key="r.id"
          type="button"
          class="activity__range"
          :class="{ 'activity__range--on': range === r.id }"
          :aria-pressed="range === r.id"
          @click="range = r.id"
        >
          {{ r.label }}
        </button>
      </div>
    </header>
    <p v-if="error" class="activity__error" role="alert">{{ error }}</p>

    <template v-if="!stats && !error">
      <p class="visually-hidden" role="status" aria-label="Loading listening stats">Loading listening stats…</p>
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
        <div class="activity__tile activity__tile--hero" role="group" aria-label="Listened">
          <Clock3 :size="18" /><span>Listened</span><strong>{{ time(totals.totalMs) }}</strong>
        </div>
        <div class="activity__tile" role="group" aria-label="Daily average">
          <Gauge :size="18" /><span>Daily average</span><strong>{{ time(totals.averageMs) }}</strong>
        </div>
        <div class="activity__tile" role="group" aria-label="Streak">
          <Flame :size="18" /><span>Streak</span
          ><strong>{{ streak.current }} {{ streak.current === 1 ? "day" : "days" }}</strong
          ><small>best {{ streak.best }}</small>
        </div>
        <div class="activity__tile" role="group" aria-label="Active days">
          <CalendarDays :size="18" /><span>Days listened</span><strong>{{ totals.activeDays }}</strong
          ><small>of {{ counted.length || series.length }}</small>
        </div>
      </div>

      <p v-if="!stats.totalMs" class="activity__empty">Nothing yet. Listening time appears here as you play books.</p>
      <template v-else>
        <section class="activity__card activity__chart">
          <ol
            class="activity__bars"
            :class="{ 'activity__bars--dense': bars.length > 14 }"
            :aria-label="monthly ? 'Listening by month' : 'Listening by day'"
          >
            <li
              v-for="bar in bars"
              :key="bar.key"
              class="activity__bar"
              :class="{ 'activity__bar--now': bar.now }"
              :title="`${bar.title}: ${time(bar.ms)}`"
              :aria-label="`${bar.title}: ${time(bar.ms)}`"
            >
              <span
                class="activity__bar-fill"
                :style="{ height: bar.ms ? `${Math.max(3, (bar.ms / barMax) * 100)}%` : '0' }"
              />
              <span class="activity__bar-label" aria-hidden="true">{{ bar.label }}</span>
            </li>
          </ol>
        </section>

        <div class="activity__pair">
          <section class="activity__card">
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
          </section>
          <section class="activity__card" aria-label="By weekday">
            <h2 class="activity__heading">By weekday</h2>
            <p v-if="favourite" class="activity__note">Most on {{ favourite }}</p>
            <ul class="activity__weekdays">
              <li v-for="w in week" :key="w.label" :title="time(w.ms)">
                <span>{{ w.label }}</span><span class="activity__track"
                  ><i :style="{ width: `${(w.ms / weekMax) * 100}%` }" /></span
                ><b>{{ time(w.ms) }}</b>
              </li>
            </ul>
          </section>
        </div>

        <section class="activity__card">
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
                  ><small>{{
                    row.book
                      ? row.percent >= 99
                        ? "Finished"
                        : `${row.percent}% listened`
                      : "Removed from the library"
                  }}</small></span
                >
                <span class="activity__track activity__track--book"
                  ><i :style="{ width: `${(row.ms / bookMax) * 100}%` }"
                /></span>
                <span class="activity__book-time">{{ time(row.ms) }}</span>
              </button>
            </li>
          </ul>
        </section>
      </template>
    </template>
  </section>
</template>

<style scoped>
.activity {
  max-width: 1120px;
  margin: 0 auto;
  padding: 8px 28px 40px;
  display: grid;
  gap: 18px;
}
.activity__header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.activity__title {
  font-size: 32px;
  font-weight: 800;
  letter-spacing: -0.03em;
}
.activity__ranges {
  display: flex;
  gap: 2px;
  padding: 4px;
  border-radius: 999px;
  background: var(--glass);
}
.activity__range {
  padding: 7px 14px;
  border-radius: 999px;
  font-weight: 600;
  font-size: 13px;
  color: var(--dim);
}
.activity__range--on {
  background: var(--ink);
  color: var(--ground);
}
.activity__error {
  color: var(--accent);
  font-weight: 600;
}
.activity__tiles {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 12px;
}
.activity__tile {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: center;
  gap: 4px 10px;
  padding: 16px 18px;
  border-radius: 18px;
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
  margin-top: -4px;
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
  border-radius: 18px;
}
.activity__ghost-card {
  height: 240px;
  border-radius: 20px;
}
.activity__empty {
  padding: 40px 20px;
  border-radius: 20px;
  background: var(--page);
  color: var(--dim);
  text-align: center;
}
.activity__card {
  display: grid;
  gap: 12px;
  align-content: start;
  padding: 20px;
  border-radius: 20px;
  background: var(--page);
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
  margin-left: 6px;
  font-weight: 600;
  color: var(--faint);
  text-transform: none;
  letter-spacing: 0;
}
.activity__note {
  margin-top: -6px;
  font-size: 13px;
  font-weight: 700;
}
.activity__bars {
  display: flex;
  align-items: stretch;
  gap: 8px;
  height: 200px;
  padding: 0;
  margin: 0;
  list-style: none;
}
.activity__bars--dense {
  gap: 3px;
}
.activity__bar {
  flex: 1;
  min-width: 0;
  display: grid;
  grid-template-rows: minmax(0, 1fr) 18px;
  align-items: end;
  justify-items: center;
  gap: 6px;
}
.activity__bar-fill {
  width: 100%;
  max-width: 46px;
  border-radius: 6px 6px 3px 3px;
  background: var(--hero);
  transition: height var(--dur-2) var(--ease);
}
.activity__bar:hover .activity__bar-fill {
  background: var(--faint);
}
.activity__bar--now .activity__bar-fill {
  background: var(--accent);
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
  gap: 18px;
}
.activity__calendar {
  display: grid;
  grid-template-columns: repeat(26, minmax(0, 1fr));
  gap: 3px;
}
.activity__week {
  display: grid;
  grid-template-rows: repeat(7, auto);
  gap: 3px;
}
.activity__cell {
  display: block;
  width: 100%;
  aspect-ratio: 1;
  border-radius: 3px;
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
  gap: 3px;
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
  gap: 7px;
  padding: 0;
  margin: 0;
  list-style: none;
  font-size: 12.5px;
}
.activity__weekdays li {
  display: grid;
  grid-template-columns: 34px minmax(0, 1fr) 76px;
  align-items: center;
  gap: 10px;
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
  border-radius: 4px;
  background: var(--wash);
  overflow: hidden;
}
.activity__track i {
  position: absolute;
  inset: 0 auto 0 0;
  border-radius: 4px;
  background: var(--ink);
}
.activity__books {
  display: grid;
  gap: 4px;
  padding: 0;
  margin: 0;
  list-style: none;
}
.activity__book {
  display: grid;
  grid-template-columns: 40px minmax(0, 1.2fr) minmax(0, 1fr) 90px;
  align-items: center;
  gap: 14px;
  width: 100%;
  padding: 8px;
  border-radius: 12px;
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
  border-radius: 8px;
  box-shadow: none;
}
.activity__book-text {
  display: grid;
  gap: 2px;
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
.activity__track--book i {
  background: var(--accent);
}
.activity__book-time {
  text-align: right;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}
@media (max-width: 860px) {
  .activity {
    padding: 8px 16px 32px;
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
