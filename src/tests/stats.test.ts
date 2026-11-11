import { expect, it } from "vitest";
import { dayKey, heatmap, lastDays, lastMonths, streaks, summary, weekdays } from "../stats";

const today = new Date("2026-11-11T15:00:00");
const min = 60_000;
const days = [
  { day: "2026-10-30", ms: 10 * min },
  { day: "2026-11-07", ms: 20 * min },
  { day: "2026-11-08", ms: 40 * min },
  { day: "2026-11-09", ms: 50 * min },
  { day: "2026-11-11", ms: 30 * min },
];

it("writes local day keys", () => {
  expect(dayKey(today)).toBe("2026-11-11");
  expect(dayKey(new Date("2026-01-05T00:30:00"))).toBe("2026-01-05");
});

it("fills the last days with zeros, oldest first, ending today", () => {
  const week = lastDays(days, today, 7);
  expect(week.map(d => d.day)).toEqual(["2026-11-05", "2026-11-06", "2026-11-07", "2026-11-08", "2026-11-09", "2026-11-10", "2026-11-11"]);
  expect(week.map(d => d.ms / min)).toEqual([0, 0, 20, 40, 50, 0, 30]);
});

it("sums months, oldest first, ending this month", () => {
  const months = lastMonths(days, today, 3);
  expect(months.map(m => m.month)).toEqual(["2026-09", "2026-10", "2026-11"]);
  expect(months.map(m => m.ms / min)).toEqual([0, 10, 140]);
});

it("summarises a range: total, average per day, active days", () => {
  expect(summary(lastDays(days, today, 7))).toEqual({ totalMs: 140 * min, averageMs: 20 * min, activeDays: 4 });
  expect(summary([])).toEqual({ totalMs: 0, averageMs: 0, activeDays: 0 });
});

it("counts the current streak through today, or through yesterday before today's listening, and the best", () => {
  expect(streaks(days, today)).toEqual({ current: 1, best: 3 });
  const yesterday = [...days.slice(0, 4), { day: "2026-11-10", ms: min }];
  expect(streaks(yesterday, today)).toEqual({ current: 4, best: 4 });
  expect(streaks([{ day: "2026-11-01", ms: min }], today)).toEqual({ current: 0, best: 1 });
  expect(streaks([], today)).toEqual({ current: 0, best: 0 });
});

it("adds listening up by weekday, Monday first", () => {
  const totals = weekdays(days);
  expect(totals.map(w => w.label)).toEqual(["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]);
  // 30 Oct is a Friday, 7 Nov a Saturday, 8 Nov a Sunday, 9 Nov a Monday, 11 Nov a Wednesday.
  expect(totals.map(w => w.ms / min)).toEqual([50, 0, 30, 0, 10, 20, 40]);
});

it("lays out a calendar of whole weeks, Monday first, with intensity levels", () => {
  const weeks = heatmap(days, today, 3);
  expect(weeks).toHaveLength(3);
  expect(weeks[0].map(c => c.day)[0]).toBe("2026-10-26");
  expect(weeks[2].map(c => c.day)).toEqual(["2026-11-09", "2026-11-10", "2026-11-11", null, null, null, null]);
  const level = (day: string) => weeks.flat().find(c => c.day === day)!.level;
  expect(level("2026-11-10")).toBe(0);
  expect(level("2026-10-30")).toBe(1);
  expect(level("2026-11-09")).toBe(4);
});
