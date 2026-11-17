<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import type { BlockView, ChapterView, HighlightView, InsertView, NoteMarker, SegmentView } from "@/api/types";
import ChapterSegment from "@/components/reader/ChapterSegment.vue";
import { noteId, noteKey } from "@/components/reader/notes";
import { sentenceMarks, NO_MARKS, type Mark } from "@/components/reader/highlights";

/** A chapter set as a page: headings and paragraphs, the spoken sentence washed and the
 *  current word underlined (nothing changes size, so lines never reflow). When `playable`,
 *  clicking a sentence plays from it. Footnotes sit in the margin beside their paragraph
 *  (`marginNotes`) or open as popovers on their marker. Slots `before` and `after` put
 *  something in the text column above and below it (an article's header and notice). */
const props = withDefaults(
  defineProps<{
    chapter: ChapterView;
    currentSegment: string | null;
    currentWord: number | null;
    marginNotes: boolean;
    playable?: boolean;
    /** The book's highlights; each sentence is handed the parts that fall in it. */
    highlights?: HighlightView[];
  }>(),
  { playable: true, highlights: () => [] },
);
const emit = defineEmits<{ play: [segmentId: string] }>();
const openNote = ref<string | null>(null);

const tag = (block: BlockView) =>
  block.kind === "heading"
    ? (block.level ?? 2) <= 2
      ? "h2"
      : "h3"
    : block.kind === "preformatted"
      ? "pre"
      : block.kind === "listItem"
        ? "li"
        : "p";
const blockNotes = (block: BlockView) =>
  block.segments.flatMap((segment) => segment.notes.map((note) => ({ segment, note })));
function marker(segment: SegmentView, note: NoteMarker) {
  if (props.marginNotes) {
    document.getElementById(noteId(segment, note))?.focus();
    return;
  }
  const key = noteKey(segment, note);
  openNote.value = openNote.value === key ? null : key;
}
/** Each sentence's highlighted parts; a sentence whose parts did not change keeps its array,
 *  so a new highlight draws again only the sentences it covers. */
let lastMarks: Map<string, readonly Mark[]> | undefined;
const marks = computed(() => (lastMarks = sentenceMarks(props.chapter, props.highlights, lastMarks)));
const clickable = (segment: SegmentView) => props.playable && segment.speakable;
/** The pictures before block `index` (after the last block: `blocks.length`). */
const picturesBefore = computed(() => {
  const by = new Map<number, InsertView[]>();
  for (const insert of props.chapter.inserts) {
    const at = Math.min(insert.beforeBlock, props.chapter.blocks.length);
    by.set(at, [...(by.get(at) ?? []), insert]);
  }
  return by;
});
/** Blocks that are a picture's caption: still the text, set as a caption. */
const captions = computed(
  () => new Set(props.chapter.inserts.flatMap((i) => (i.captionBlock === null ? [] : [i.captionBlock]))),
);
const pictureLabel = (insert: InsertView) => insert.alt || insert.caption || "a picture";
/** The picture opened large, over the page. */
const enlarged = ref<InsertView | null>(null),
  overlay = ref<HTMLElement>();
let opener: HTMLElement | null = null;
async function enlarge(insert: InsertView, event: Event) {
  opener = event.currentTarget as HTMLElement;
  enlarged.value = insert;
  await nextTick();
  overlay.value?.focus();
}
function shrink() {
  enlarged.value = null;
  opener?.focus();
}
/** Sentences the book's recording has no audio for (playback skips them). */
const unnarrated = computed(() => new Set(props.chapter.notNarrated ?? []));
const chapterMissing = computed(
  () =>
    unnarrated.value.size > 0 &&
    props.chapter.blocks.every((b) => b.segments.every((s) => !s.speakable || unnarrated.value.has(s.id))),
);
</script>

<template>
  <article class="chapter-text" aria-label="Chapter text">
    <p v-if="chapterMissing" class="chapter-text__missing" role="note">
      This chapter is not in the recording, so playing skips it. To hear it, turn on
      <strong>Speak what the recording lacks</strong> under Narration at the top of the reader, or choose
      <strong>Speech engine</strong> there.
    </p>
    <slot name="before" />
    <template v-for="(block, index) in chapter.blocks" :key="`${chapter.index}:${index}`">
      <figure
        v-for="(insert, p) in picturesBefore.get(index) ?? []"
        :key="`p${index}:${p}`"
        class="chapter-text__picture"
        data-not-text
      >
        <button
          type="button"
          class="chapter-text__enlarge"
          :aria-label="`Enlarge picture: ${pictureLabel(insert)}`"
          @click="enlarge(insert, $event)"
        >
          <img :src="insert.url" :alt="insert.alt" loading="lazy" class="chapter-text__image" />
        </button>
        <figcaption v-if="insert.caption" class="chapter-text__caption">{{ insert.caption }}</figcaption>
      </figure>
      <div class="chapter-text__row">
        <component
          :is="tag(block)"
          class="chapter-text__block"
          :class="[`chapter-text__block--${block.kind}`, { 'chapter-text__block--caption': captions.has(index) }]"
        >
          <ChapterSegment
            v-for="segment in block.segments"
            :key="segment.id"
            :segment="segment"
            :current="segment.id === currentSegment"
            :word="segment.id === currentSegment ? currentWord : null"
            :unnarrated="unnarrated.has(segment.id)"
            :clickable="clickable(segment)"
            :margin-notes="marginNotes"
            :open-note="openNote?.startsWith(`${segment.id}:`) ? openNote : null"
            :marks="marks.get(segment.id) ?? NO_MARKS"
            @play="emit('play', segment.id)"
            @marker="marker(segment, $event)"
          />
        </component>
        <aside v-if="marginNotes && blockNotes(block).length" class="chapter-text__margin" data-not-text>
          <p
            v-for="{ segment, note } in blockNotes(block)"
            :id="noteId(segment, note)"
            :key="noteKey(segment, note)"
            class="chapter-text__margin-note"
            tabindex="-1"
          >
            <small class="chapter-text__note-label">Note {{ note.label }}</small
            >{{ note.text ?? "The text of this note was not found in the book." }}
          </p>
        </aside>
      </div>
    </template>
    <figure
      v-for="(insert, p) in picturesBefore.get(chapter.blocks.length) ?? []"
      :key="`end:${p}`"
      class="chapter-text__picture"
      data-not-text
    >
      <button
        type="button"
        class="chapter-text__enlarge"
        :aria-label="`Enlarge picture: ${pictureLabel(insert)}`"
        @click="enlarge(insert, $event)"
      >
        <img :src="insert.url" :alt="insert.alt" loading="lazy" class="chapter-text__image" />
      </button>
      <figcaption v-if="insert.caption" class="chapter-text__caption">{{ insert.caption }}</figcaption>
    </figure>
    <slot name="after" />
    <Teleport to="body">
      <div
        v-if="enlarged"
        ref="overlay"
        class="chapter-text__overlay"
        role="dialog"
        aria-modal="true"
        aria-label="Picture"
        tabindex="-1"
        @click="shrink"
        @keydown.esc="shrink"
      >
        <img :src="enlarged.url" :alt="enlarged.alt" class="chapter-text__large" />
      </div>
    </Teleport>
  </article>
</template>

<style scoped>
.chapter-text {
  display: grid;
  row-gap: var(--space-9);
  font: 400 calc(20.5px * var(--read-scale)) / var(--read-line) var(--read-font);
  color: var(--reading);
}
.chapter-text__row {
  min-width: 0;
}
/* Justified and hyphenated as chosen: the book's text, not its headings or code, nor what the
   page puts around it (an article's header, notices). */
.chapter-text__block {
  text-align: var(--read-align);
  hyphens: var(--read-hyphens);
  -webkit-hyphens: var(--read-hyphens);
  margin: 0;
}
.chapter-text__block--heading,
.chapter-text__block--preformatted {
  text-align: start;
  hyphens: manual;
  -webkit-hyphens: manual;
}
.chapter-text__block--heading {
  font: 600 calc(34px * var(--read-scale)) / 1.15 var(--read-font);
  color: var(--ink);
  letter-spacing: -0.012em;
  padding-bottom: var(--space-3);
}
h3.chapter-text__block--heading {
  font-size: calc(26px * var(--read-scale));
}
.chapter-text__block--preformatted {
  font:
    calc(15px * var(--read-scale)) / 1.7 ui-monospace,
    monospace;
  white-space: pre-wrap;
}
.chapter-text__block--listItem {
  margin-left: 1.2em;
  list-style: disc;
  display: list-item;
}
/* A picture: at most the column's width, centred, its caption paragraph just under it. */
.chapter-text__picture {
  display: grid;
  justify-items: center;
  gap: var(--space-3);
  margin: 0;
}
.chapter-text__enlarge {
  display: block;
  max-width: 100%;
  padding: 0;
  border-radius: var(--radius-md);
  cursor: zoom-in;
}
.chapter-text__image {
  display: block;
  max-width: 100%;
  max-height: 70vh;
  height: auto;
  border-radius: var(--radius-md);
}
.chapter-text__caption,
.chapter-text__block--caption {
  color: var(--dim);
  font-size: 0.82em;
  font-style: italic;
  text-align: center;
}
.chapter-text__block--caption {
  margin-top: calc(-1 * var(--space-6));
}
.chapter-text__overlay {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: grid;
  place-items: center;
  padding: var(--space-8);
  background: color-mix(in srgb, var(--ground) 92%, transparent);
  cursor: zoom-out;
}
.chapter-text__overlay:focus {
  outline: none;
}
.chapter-text__large {
  width: 100%;
  height: 100%;
  object-fit: contain;
}
.chapter-text__missing {
  margin: 0 0 var(--space-9);
  padding: var(--space-5) var(--space-7);
  border-radius: var(--radius-md);
  background: var(--wash);
  color: var(--dim);
  font-family: var(--font-ui, inherit);
  font-size: 13px;
  line-height: 1.5;
}
.chapter-text__margin {
  display: grid;
  gap: var(--space-7);
  align-content: start;
}
.chapter-text__margin-note {
  padding-top: var(--space-3);
  font: 400 14.5px/1.5 var(--read-font);
  color: var(--dim);
  /* A narrow column: ragged, hyphenated when the text is. */
  hyphens: var(--read-hyphens);
  -webkit-hyphens: var(--read-hyphens);
}
.chapter-text__margin-note:focus {
  outline: none;
}
.chapter-text__margin-note:focus-visible,
.chapter-text__margin-note:focus {
  box-shadow: -10px 0 0 -7px var(--accent);
}
.chapter-text__note-label {
  display: block;
  margin-bottom: var(--space-1);
  font: 700 11px/1.4 var(--font-ui);
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--accent);
}
</style>
