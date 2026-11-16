<script setup lang="ts">
import { computed } from "vue";
import type { NoteMarker, SegmentView } from "@/api/types";
import { noteId, noteKey } from "@/components/reader/notes";

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
}>();
const emit = defineEmits<{ play: []; marker: [note: NoteMarker] }>();
/** The sentence's words in runs, each ending where footnote markers follow a word (the last
 *  ends with the sentence), with those markers. */
const runs = computed(() => {
  const { words, notes } = props.segment;
  const runs: { first: number; words: string[]; notes: NoteMarker[] }[] = [];
  let first = 0;
  words.forEach((_, index) => {
    const after = notes.filter((note) => note.afterWord === index);
    if (!after.length && index < words.length - 1) return;
    runs.push({ first, words: words.slice(first, index + 1), notes: after });
    first = index + 1;
  });
  return runs;
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
    @click="clickable && emit('play')"
    @keydown.enter.prevent="clickable && emit('play')"
    @keydown.space.prevent="clickable && emit('play')"
    ><template v-for="(run, r) in runs" :key="run.first"
      >{{ r > 0 ? " " : ""
      }}<template v-if="current"
        ><template v-for="(text, i) in run.words" :key="i"
          ><span class="chapter-segment__word" :class="{ 'chapter-segment__word--current': word === run.first + i }">{{
            text
          }}</span
          >{{ i < run.words.length - 1 ? " " : "" }}</template
        ></template
      ><template v-else>{{ run.words.join(" ") }}</template
      ><button
        v-for="(note, n) in run.notes"
        :key="n"
        type="button"
        class="chapter-segment__note"
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
    >{{ " " }}</span
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
  font: 400 14.5px/1.5 var(--font-read);
  text-align: left;
  white-space: normal;
  cursor: auto;
}
</style>
