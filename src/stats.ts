/** Listening statistics from per-day totals (`listening` rows): local "YYYY-MM-DD" days. */
export interface Day { day: string; ms: number }

const pad = (n: number) => String(n).padStart(2, "0");
/** The local calendar day of a date, as stored by Aoide. */
export const dayKey = (date: Date) => `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
const addDays = (date: Date, n: number) => new Date(date.getFullYear(), date.getMonth(), date.getDate() + n, 12);
const totals = (days: Day[]) => new Map(days.map(d => [d.day, d.ms]));

/** The last `count` days ending today, oldest first, with zeros for silent days. */
export function lastDays(days: Day[], today: Date, count: number): Day[] {
  const byDay = totals(days);
  return Array.from({ length: count }, (_, i) => {
    const day = dayKey(addDays(today, i - count + 1));
    return { day, ms: byDay.get(day) ?? 0 };
  });
}

/** The last `count` calendar months ending this month, oldest first ("YYYY-MM"). */
export function lastMonths(days: Day[], today: Date, count: number): { month: string; ms: number }[] {
  const months = Array.from({ length: count }, (_, i) => {
    const date = new Date(today.getFullYear(), today.getMonth() - count + 1 + i, 1, 12);
    return { month: `${date.getFullYear()}-${pad(date.getMonth() + 1)}`, ms: 0 };
  });
  const index = new Map(months.map((m, i) => [m.month, i]));
  for (const { day, ms } of days) {
    const at = index.get(day.slice(0, 7));
    if (at !== undefined) months[at].ms += ms;
  }
  return months;
}

/** Total, average per day of the range, and days with any listening. */
export function summary(range: Day[]) {
  const totalMs = range.reduce((n, d) => n + d.ms, 0);
  return { totalMs, averageMs: range.length ? Math.round(totalMs / range.length) : 0, activeDays: range.filter(d => d.ms > 0).length };
}

/** Consecutive listening days: the current run (through today, or through yesterday when
 *  today has no listening yet) and the longest run. */
export function streaks(days: Day[], today: Date) {
  const listened = new Set(days.filter(d => d.ms > 0).map(d => d.day));
  let current = 0;
  let cursor = listened.has(dayKey(today)) ? today : addDays(today, -1);
  while (listened.has(dayKey(cursor))) { current += 1; cursor = addDays(cursor, -1); }
  let best = 0, run = 0, previous: string | null = null;
  for (const day of [...listened].sort()) {
    const [y, m, d] = day.split("-").map(Number);
    run = previous === dayKey(addDays(new Date(y, m - 1, d, 12), -1)) ? run + 1 : 1;
    best = Math.max(best, run);
    previous = day;
  }
  return { current, best };
}

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const weekday = (day: string) => (new Date(`${day}T12:00:00`).getDay() + 6) % 7;

/** Listening per weekday, Monday first. */
export function weekdays(days: Day[]) {
  const sums = WEEKDAYS.map(label => ({ label, ms: 0 }));
  for (const { day, ms } of days) sums[weekday(day)].ms += ms;
  return sums;
}

/** `count` whole weeks (Monday first) ending with this week; days after today are null.
 *  Level 0 is no listening, 1–4 rise with the share of the busiest day. */
export function heatmap(days: Day[], today: Date, count: number): { day: string | null; ms: number; level: number }[][] {
  const byDay = totals(days);
  const monday = addDays(today, -weekday(dayKey(today)) - (count - 1) * 7);
  const end = dayKey(today);
  const weeks = Array.from({ length: count }, (_, w) => Array.from({ length: 7 }, (_, d) => {
    const day = dayKey(addDays(monday, w * 7 + d));
    return day > end ? { day: null, ms: 0 } : { day, ms: byDay.get(day) ?? 0 };
  }));
  const busiest = Math.max(0, ...weeks.flat().map(c => c.ms));
  return weeks.map(week => week.map(cell => ({ ...cell, level: cell.ms && busiest ? Math.max(1, Math.ceil((cell.ms / busiest) * 4)) : 0 })));
}
