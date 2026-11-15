/** Local days and ages, as the screens name them. */

const DAY = 86_400_000;
/** The local day `ms` falls on, counted from the epoch. */
export const localDay = (ms: number) => Math.floor((ms - new Date(ms).getTimezoneOffset() * 60_000) / DAY);

/** "today", "yesterday", a weekday within the week, else the date. */
export function dayName(at: number, now = Date.now()) {
  const ago = localDay(now) - localDay(at);
  if (ago <= 0) return "today";
  if (ago === 1) return "yesterday";
  if (ago < 7) return new Date(at).toLocaleDateString(undefined, { weekday: "long" });
  return new Date(at).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}

/** How long ago: "just now", "20 minutes ago", "2 hours ago", then the day ("yesterday"). */
export function age(at: number, now = Date.now()) {
  const minutes = Math.floor((now - at) / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return minutes === 1 ? "1 minute ago" : `${minutes} minutes ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return hours === 1 ? "1 hour ago" : `${hours} hours ago`;
  return dayName(at, now);
}
