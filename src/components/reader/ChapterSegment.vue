<script setup lang="ts">
import { computed } from "vue";
import type { NoteMarker, SegmentView } from "@/api/types";
import { noteId, noteKey } from "@/components/reader/notes";
import { clickToPlay } from "@/components/reader/selection";
import { pieces, type Mark } from "@/components/reader/highlights";

/** One sentence of a chapter's text: its words (the one being spoken underlined) and the
 *  markers of its footnotes. A sentence is drawn again only when its own props change, so a
 *  new word redraws the sentence being spoken, not the chapter. Only the sentence being spoken
 *  is drawn word by word; the others are runs of plain text, so a long chapter stays a small page
 *  to lay out and paint. */
const props = defineProps<{
  segment: SegmentView;
  /** Being spoken: washed. */
  current: boolean;
  /** The word being spoken, when `current`. */
  word: number | null;
  /** The book's recording has no audio for it (playback skips it). */
  unnarrated: boolean;
  /** Clicking it plays from it. */
  clickable: boolean;
  /** Footnotes sit in the margin (markers focus them) rather than in popovers. */
  marginNotes: boolean;
  /** The key of its footnote whose popover is open. */
  openNote: string | null;
  /** Its highlighted parts (see `highlights.ts`). */
  marks: readonly Mark[];
  /** Code: the whitespace after each word as written. Drawn after the single space that is
   *  counted as the text's, as what is not the text (`data-not-text`), so line breaks and
   *  indentation show and selections still count single spaces. */
  gaps?: readonly string[] | null;
}>();
/** What of word `i`'s gap is drawn beyond its counted single space. */
const extra = (i: number) => {
  const gap = props.gaps?.[i] ?? " ";
  return gap.startsWith(" ") ? gap.slice(1) : gap;
};
const emit = defineEmits<{ play: []; marker: [note: NoteMarker] }>();
/** The sentence's words in runs, each ending where footnote markers follow a word (the last
 *  ends with the sentence), with those markers. */
const runs = computed(() => {
  const { words, notes } = props.segment;
  const runs: { first: number; words: string[]; notes: NoteMarker[]; at: number }[] = [];
  let first = 0,
    at = 0;
  words.forEach((_, index) => {
    const after = notes.filter((note) => note.afterWord === index);
    if (!after.length && index < words.length - 1) return;
    const run = words.slice(first, index + 1);
    runs.push({ first, words: run, notes: after, at });
    at += run.join(" ").length + 1;
    first = index + 1;
  });
  return runs;
});
/** Where each word starts in the sentence as drawn (words joined by single spaces). */
const wordAt = computed(() => {
  let at = 0;
  return props.segment.words.map((word) => {
    const start = at;
    at += word.length + 1;
    return start;
  });
});
</script>

<template>
  <span
    class="chapter-segment"
    :class="{
      'chapter-segment--quiet': !segment.speakable,
      'chapter-segment--unnarrated': unnarrated,
      'chapter-segment--current': current,
      'chapter-segment--clickable': clickable,
    }"
    :data-segment-id="segment.id"
    :role="clickable ? 'button' : undefined"
    :tabindex="clickable ? 0 : undefined"
    @click="clickable && clickToPlay($event) && emit('play')"
    @keydown.enter.prevent="clickable && emit('play')"
    @keydown.space.prevent="clickable && emit('play')"
    ><template v-for="(run, r) in runs" :key="run.first"
      ><template v-if="current || gaps"
        >{{ r > 0 ? " " : ""
        }}<span v-if="gaps && r > 0 && extra(run.first - 1)" class="chapter-segment__gap" data-not-text>{{
          extra(run.first - 1)
        }}</span
        ><template v-for="(text, i) in run.words" :key="i"
          ><span class="chapter-segment__word" :class="{ 'chapter-segment__word--current': word === run.first + i }"
            ><template v-for="(piece, p) in pieces(text, wordAt[run.first + i], marks)" :key="p"
              ><mark v-if="piece.id !== null" class="chapter-segment__mark" :data-highlight-id="piece.id">{{
                piece.text
              }}</mark
              ><template v-else>{{ piece.text }}</template></template
            ></span
          ><template v-if="i < run.words.length - 1"
            ><template v-for="(piece, p) in pieces(' ', wordAt[run.first + i] + text.length, marks)" :key="p"
              ><mark v-if="piece.id !== null" class="chapter-segment__mark" :data-highlight-id="piece.id">{{
                piece.text
              }}</mark
              ><template v-else>{{ piece.text }}</template></template
            ><span v-if="gaps && extra(run.first + i)" class="chapter-segment__gap" data-not-text>{{
              extra(run.first + i)
            }}</span></template
          ></template
        ></template
      ><template v-else
        ><template
          v-for="(piece, p) in pieces((r > 0 ? ' ' : '') + run.words.join(' '), run.at - (r > 0 ? 1 : 0), marks)"
          :key="p"
          ><mark v-if="piece.id !== null" class="chapter-segment__mark" :data-highlight-id="piece.id">{{
            piece.text
          }}</mark
          ><template v-else>{{ piece.text }}</template></template
        ></template
      ><button
        v-for="(note, n) in run.notes"
        :key="n"
        type="button"
        class="chapter-segment__note"
        data-not-text
        :aria-label="`Note ${note.label}`"
        :aria-describedby="marginNotes ? noteId(segment, note) : undefined"
        :aria-expanded="marginNotes ? undefined : openNote === noteKey(segment, note)"
        @click.stop="emit('marker', note)"
        @keydown.enter.stop
        @keydown.space.stop
      >
        {{ note.label
        }}<span
          v-if="!marginNotes && openNote === noteKey(segment, note)"
          class="chapter-segment__popover"
          role="note"
          >{{ note.text ?? "The text of this note was not found in the book." }}</span
        >
      </button></template
    >{{ " "
    }}<span v-if="gaps && extra(segment.words.length - 1)" class="chapter-segment__gap" data-not-text>{{
      extra(segment.words.length - 1)
    }}</span></span
  >
</template>

<style scoped>
.chapter-segment {
  border-radius: var(--radius-xs);
  -webkit-box-decoration-break: clone;
  box-decoration-break: clone;
  transition:
    background-color 0.12s,
    box-shadow 0.12s;
}
.chapter-segment--clickable {
  cursor: pointer;
}
.chapter-segment--clickable:hover {
  background: color-mix(in srgb, var(--wash) 55%, transparent);
}
.chapter-segment:focus-visible {
  outline-offset: 2px;
}
.chapter-segment--quiet {
  color: var(--dim);
}
.chapter-segment--unnarrated {
  color: var(--dim);
  text-decoration: underline dotted var(--faint);
  text-underline-offset: 4px;
}
.chapter-segment--current,
.chapter-segment--current:hover {
  background: var(--wash);
  box-shadow: 0 0 0 3px var(--wash);
  color: var(--ink);
}
.chapter-segment__mark {
  background: var(--mark);
  color: inherit;
  border-radius: var(--radius-xs);
  -webkit-box-decoration-break: clone;
  box-decoration-break: clone;
}
/* The words a search of the library found. */
.chapter-segment__mark[data-highlight-id="-1"] {
  background: none;
  box-shadow: inset 0 -0.18em 0 var(--accent);
}
/* Drawn word by word while spoken: square ends, so the pieces meet. */
.chapter-segment--current .chapter-segment__mark {
  border-radius: 0;
}
.chapter-segment__word--current {
  text-decoration: underline 2.5px var(--accent);
  text-underline-offset: 5px;
}
.chapter-segment__note {
  position: relative;
  vertical-align: super;
  font: 700 11px/1 var(--font-ui);
  color: var(--accent);
  padding: 0 var(--space-1);
}
.chapter-segment__popover {
  position: absolute;
  left: 50%;
  top: 1.8em;
  z-index: 4;
  transform: translateX(-50%);
  width: min(320px, 70vw);
  padding: var(--space-6) var(--space-7);
  border-radius: var(--radius-md);
  background: var(--page);
  color: var(--dim);
  box-shadow:
    0 14px 34px -12px var(--shadow),
    0 0 0 1px var(--line);
  font: 400 14.5px/1.5 var(--read-font);
  text-align: left;
  white-space: normal;
  cursor: auto;
}
</style>
