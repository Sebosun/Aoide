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
const RANGES: { id: Range; label: string }[] = [{ id: "week", label: "7 days" }, { id: "month", label: "30 days" }, { id: "year", label: "12 months" }, { id: "all", label: "All" }];
const library = useLibraryStore(), navigation = useNavigationStore();
const stats = ref<ListeningStats | null>(null), error = ref(""), range = ref<Range>("week");
onMounted(async () => {
  void library.start();
  try { stats.value = await api.listeningStats(); } catch (e) { error.value = String(e); }
});

const today = new Date();
const days = computed<Day[]>(() => stats.value?.byDay ?? []);
const first = computed(() => days.value.find(d => d.ms > 0)?.day ?? dayKey(today));
const spanDays = computed(() => Math.max(1, Math.round((new Date(`${dayKey(today)}T12:00:00`).getTime() - new Date(`${first.value}T12:00:00`).getTime()) / 86_400_000) + 1));
const series = computed(() => lastDays(days.value, today, range.value === "week" ? 7 : range.value === "month" ? 30 : range.value === "year" ? 365 : spanDays.value));
/** Over 12 months, averages count from the first day of listening, so a new library is not
 *  diluted by months before it existed; 7 and 30 days are always whole. */
const counted = computed(() => range.value === "year" ? series.value.filter(d => d.day >= first.value) : series.value);
const totals = computed(() => summary(counted.value.length ? counted.value : series.value));
const streak = computed(() => streaks(days.value, today));
const monthly = computed(() => range.value === "year" || (range.value === "all" && spanDays.value > 62));
const bars = computed(() => {
  if (!monthly.value) return series.value.map((d, i, all) => ({ key: d.day, ms: d.ms, label: dayLabel(d.day, all.length - 1 - i), title: new Date(`${d.day}T12:00:00`).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" }), now: d.day === dayKey(today) }));
  const count = range.value === "year" ? 12 : Math.ceil(spanDays.value / 30.4) + 1;
  return lastMonths(days.value, today, count).map(m => {
    const date = new Date(`${m.month}-15T12:00:00`);
    return { key: m.month, ms: m.ms, label: date.toLocaleDateString(undefined, { month: "narrow" }), title: date.toLocaleDateString(undefined, { month: "long", year: "numeric" }), now: m.month === dayKey(today).slice(0, 7) };
  });
});
const barMax = computed(() => Math.max(1, ...bars.value.map(b => b.ms)));
/** Weekday initials for a week; for longer ranges the date of today and every seventh day back. */
function dayLabel(day: string, daysAgo: number) {
  const date = new Date(`${day}T12:00:00`);
  if (range.value === "week") return date.toLocaleDateString(undefined, { weekday: "narrow" });
  return daysAgo % 7 === 0 ? date.toLocaleDateString(undefined, { day: "numeric", month: "short" }) : "";
}
const calendar = computed(() => heatmap(days.value, today, 26));
const week = computed(() => weekdays(days.value));
const weekMax = computed(() => Math.max(1, ...week.value.map(w => w.ms)));
const favourite = computed(() => {
  const best = [...week.value].sort((a, b) => b.ms - a.ms)[0];
  if (!best?.ms) return "";
  const names: Record<string, string> = { Mon: "Mondays", Tue: "Tuesdays", Wed: "Wednesdays", Thu: "Thursdays", Fri: "Fridays", Sat: "Saturdays", Sun: "Sundays" };
  return names[best.label];
});
const books = computed(() => (stats.value?.byBook ?? []).map(entry => {
  const book = library.books.find(b => b.id === entry.bookId);
  return { ...entry, book, title: book?.title ?? entry.title, percent: Math.round((book?.progress?.fraction ?? 0) * 100) };
}));
const bookMax = computed(() => Math.max(1, ...books.value.map(b => b.ms)));
const time = (ms: number) => (ms ? formatDuration(ms) : "0 min");
const cellTitle = (cell: { day: string | null; ms: number }) => cell.day ? `${new Date(`${cell.day}T12:00:00`).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" })}: ${cell.ms ? formatDuration(cell.ms) : "nothing"}` : "";
</script>

<template>
  <section class="stats" :aria-busy="!stats && !error">
    <header class="stats__header">
      <h1 class="stats__title">Listening</h1>
      <div class="stats__ranges" role="group" aria-label="Range">
        <button v-for="r in RANGES" :key="r.id" type="button" class="stats__range" :class="{ 'stats__range--on': range === r.id }" :aria-pressed="range === r.id" @click="range = r.id">{{ r.label }}</button>
      </div>
    </header>
    <p v-if="error" class="stats__error" role="alert">{{ error }}</p>

    <template v-if="!stats && !error">
      <p class="visually-hidden" role="status" aria-label="Loading listening stats">Loading listening stats…</p>
      <div class="stats__tiles" aria-hidden="true"><UiSkeleton v-for="n in 4" :key="n" class="stats__ghost-tile" /></div>
      <UiSkeleton class="stats__ghost-card" />
      <div class="stats__pair" aria-hidden="true"><UiSkeleton class="stats__ghost-card" /><UiSkeleton class="stats__ghost-card" /></div>
    </template>
    <template v-if="stats">
      <div class="stats__tiles">
        <div class="stats__tile stats__tile--hero" role="group" aria-label="Listened"><Clock3 :size="18" /><span>Listened</span><strong>{{ time(totals.totalMs) }}</strong></div>
        <div class="stats__tile" role="group" aria-label="Daily average"><Gauge :size="18" /><span>Daily average</span><strong>{{ time(totals.averageMs) }}</strong></div>
        <div class="stats__tile" role="group" aria-label="Streak"><Flame :size="18" /><span>Streak</span><strong>{{ streak.current }} {{ streak.current === 1 ? "day" : "days" }}</strong><small>best {{ streak.best }}</small></div>
        <div class="stats__tile" role="group" aria-label="Active days"><CalendarDays :size="18" /><span>Days listened</span><strong>{{ totals.activeDays }}</strong><small>of {{ counted.length || series.length }}</small></div>
      </div>

      <p v-if="!stats.totalMs" class="stats__empty">Nothing yet. Listening time appears here as you play books.</p>
      <template v-else>
        <section class="stats__card stats__chart">
          <ol class="stats__bars" :class="{ 'stats__bars--dense': bars.length > 14 }" :aria-label="monthly ? 'Listening by month' : 'Listening by day'">
            <li v-for="bar in bars" :key="bar.key" class="stats__bar" :class="{ 'stats__bar--now': bar.now }" :title="`${bar.title}: ${time(bar.ms)}`" :aria-label="`${bar.title}: ${time(bar.ms)}`">
              <span class="stats__bar-fill" :style="{ height: bar.ms ? `${Math.max(3, (bar.ms / barMax) * 100)}%` : '0' }" />
              <span class="stats__bar-label" aria-hidden="true">{{ bar.label }}</span>
            </li>
          </ol>
        </section>

        <div class="stats__pair">
          <section class="stats__card">
            <h2 class="stats__heading">Last 26 weeks</h2>
            <div class="stats__calendar" role="img" aria-label="Last 26 weeks">
              <div v-for="(column, w) in calendar" :key="w" class="stats__week">
                <span v-for="(cell, d) in column" :key="d" class="stats__cell" :class="[`stats__cell--${cell.level}`, { 'stats__cell--future': !cell.day }]" :data-day="cell.day ?? undefined" :title="cellTitle(cell)" />
              </div>
            </div>
            <div class="stats__scale" aria-hidden="true">Less <i v-for="n in 5" :key="n" :class="`stats__cell stats__cell--${n - 1}`" /> More</div>
          </section>
          <section class="stats__card" aria-label="By weekday">
            <h2 class="stats__heading">By weekday</h2>
            <p v-if="favourite" class="stats__note">Most on {{ favourite }}</p>
            <ul class="stats__weekdays">
              <li v-for="w in week" :key="w.label" :title="time(w.ms)"><span>{{ w.label }}</span><span class="stats__track"><i :style="{ width: `${(w.ms / weekMax) * 100}%` }" /></span><b>{{ time(w.ms) }}</b></li>
            </ul>
          </section>
        </div>

        <section class="stats__card">
          <h2 class="stats__heading">By book <small>all time</small></h2>
          <ul class="stats__books" aria-label="By book">
            <li v-for="row in books" :key="row.bookId">
              <button type="button" class="stats__book" :disabled="!row.book" @click="row.book && navigation.openBook(row.bookId)">
                <CoverArt class="stats__thumb" :title="row.title" :url="row.book?.coverUrl" :label="false" shape="square" />
                <span class="stats__book-text"><b>{{ row.title }}</b><small>{{ row.book ? (row.percent >= 99 ? "Finished" : `${row.percent}% listened`) : "Removed from the library" }}</small></span>
                <span class="stats__track stats__track--book"><i :style="{ width: `${(row.ms / bookMax) * 100}%` }" /></span>
                <span class="stats__book-time">{{ time(row.ms) }}</span>
              </button>
            </li>
          </ul>
        </section>
      </template>
    </template>
  </section>
</template>

<style scoped>
.stats { max-width: 1120px; margin: 0 auto; padding: 8px 28px 40px; display: grid; gap: 18px; }
.stats__header { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 12px; }
.stats__title { font-size: 32px; font-weight: 800; letter-spacing: -.03em; }
.stats__ranges { display: flex; gap: 2px; padding: 4px; border-radius: 999px; background: var(--glass); }
.stats__range { padding: 7px 14px; border-radius: 999px; font-weight: 600; font-size: 13px; color: var(--dim); }
.stats__range--on { background: var(--ink); color: var(--ground); }
.stats__error { color: var(--accent); font-weight: 600; }
.stats__tiles { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 12px; }
.stats__tile { display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: center; gap: 4px 10px; padding: 16px 18px; border-radius: 18px; background: var(--page); color: var(--dim); font-size: 12.5px; font-weight: 600; }
.stats__tile strong { grid-column: 1 / -1; font-size: 26px; font-weight: 800; letter-spacing: -.03em; color: var(--ink); font-variant-numeric: tabular-nums; }
.stats__tile small { grid-column: 1 / -1; margin-top: -4px; font-size: 12px; color: var(--faint); }
.stats__tile--hero { background: var(--hero); }
.stats__tile--hero svg { color: var(--accent); }
.stats__ghost-tile { height: 96px; border-radius: 18px; }
.stats__ghost-card { height: 240px; border-radius: 20px; }
.stats__empty { padding: 40px 20px; border-radius: 20px; background: var(--page); color: var(--dim); text-align: center; }
.stats__card { display: grid; gap: 12px; align-content: start; padding: 20px; border-radius: 20px; background: var(--page); min-width: 0; }
.stats__heading { font-size: 12px; font-weight: 700; letter-spacing: .06em; text-transform: uppercase; color: var(--dim); }
.stats__heading small { margin-left: 6px; font-weight: 600; color: var(--faint); text-transform: none; letter-spacing: 0; }
.stats__note { margin-top: -6px; font-size: 13px; font-weight: 700; }
.stats__bars { display: flex; align-items: stretch; gap: 8px; height: 200px; padding: 0; margin: 0; list-style: none; }
.stats__bars--dense { gap: 3px; }
.stats__bar { flex: 1; min-width: 0; display: grid; grid-template-rows: minmax(0, 1fr) 18px; align-items: end; justify-items: center; gap: 6px; }
.stats__bar-fill { width: 100%; max-width: 46px; border-radius: 6px 6px 3px 3px; background: var(--hero); transition: height var(--dur-2) var(--ease); }
.stats__bar:hover .stats__bar-fill { background: var(--faint); }
.stats__bar--now .stats__bar-fill { background: var(--accent); }
.stats__bar-label { font-size: 11px; font-weight: 600; color: var(--dim); white-space: nowrap; overflow: visible; }
.stats__pair { display: grid; grid-template-columns: minmax(0, 1.5fr) minmax(0, 1fr); gap: 18px; }
.stats__calendar { display: grid; grid-template-columns: repeat(26, minmax(0, 1fr)); gap: 3px; }
.stats__week { display: grid; grid-template-rows: repeat(7, auto); gap: 3px; }
.stats__cell { display: block; width: 100%; aspect-ratio: 1; border-radius: 3px; background: var(--wash); }
.stats__cell--1 { background: color-mix(in srgb, var(--accent) 30%, var(--wash)); }
.stats__cell--2 { background: color-mix(in srgb, var(--accent) 55%, var(--wash)); }
.stats__cell--3 { background: color-mix(in srgb, var(--accent) 78%, var(--wash)); }
.stats__cell--4 { background: var(--accent); }
.stats__cell--future { background: none; }
.stats__scale { display: flex; align-items: center; gap: 3px; font-size: 11px; color: var(--dim); }
.stats__scale i { width: 11px; height: 11px; aspect-ratio: auto; }
.stats__weekdays { display: grid; gap: 7px; padding: 0; margin: 0; list-style: none; font-size: 12.5px; }
.stats__weekdays li { display: grid; grid-template-columns: 34px minmax(0, 1fr) 76px; align-items: center; gap: 10px; color: var(--dim); font-weight: 600; }
.stats__weekdays b { text-align: right; color: var(--ink); font-variant-numeric: tabular-nums; }
.stats__track { position: relative; height: 8px; border-radius: 4px; background: var(--wash); overflow: hidden; }
.stats__track i { position: absolute; inset: 0 auto 0 0; border-radius: 4px; background: var(--ink); }
.stats__books { display: grid; gap: 4px; padding: 0; margin: 0; list-style: none; }
.stats__book { display: grid; grid-template-columns: 40px minmax(0, 1.2fr) minmax(0, 1fr) 90px; align-items: center; gap: 14px; width: 100%; padding: 8px; border-radius: 12px; text-align: left; }
.stats__book:hover:not(:disabled) { background: var(--wash); }
.stats__book:disabled { opacity: 1; cursor: default; }
.stats__thumb { border-radius: 8px; box-shadow: none; }
.stats__book-text { display: grid; gap: 2px; min-width: 0; }
.stats__book-text b { font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.stats__book-text small { color: var(--dim); font-weight: 500; }
.stats__track--book i { background: var(--accent); }
.stats__book-time { text-align: right; font-weight: 700; font-variant-numeric: tabular-nums; }
@media (max-width: 860px) {
  .stats { padding: 8px 16px 32px; }
  .stats__tiles { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .stats__pair { grid-template-columns: minmax(0, 1fr); }
  .stats__book { grid-template-columns: 40px minmax(0, 1fr) 76px; }
  .stats__track--book { display: none; }
}
</style>
