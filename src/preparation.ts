import type { Component } from "vue";
import { Pause, Play, Sparkles, X } from "@lucide/vue";
import { api } from "./api";
import type { BookSummary, PrepareState } from "./api/types";
import { useUiStore } from "./stores/ui";

/** What a person can do about a book's whole-book preparation. */
export type PrepareAction = "prepare" | "start" | "pause" | "resume" | "cancel";

/** How a book's preparation reads and what it offers, for every view that shows it. The stage
 *  itself is decided in the core (core/preparation.rs); this only presents it. */
export interface PreparationStatus {
  state: PrepareState | null;
  /** "Preparing", "Queued", …; null when the book is not being prepared. */
  label: string | null;
  /** "Chapter 3", "Chapters 2 and 5" when only some chapters are prepared; null for the whole book. */
  scope: string | null;
  /** "3 / 120" segments, while the size is known and the book is not done. */
  count: string | null;
  percent: number;
  actions: PrepareAction[];
}

const LABELS: Record<PrepareState, string> = {
  waiting: "Waiting to start", queued: "Queued", preparing: "Preparing", paused: "Paused", done: "Ready offline",
};
const ACTIONS: Record<PrepareState | "none", PrepareAction[]> = {
  none: ["prepare"], waiting: ["start", "cancel"], queued: ["pause", "cancel"], preparing: ["pause", "cancel"], paused: ["resume", "cancel"], done: [],
};
/** Button and menu wording for each action (also its accessible name, with the title appended in lists). */
export const ACTION_LABELS: Record<PrepareAction, string> = {
  prepare: "Prepare whole book", start: "Start preparing", pause: "Pause preparing", resume: "Resume preparing", cancel: "Cancel preparing",
};

export const ACTION_ICONS: Record<PrepareAction, Component> = {
  prepare: Sparkles, start: Play, pause: Pause, resume: Play, cancel: X,
};

export function preparationStatus(prepare: BookSummary["prepare"]): PreparationStatus {
  if (!prepare) return { state: null, label: null, scope: null, count: null, percent: 0, actions: ACTIONS.none };
  const { state, done, total, chapters } = prepare;
  return {
    state,
    label: LABELS[state],
    scope: chapterScope(chapters),
    count: total && state !== "done" ? `${done.toLocaleString()} / ${total.toLocaleString()}` : null,
    percent: state === "done" ? 100 : total ? Math.floor((done / total) * 100) : 0,
    // A chapter-only preparation can still be widened to the whole book.
    actions: chapters && state !== "done" ? ["prepare", ...ACTIONS[state]] : ACTIONS[state],
  };
}

/** "Chapter 3", "Chapters 2 and 5", "Chapters 1, 2 and 5" (numbered from 1); null for none. */
function chapterScope(chapters: number[] | null): string | null {
  if (!chapters?.length) return null;
  const numbers = chapters.map(c => String(c + 1));
  if (numbers.length === 1) return `Chapter ${numbers[0]}`;
  return `Chapters ${numbers.slice(0, -1).join(", ")} and ${numbers.at(-1)}`;
}

/** "Preparing · 3 / 120", "Waiting to start", "Ready offline"; empty when not prepared. */
export function describePreparation(status: PreparationStatus): string {
  return [status.label, status.scope, status.count].filter(Boolean).join(" · ");
}

/** Queues one chapter (`chapter` counts from 0) and says so; a failure is told instead. */
export async function prepareChapter(bookId: string, chapter: number): Promise<void> {
  const ui = useUiStore();
  try {
    await api.prepareChapter(bookId, chapter);
    ui.toast(`Preparing chapter ${chapter + 1} in the background`);
  } catch (e) {
    ui.toast(String(e), "error");
  }
}

/** Runs an action; failures and the start of a preparation are told in a toast. */
export async function runPrepareAction(bookId: string, action: PrepareAction): Promise<void> {
  const ui = useUiStore();
  try {
    if (action === "prepare") {
      await api.prepareBook(bookId);
      ui.toast("Preparing the whole book in the background");
    } else if (action === "start") await api.prepareBook(bookId);
    else if (action === "pause" || action === "resume") await api.setPreparePaused(bookId, action === "pause");
    else await api.cancelPrepare(bookId);
  } catch (e) {
    ui.toast(String(e), "error");
  }
}
