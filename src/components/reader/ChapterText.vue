<script setup lang="ts">
import { computed, ref } from "vue";
import type { BlockView, ChapterView, NoteMarker, SegmentView } from "../../api/types";

/** A chapter set as a page: headings and paragraphs, the spoken sentence washed and the
 *  current word underlined (nothing changes size, so lines never reflow). Clicking a sentence
 *  plays from it. Footnotes sit in the margin beside their paragraph (`marginNotes`) or open
 *  as popovers on their marker. */
const props = defineProps<{ chapter: ChapterView; currentSegment: string | null; currentWord: number | null; marginNotes: boolean }>();
const emit = defineEmits<{ play: [segmentId: string] }>();
const openNote = ref<string | null>(null);

const tag = (block: BlockView) => block.kind === "heading" ? ((block.level ?? 2) <= 2 ? "h2" : "h3") : block.kind === "preformatted" ? "pre" : block.kind === "listItem" ? "li" : "p";
const noteKey = (segment: SegmentView, note: NoteMarker) => `${segment.id}:${note.afterWord}:${note.label}`;
const noteId = (segment: SegmentView, note: NoteMarker) => `note-${noteKey(segment, note).replace(/[^\w-]/g, "-")}`;
const blockNotes = (block: BlockView) => block.segments.flatMap(segment => segment.notes.map(note => ({ segment, note })));
function marker(segment: SegmentView, note: NoteMarker) {
  if (props.marginNotes) { document.getElementById(noteId(segment, note))?.focus(); return; }
  const key = noteKey(segment, note);
  openNote.value = openNote.value === key ? null : key;
}
const play = (segment: SegmentView) => { if (segment.speakable) emit("play", segment.id); };
/** Sentences the book's recording has no audio for (playback skips them). */
const unnarrated = computed(() => new Set(props.chapter.notNarrated ?? []));
const chapterMissing = computed(() => unnarrated.value.size > 0 && props.chapter.blocks.every(b => b.segments.every(s => !s.speakable || unnarrated.value.has(s.id))));
</script>

<template>
  <article class="chapter-text" aria-label="Chapter text">
    <p v-if="chapterMissing" class="chapter-text__missing" role="note">This chapter is not in the recording, so playing skips it. Choose <strong>Speech engine</strong> under Narration in the book tools to hear it.</p>
    <div v-for="(block, index) in chapter.blocks" :key="`${chapter.index}:${index}`" class="chapter-text__row">
      <component :is="tag(block)" class="chapter-text__block" :class="`chapter-text__block--${block.kind}`">
        <span v-for="segment in block.segments" :key="segment.id" class="chapter-text__segment" :class="{ 'chapter-text__segment--quiet': !segment.speakable, 'chapter-text__segment--unnarrated': unnarrated.has(segment.id), 'chapter-text__segment--current': segment.id === currentSegment }" :data-segment-id="segment.id" :role="segment.speakable ? 'button' : undefined" :tabindex="segment.speakable ? 0 : undefined" @click="play(segment)" @keydown.enter.prevent="play(segment)" @keydown.space.prevent="play(segment)"><template v-for="(word, wordIndex) in segment.words" :key="wordIndex"><span class="chapter-text__word" :class="{ 'chapter-text__word--current': segment.id === currentSegment && currentWord === wordIndex }">{{ word }}</span><button v-for="note in segment.notes.filter(n => n.afterWord === wordIndex)" :key="note.label + wordIndex" type="button" class="chapter-text__note" :aria-label="`Note ${note.label}`" :aria-describedby="marginNotes ? noteId(segment, note) : undefined" :aria-expanded="marginNotes ? undefined : openNote === noteKey(segment, note)" @click.stop="marker(segment, note)" @keydown.enter.stop @keydown.space.stop>{{ note.label }}<span v-if="!marginNotes && openNote === noteKey(segment, note)" class="chapter-text__popover" role="note">{{ note.text ?? 'The text of this note was not found in the book.' }}</span></button>{{ wordIndex < segment.words.length - 1 ? ' ' : '' }}</template>{{ ' ' }}</span>
      </component>
      <aside v-if="marginNotes && blockNotes(block).length" class="chapter-text__margin">
        <p v-for="{ segment, note } in blockNotes(block)" :id="noteId(segment, note)" :key="noteKey(segment, note)" class="chapter-text__margin-note" tabindex="-1"><small class="chapter-text__note-label">Note {{ note.label }}</small>{{ note.text ?? 'The text of this note was not found in the book.' }}</p>
      </aside>
    </div>
  </article>
</template>

<style scoped>
.chapter-text { display: grid; row-gap: 18px; font: 400 calc(20.5px * var(--read-scale)) / 1.68 var(--font-read); color: var(--reading); }
.chapter-text__row { min-width: 0; }
.chapter-text__block { margin: 0; }
.chapter-text__block--heading { font: 600 calc(34px * var(--read-scale)) / 1.15 var(--font-read); color: var(--ink); letter-spacing: -.012em; padding-bottom: 6px; }
h3.chapter-text__block--heading { font-size: calc(26px * var(--read-scale)); }
.chapter-text__block--preformatted { font: calc(15px * var(--read-scale)) / 1.7 ui-monospace, monospace; white-space: pre-wrap; }
.chapter-text__block--listItem { margin-left: 1.2em; list-style: disc; display: list-item; }
.chapter-text__segment { cursor: pointer; border-radius: 3px; -webkit-box-decoration-break: clone; box-decoration-break: clone; transition: background-color .12s, box-shadow .12s; }
.chapter-text__segment:hover { background: color-mix(in srgb, var(--wash) 55%, transparent); }
.chapter-text__segment:focus-visible { outline-offset: 2px; }
.chapter-text__segment--quiet { color: var(--dim); cursor: default; }
.chapter-text__segment--quiet:hover { background: none; }
.chapter-text__segment--unnarrated { color: var(--dim); text-decoration: underline dotted var(--faint); text-underline-offset: 4px; }
.chapter-text__missing { margin: 0 0 18px; padding: 10px 14px; border-radius: 12px; background: var(--wash); color: var(--dim); font-family: var(--font-ui, inherit); font-size: 13px; line-height: 1.5; }
.chapter-text__segment--current, .chapter-text__segment--current:hover { background: var(--wash); box-shadow: 0 0 0 3px var(--wash); color: var(--ink); }
.chapter-text__word--current { text-decoration: underline 2.5px var(--accent); text-underline-offset: 5px; }
.chapter-text__note { position: relative; vertical-align: super; font: 700 11px/1 var(--font-ui); color: var(--accent); padding: 0 2px; }
.chapter-text__popover { position: absolute; left: 50%; top: 1.8em; z-index: 4; transform: translateX(-50%); width: min(320px, 70vw); padding: 12px 14px; border-radius: 12px; background: var(--page); color: var(--dim); box-shadow: 0 14px 34px -12px var(--shadow), 0 0 0 1px var(--line); font: 400 14.5px/1.5 var(--font-read); text-align: left; white-space: normal; cursor: auto; }
.chapter-text__margin { display: grid; gap: 14px; align-content: start; }
.chapter-text__margin-note { padding-top: 6px; font: 400 14.5px/1.5 var(--font-read); color: var(--dim); }
.chapter-text__margin-note:focus { outline: none; }
.chapter-text__margin-note:focus-visible, .chapter-text__margin-note:focus { box-shadow: -10px 0 0 -7px var(--accent); }
.chapter-text__note-label { display: block; margin-bottom: 3px; font: 700 11px/1.4 var(--font-ui); letter-spacing: .06em; text-transform: uppercase; color: var(--accent); }
</style>
