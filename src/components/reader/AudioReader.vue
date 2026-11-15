<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Pause, Play } from "@lucide/vue";
import CoverArt from "../CoverArt.vue";
import PlayerScrubber from "../player/PlayerScrubber.vue";
import SpeedPill from "../player/SpeedPill.vue";
import SleepPill from "../player/SleepPill.vue";
import { formatClock, formatDuration, partsOf } from "../../estimate";
import { useLibraryStore } from "../../stores/library";
import { usePlayerStore } from "../../stores/player";
import { useReaderStore } from "../../stores/reader";

/** The reader for an audiobook on its own (shape `listen`): cover, a scrubber over the part
 *  playing, the transport with ±30 s, the parts to jump to, and what adding its text would
 *  give. Before it plays, it shows and moves the saved place. */
const reader = useReaderStore(),
  player = usePlayerStore(),
  library = useLibraryStore();
const SKIP_MS = 30_000;

const book = computed(() => reader.book!);
const summary = computed(() => library.books.find((b) => b.id === book.value.id));
const parts = computed(() => partsOf(book.value.chapters));
const total = computed(() => parts.value.at(-1)?.end ?? 0);
/** The player is on this audiobook. */
const here = computed(() => player.state.bookId === book.value.id && player.state.positionMs !== null);
const active = computed(() => here.value && (player.state.status === "playing" || player.state.status === "buffering"));
/** Where a seek put the saved place while it is not playing (the core keeps it). */
const placed = ref<number | null>(null);
watch(
  () => [book.value.id, here.value],
  () => (placed.value = null),
);
const position = computed(() => {
  if (here.value) return player.state.positionMs ?? 0;
  return placed.value ?? summary.value?.progress?.positionMs ?? book.value.progress?.positionMs ?? 0;
});
const partIndex = computed(() =>
  here.value ? player.state.chapter : Math.max(0, parts.value.filter((p) => p.start <= position.value).length - 1),
);
const part = computed(() => parts.value[partIndex.value] ?? { index: 0, title: "", start: 0, end: total.value });
const partLength = computed(() => part.value.end - part.value.start);
const into = computed(() => Math.max(0, position.value - part.value.start));
const played = computed(() => (partLength.value ? Math.min(1, into.value / partLength.value) : 0));
const speed = computed(() => player.state.speed);
const overall = computed(() => {
  const percent = total.value ? Math.round((position.value / total.value) * 100) : 0;
  const left = formatDuration((total.value - position.value) / Math.max(0.1, speed.value));
  return `${percent}% of the book · ${left} left${speed.value === 1 ? "" : ` at ${speed.value}×`}`;
});

function moveTo(at: number) {
  const clamped = Math.min(total.value, Math.max(0, Math.round(at)));
  if (!here.value) placed.value = clamped;
  void player.seek(book.value.id, clamped);
}
const seek = (fraction: number) => moveTo(part.value.start + fraction * partLength.value);
const skip = (delta: number) => moveTo(position.value + delta);
async function toggle() {
  if (active.value) await player.pause();
  else if (here.value && player.state.status === "paused") await player.resume();
  else await player.play(book.value.id, null, null);
}
const playPart = (index: number) => player.play(book.value.id, index, null);
</script>

<template>
  <div class="audio-reader">
    <section class="audio-reader__deck" aria-label="Player">
      <CoverArt
        class="audio-reader__cover"
        shape="square"
        :title="book.title"
        :url="summary?.coverUrl"
        :label="false"
      />
      <div class="audio-reader__heading">
        <h1 class="audio-reader__title">{{ book.title }}</h1>
        <p class="audio-reader__authors">{{ book.authors.join(", ") || "Unknown author" }}</p>
      </div>
      <div class="audio-reader__scrub">
        <PlayerScrubber
          label="Position in part"
          :played="played"
          :start="formatClock(into)"
          :end="`−${formatClock(partLength - into, true)}`"
          @seek="seek"
          @step="skip($event * 10_000)"
        />
        <p class="audio-reader__part">Part {{ part.index + 1 }} · {{ part.title }}</p>
      </div>
      <div class="audio-reader__transport">
        <SpeedPill class="audio-reader__pill" />
        <button type="button" class="audio-reader__skip" aria-label="Back 30 seconds" @click="skip(-SKIP_MS)">
          −30
        </button>
        <button
          type="button"
          class="audio-reader__play"
          :aria-label="active ? 'Pause' : 'Play'"
          :title="active ? 'Pause' : 'Play'"
          @click="toggle"
        >
          <Pause v-if="active" :size="26" fill="currentColor" aria-hidden="true" />
          <Play v-else :size="26" fill="currentColor" aria-hidden="true" />
        </button>
        <button type="button" class="audio-reader__skip" aria-label="Forward 30 seconds" @click="skip(SKIP_MS)">
          +30
        </button>
        <SleepPill class="audio-reader__pill" />
      </div>
      <p class="audio-reader__overall">{{ overall }}</p>
      <p v-if="here && player.state.error" class="audio-reader__error" role="alert">{{ player.state.error }}</p>
    </section>

    <div class="audio-reader__side">
      <section class="audio-reader__card" aria-labelledby="audio-reader-parts">
        <h2 id="audio-reader-parts" class="audio-reader__label">Parts</h2>
        <p v-for="warning in book.warnings" :key="warning" class="audio-reader__warning" role="alert">
          {{ warning }}
        </p>
        <ol class="audio-reader__parts">
          <li v-for="p in parts" :key="p.index">
            <button
              type="button"
              class="audio-reader__part-row"
              :class="{ 'audio-reader__part-row--current': p.index === part.index }"
              :aria-current="p.index === part.index || undefined"
              @click="playPart(p.index)"
            >
              <span class="audio-reader__part-number">{{ p.index + 1 }}</span>
              <span class="audio-reader__part-title">{{ p.title }}</span>
              <span class="audio-reader__part-length">{{ formatClock(p.end - p.start) }}</span>
            </button>
          </li>
        </ol>
      </section>
      <section class="audio-reader__card" aria-labelledby="audio-reader-text">
        <h2 id="audio-reader-text" class="audio-reader__label">Text</h2>
        <p class="audio-reader__lead">Read along with the book</p>
        <p class="audio-reader__copy">
          Add the EPUB of this book and the words are highlighted as they are spoken. You can also read it silently and
          pick the recording up where you stopped.
        </p>
      </section>
    </div>
  </div>
</template>

<style scoped>
.audio-reader {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-wrap: wrap;
  gap: 40px;
  padding: 24px 20px 32px;
  align-items: flex-start;
  justify-content: center;
}
.audio-reader__deck {
  flex: 1 1 380px;
  max-width: 520px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 18px;
  text-align: center;
}
.audio-reader__cover {
  width: 300px;
  max-width: 100%;
  border-radius: 14px;
  box-shadow: 0 24px 48px -12px var(--shadow);
}
.audio-reader__title {
  margin: 0;
  font-size: 28px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.audio-reader__authors {
  margin: 4px 0 0;
  color: var(--dim);
  font-size: 15px;
}
.audio-reader__scrub {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.audio-reader__part {
  margin: 0;
  font-size: 13px;
  color: var(--dim);
}
.audio-reader__transport {
  display: flex;
  align-items: center;
  gap: 16px;
}
.audio-reader__pill {
  height: 44px;
  min-width: 52px;
  padding: 0 12px;
  border-radius: 22px;
  justify-content: center;
}
.audio-reader__skip {
  width: 48px;
  height: 48px;
  border-radius: 50%;
  background: var(--page);
  font-weight: 700;
  font-size: 13px;
  font-variant-numeric: tabular-nums;
}
.audio-reader__play {
  display: grid;
  place-items: center;
  width: 72px;
  height: 72px;
  border-radius: 50%;
  background: var(--accent);
  color: var(--on-accent);
  box-shadow: 0 10px 22px -8px var(--accent);
}
.audio-reader__overall {
  margin: 0;
  font-size: 13px;
  color: var(--dim);
}
.audio-reader__error,
.audio-reader__warning {
  margin: 0;
  font-size: 13px;
  color: var(--warn);
}
.audio-reader__side {
  flex: 1 1 340px;
  max-width: 460px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.audio-reader__card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 18px;
  border-radius: 20px;
  background: var(--page);
}
.audio-reader__label {
  margin: 0;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--dim);
}
.audio-reader__parts {
  margin: 0;
  padding: 0;
  list-style: none;
}
.audio-reader__parts li + li {
  border-top: 1px solid var(--line);
}
.audio-reader__part-row {
  width: 100%;
  min-height: 44px;
  display: flex;
  align-items: center;
  gap: 14px;
  background: none;
  color: var(--dim);
  text-align: left;
}
.audio-reader__part-row--current {
  color: var(--ink);
  font-weight: 700;
}
.audio-reader__part-number {
  width: 18px;
  text-align: right;
  font-variant-numeric: tabular-nums;
}
.audio-reader__part-row--current .audio-reader__part-number {
  color: var(--accent);
}
.audio-reader__part-title {
  flex: 1;
  min-width: 0;
}
.audio-reader__part-length {
  font-size: 13px;
  font-variant-numeric: tabular-nums;
}
.audio-reader__lead {
  margin: 0;
  font-size: 16px;
  font-weight: 700;
}
.audio-reader__copy {
  margin: 0;
  font-size: 13px;
  line-height: 1.45;
  color: var(--dim);
}
@media (max-width: 760px) {
  .audio-reader {
    padding: 12px 6px 24px;
    gap: 24px;
  }
}
</style>
