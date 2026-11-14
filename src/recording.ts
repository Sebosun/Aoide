import type { Component } from "vue";
import { BookDown, Check, Download, FileText, Headphones, Pause, Play, RotateCcw, Trash2 } from "@lucide/vue";
import type { RecordingView, SyncStage } from "./api/types";
import { formatDuration } from "./estimate";

/** What a person can do about a book's recording. */
export type RecordingAction = "listen" | "pause" | "resume" | "retry" | "accept" | "resync" | "download" | "report" | "export" | "remove";

/** How a recording's sync reads and what it offers, for every view that shows it. The stage is
 *  decided in the core (core/syncing.rs); this only presents it. */
export interface RecordingStatus {
  stage: SyncStage;
  label: string;
  detail: string | null;
  /** Share transcribed, while that is the work being done. */
  percent: number | null;
  actions: RecordingAction[];
  /** It is synced. */
  playable: boolean;
  /** What is synced so far can be listened to (all of it once synced). */
  listenable: boolean;
  /** Something is going on (shown with a moving bar). */
  working: boolean;
}

const LABELS: Record<SyncStage, string> = {
  checking: "Checking the audio", waiting: "Waiting to sync", needsModel: "Needs the speech-recognition model",
  transcribing: "Syncing", matching: "Matching to the text", ready: "Synced",
  sampleMismatch: "This may not be the same book", poorMatch: "Needs review", paused: "Paused",
  missing: "Audio files missing", failed: "Sync failed",
};
const ACTIONS: Record<SyncStage, RecordingAction[]> = {
  checking: ["remove"], waiting: ["pause", "remove"], needsModel: ["download", "remove"], transcribing: ["pause", "remove"],
  matching: [], ready: ["report", "export", "resync", "remove"], sampleMismatch: ["accept", "remove"], poorMatch: ["report", "accept", "remove"],
  paused: ["resume", "remove"], missing: ["remove"], failed: ["retry", "remove"],
};
export const RECORDING_ACTION_LABELS: Record<RecordingAction, string> = {
  listen: "Listen now", pause: "Pause syncing", resume: "Resume syncing", retry: "Try again", accept: "Use it anyway", resync: "Sync again",
  download: "Download the model", report: "See the report", export: "Export read-along EPUB", remove: "Remove recording",
};
export const RECORDING_ACTION_ICONS: Record<RecordingAction, Component> = {
  listen: Headphones, pause: Pause, resume: Play, retry: RotateCcw, accept: Check, resync: RotateCcw, download: Download, report: FileText, export: BookDown, remove: Trash2,
};

const share = (part: number, whole: number) => Math.floor((part / Math.max(1, whole)) * 100);

export function recordingStatus(recording: RecordingView): RecordingStatus {
  const { stage, transcribedMs, totalMs, summary } = recording;
  const percent = ["transcribing", "paused", "waiting"].includes(stage) && transcribedMs ? share(transcribedMs, totalMs) : stage === "transcribing" ? 0 : null;
  const found = summary ? `${share(summary.matchedWords, summary.words)}% of the text found in the recording` : null;
  const detail: Record<SyncStage, string | null> = {
    checking: null,
    waiting: recording.position ? `${recording.position} recording${recording.position === 1 ? "" : "s"} ahead` : "Starts next",
    needsModel: "Aoide hears the narration with a speech-recognition model. Download it once in Settings → Recordings.",
    transcribing: [`${percent ?? 0}%`, recording.remainingMs !== null ? `about ${formatDuration(recording.remainingMs)} left` : null].filter(Boolean).join(" · "),
    matching: null,
    ready: found,
    sampleMismatch: "Three short samples of the narration were not found in the book's text. It may be another book, edition or translation.",
    poorMatch: found ? `Only ${found}. It may be abridged, or another edition.` : "Little of the text was found in the recording.",
    paused: percent !== null ? `${percent}% synced` : null,
    missing: "The audio files were moved, renamed or changed. Put them back, or pair the book again.",
    failed: recording.error,
  };
  // While it syncs, what is synced so far can be listened to (the core waits at the edge).
  const syncing = ["waiting", "transcribing", "paused", "matching"].includes(stage) && transcribedMs > 0;
  return {
    stage, label: LABELS[stage], detail: detail[stage], percent, actions: syncing && stage !== "matching" ? ["listen", ...ACTIONS[stage]] : ACTIONS[stage],
    playable: stage === "ready", listenable: stage === "ready" || syncing, working: ["checking", "transcribing", "matching"].includes(stage),
  };
}

/** "1 file · 10 h 23 min · 20 chapters". */
export function describeFiles(recording: { files: unknown[]; totalMs: number; chapters: number }): string {
  const files = `${recording.files.length} file${recording.files.length === 1 ? "" : "s"}`;
  return [files, formatDuration(recording.totalMs), recording.chapters > 1 ? `${recording.chapters} chapters` : null].filter(Boolean).join(" · ");
}

/** Audio Aoide can pair with a book: audio files, and folders (paths without an extension). */
export const isAudioPath = (path: string) => /\.(m4b|m4a|mp4|aac|mp3|flac|ogg|oga|opus|wav|aax|aa)$/i.test(path) || !/\.[a-z0-9]{1,5}$/i.test(path.split("/").pop() ?? path);
