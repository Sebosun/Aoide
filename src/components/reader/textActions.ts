import type { Component } from "vue";
import { Bookmark, Copy, Headphones, RotateCcw, Search, SpellCheck } from "@lucide/vue";
import { api } from "@/api";
import { fewWords, type TextRange } from "@/components/reader/selection";
import { useNavigationStore } from "@/stores/navigation";
import { usePlayerStore } from "@/stores/player";
import { useReaderStore } from "@/stores/reader";
import { useUiStore } from "@/stores/ui";

/** What the reader's text menu is for: words selected on the page, or one sentence (a right
 *  click without selecting). */
export type TextTarget =
  { kind: "selection"; range: TextRange } | { kind: "sentence"; chapter: number; segmentId: string };

/** What an entry applies to: any selection, a selection of a few words (a name or a term, see
 *  `fewWords`), or a sentence. */
export type Applies = "selection" | "words" | "sentence";

/** An entry of the reader's text menu. Features add theirs to the list `useTextActions` gives;
 *  the menu shows the ones that apply to what it is for. */
export interface TextAction {
  id: string;
  label: string;
  icon?: Component;
  applies: readonly Applies[];
  /** A rule of the core's that decides it further (as `canSayAgain`). */
  available?: (target: TextTarget) => boolean;
  run: (target: TextTarget) => void | Promise<void>;
}

/** Whether `action` is offered for `target`. */
export function applies(action: TextAction, target: TextTarget): boolean {
  const shape =
    target.kind === "sentence"
      ? action.applies.includes("sentence")
      : action.applies.includes("selection") ||
        (action.applies.includes("words") && Boolean(fewWords(target.range.text)));
  return shape && (action.available?.(target) ?? true);
}

/** The sentence a target starts in. */
export const firstSentence = (target: TextTarget) =>
  target.kind === "sentence" ? target.segmentId : target.range.firstSegmentId;
const chapterOf = (target: TextTarget) => (target.kind === "sentence" ? target.chapter : target.range.chapter);
const textOf = (target: TextTarget) => (target.kind === "selection" ? target.range.text : "");

/** Copies text to the clipboard; WebKit without the clipboard API copies the page's selection. */
async function copy(text: string) {
  if (navigator.clipboard?.writeText) await navigator.clipboard.writeText(text);
  else document.execCommand("copy");
}

/** The reader's entries for selected text and sentences. */
export function useTextActions(): TextAction[] {
  const reader = useReaderStore(),
    player = usePlayerStore(),
    navigation = useNavigationStore(),
    ui = useUiStore();
  return [
    {
      id: "copy",
      label: "Copy",
      icon: Copy,
      applies: ["selection"],
      run: async (target) => {
        try {
          await copy(textOf(target));
          ui.toast("Copied", "info", "copy");
        } catch (e) {
          ui.toast(`Could not copy: ${e}`, "error");
        }
      },
    },
    {
      id: "search",
      label: "Search the book",
      icon: Search,
      applies: ["selection"],
      run: (target) => navigation.findInBook(textOf(target).replace(/\s+/g, " ").trim()),
    },
    {
      id: "listen",
      label: "Listen from here",
      icon: Headphones,
      applies: ["selection", "sentence"],
      run: async (target) => {
        if (reader.book) await player.play(reader.book.id, chapterOf(target), firstSentence(target));
      },
    },
    {
      id: "say-again",
      label: "Say this again",
      icon: RotateCcw,
      applies: ["sentence"],
      // The core decides (`canSayAgain`): the speech engine says it.
      available: (target) =>
        Boolean(
          reader.chapter?.blocks.flatMap((b) => b.segments).find((s) => s.id === firstSentence(target))?.canSayAgain,
        ),
      run: async (target) => {
        if (reader.book) await player.sayAgain(reader.book.id, firstSentence(target));
      },
    },
    {
      id: "bookmark",
      label: "Bookmark here",
      icon: Bookmark,
      applies: ["sentence"],
      run: async (target) => {
        if (!reader.book) return;
        try {
          await api.addBookmark({ bookId: reader.book.id, note: "", segmentId: firstSentence(target) });
          ui.toast("Bookmarked");
        } catch (e) {
          ui.toast(String(e), "error");
        }
      },
    },
    {
      id: "say-differently",
      label: "Say this differently…",
      icon: SpellCheck,
      applies: ["words"],
      run: (target) => {
        navigation.sayDifferently(fewWords(textOf(target)));
      },
    },
  ];
}

/** Logs which entry was chosen (never the text). */
export function logTextAction(id: string) {
  void api
    .logUi({
      entry: {
        level: "info",
        event: "ui.selection",
        msg: "An entry of the text menu was chosen",
        command: id,
        view: "reader",
      },
    })
    .catch(() => {});
}
