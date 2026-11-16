import { expect, it, vi } from "vitest";
import { beginShowing, countFrames, endShowing, frameLine } from "@/frames";

/** A display that draws when told to: each `draw(at)` runs the frames asked for so far. */
function display(start = 0) {
  let asked: FrameRequestCallback[] = [],
    clock = start;
  return {
    frame: (callback: FrameRequestCallback) => {
      asked.push(callback);
      return asked.length;
    },
    now: () => clock,
    draw(at: number) {
      clock = at;
      const due = asked;
      asked = [];
      due.forEach((callback) => callback(at));
    },
    waiting: () => asked.length,
  };
}

it("counts the frames drawn and the ones that came late", () => {
  const screen = display(1_000);
  const counting = countFrames(screen.frame, screen.now);
  for (const at of [1_016, 1_032, 1_048, 1_096, 1_112]) screen.draw(at);
  expect(counting.stop()).toEqual({ frames: 5, dropped: 1, worstMs: 48, tookMs: 112 });
});

it("asks for no more frames once stopped", () => {
  const screen = display();
  const counting = countFrames(screen.frame, screen.now);
  screen.draw(16);
  counting.stop();
  screen.draw(32);
  expect(screen.waiting()).toBe(0);
});

it("is logged as one debug line with its counts", () => {
  const line = frameLine("scroll", "reader", { frames: 120, dropped: 2, worstMs: 50, tookMs: 2_000 });
  expect(line).toEqual({
    level: "debug",
    event: "ui.frames",
    msg: "Scrolled",
    command: "scroll",
    view: "reader",
    frames: 120,
    dropped: 2,
    worstMs: 50,
    tookMs: 2_000,
  });
});

it("times an opening until its view has come in, and a change of view does not restart it", () => {
  const screen = display();
  vi.stubGlobal("requestAnimationFrame", screen.frame);
  vi.stubGlobal("performance", { now: screen.now });
  beginShowing("open");
  screen.draw(16);
  beginShowing("view");
  screen.draw(64);
  expect(endShowing()).toEqual({ motion: "open", report: { frames: 2, dropped: 1, worstMs: 48, tookMs: 64 } });
  expect(endShowing()).toBeNull();
  vi.unstubAllGlobals();
});

it("does nothing where the window cannot draw frames", () => {
  vi.stubGlobal("requestAnimationFrame", undefined);
  beginShowing("view");
  expect(endShowing()).toBeNull();
  vi.unstubAllGlobals();
});
