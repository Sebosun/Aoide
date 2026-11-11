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
  if (!prepare) return { state: null, label: null, count: null, percent: 0, actions: ACTIONS.none };
  const { state, done, total } = prepare;
  return {
    state,
    label: LABELS[state],
    count: total && state !== "done" ? `${done.toLocaleString()} / ${total.toLocaleString()}` : null,
    percent: state === "done" ? 100 : total ? Math.floor((done / total) * 100) : 0,
    actions: ACTIONS[state],
  };
}

/** "Preparing · 3 / 120", "Waiting to start", "Ready offline"; empty when not prepared. */
export function describePreparation(status: PreparationStatus): string {
  return [status.label, status.count].filter(Boolean).join(" · ");
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
