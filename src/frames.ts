import type { UiLog } from "@/api/contract";

/** How smoothly the window drew while something moved. */
export type FrameReport = {
  frames: number;
  /** Frames that came more than one refresh late. */
  dropped: number;
  /** The longest wait for a frame. */
  worstMs: number;
  tookMs: number;
};

/** A frame later than this missed at least one refresh. WebKitGTK draws every 16 ms when it
 *  cannot follow the display (an NVIDIA card), and at 60 Hz otherwise. */
export const LATE_MS = 24;

/** Counts animation frames until `stop`, which says how it went. `frame` and `now` are the
 *  window's own unless a test passes others. */
export function countFrames(
  frame: (callback: FrameRequestCallback) => number = (callback) => requestAnimationFrame(callback),
  now: () => number = () => performance.now(),
) {
  const start = now();
  let last = start,
    frames = 0,
    dropped = 0,
    worst = 0,
    stopped = false;
  const tick = (at: number) => {
    if (stopped) return;
    const gap = at - last;
    last = at;
    frames += 1;
    if (gap > LATE_MS) dropped += 1;
    worst = Math.max(worst, gap);
    frame(tick);
  };
  frame(tick);
  return {
    stop(): FrameReport {
      stopped = true;
      return { frames, dropped, worstMs: Math.round(worst), tookMs: Math.round(now() - start) };
    },
  };
}

const MESSAGES = { scroll: "Scrolled", open: "Opened an item", view: "Changed view" } as const;
export type Motion = keyof typeof MESSAGES;

/** Whether this window draws animation frames (a test's DOM may not). */
export const canCount = () => typeof globalThis.requestAnimationFrame === "function";

let showing: { motion: Motion; counting: ReturnType<typeof countFrames> } | null = null;

/** Starts timing an opening or a change of view until `endShowing`. An opening replaces what
 *  was being timed; a change of view is only timed when nothing else is. */
export function beginShowing(motion: Motion) {
  if (!canCount() || (showing && motion !== "open")) return;
  showing?.counting.stop();
  showing = { motion, counting: countFrames() };
}

/** Ends the timing begun by `beginShowing`, once the view has come in. */
export function endShowing(): { motion: Motion; report: FrameReport } | null {
  if (!showing) return null;
  const { motion, counting } = showing;
  showing = null;
  return { motion, report: counting.stop() };
}

/** Drops a timing whose view never came (the opening failed). */
export function cancelShowing() {
  showing?.counting.stop();
  showing = null;
}

/** The log line (`ui.frames`) for one scroll, opening or change of view. */
export function frameLine(motion: Motion, view: string, report: FrameReport): UiLog {
  return { level: "debug", event: "ui.frames", msg: MESSAGES[motion], command: motion, view, ...report };
}
