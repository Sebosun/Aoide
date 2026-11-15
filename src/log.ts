import type { UiLog } from "@/api/contract";

/** How long the same line stays out of the log after it was sent. */
const QUIET_MS = 10_000;
const sent = new Map<string, number>();

/** Sends `entry` to Aoide's log with `send`, unless the same line went within the last 10 s.
 *  Logging never fails its caller. */
export function report(entry: UiLog, send: (entry: UiLog) => Promise<unknown>) {
  const key = [entry.event, entry.command, entry.msg].join("\u0000");
  const now = Date.now();
  if (now - (sent.get(key) ?? -Infinity) < QUIET_MS) return;
  sent.set(key, now);
  void send(entry).catch(() => {});
}

/** Forgets which lines were sent (tests). */
export function forget() {
  sent.clear();
}
