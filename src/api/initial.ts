import type { GenerationState, PlayerState } from "@/api/types";

/** What the player shows before the core has said anything: stopped, no book. */
export const initialPlayer = (): PlayerState => ({
  bookId: null,
  status: "stopped",
  chapter: 0,
  segmentId: null,
  wordIndex: null,
  speed: 1,
  volume: 1,
  chapterFraction: 0,
  error: null,
  sleep: null,
  source: "speech",
  offText: false,
  chapterLeftMs: null,
  waitingForSync: false,
  positionMs: null,
  durationMs: null,
  partPositionMs: null,
});
/** What generation shows before the core has said anything: idle, no book. */
export const initialGeneration = (): GenerationState => ({
  bookId: null,
  generatingSegmentId: null,
  bufferedAheadMs: 0,
  pending: 0,
  error: null,
  preparing: null,
});
