<script setup lang="ts">
import { computed } from "vue";
import type { BookSummary } from "../../api/types";
import CoverArt from "../CoverArt.vue";
import { timeLeft } from "../../estimate";
import { useLibraryStore } from "../../stores/library";
import { usePlayerStore } from "../../stores/player";
import { useReaderStore } from "../../stores/reader";
import { useNavigationStore } from "../../stores/navigation";

/** One item to continue, as a card that opens it in the reader. The first is the hero. */
const props = defineProps<{ book: BookSummary; hero?: boolean }>();
const player = usePlayerStore(),
  library = useLibraryStore(),
  reader = useReaderStore(),
  navigation = useNavigationStore();
const kind = computed(() => (props.book.audio === "recording" ? "Book · read-along" : "Book"));
const chapter = computed(() =>
  player.state.bookId === props.book.id ? player.state.chapter : (props.book.progress?.chapter ?? 0),
);
const fraction = computed(() => props.book.progress?.fraction ?? 0);
const details = computed(() => {
  const parts = [`Chapter ${chapter.value + 1}`, `${Math.round(fraction.value * 100)}%`];
  const left = timeLeft(props.book, library.readingWpm, player.state.speed);
  if (left) parts.push(left);
  return parts.join(" · ");
});
async function open() {
  if (reader.book?.id === props.book.id) navigation.view = "reader";
  else await reader.open(props.book.id);
}
</script>

<template>
  <button
    type="button"
    class="continue-card"
    :class="{ 'continue-card--hero': hero }"
    :aria-label="`Continue ${book.title}`"
    @click="open"
  >
    <CoverArt class="continue-card__cover" :title="book.title" :url="book.coverUrl" :label="false" />
    <span class="continue-card__body">
      <span class="continue-card__kind">{{ kind }}</span>
      <b class="continue-card__title">{{ book.title }}</b>
      <span class="continue-card__authors">{{ book.authors.join(", ") || "Unknown author" }}</span>
      <span class="continue-card__progress">
        <span class="continue-card__track"><i :style="{ width: `${fraction * 100}%` }" /></span>
        <span class="continue-card__details">{{ details }}</span>
      </span>
    </span>
  </button>
</template>

<style scoped>
.continue-card {
  flex: 1 1 280px;
  max-width: 560px;
  min-height: 196px;
  display: flex;
  gap: 16px;
  padding: 18px;
  border-radius: 22px;
  background: var(--page);
  text-align: left;
  transition: transform var(--dur-1) var(--ease);
}
.continue-card:hover {
  transform: translateY(-2px);
}
.continue-card--hero {
  background: var(--hero);
}
.continue-card .continue-card__cover {
  flex: none;
  width: 106px;
  padding: 10px;
  align-self: flex-start;
  border-radius: 8px;
  box-shadow: 0 10px 22px -6px var(--shadow);
}
.continue-card__body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.continue-card__kind {
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--dim);
}
.continue-card__title {
  font-size: 19px;
  line-height: 1.2;
}
.continue-card__authors {
  color: var(--dim);
}
.continue-card__progress {
  margin-top: auto;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.continue-card__track {
  height: 4px;
  border-radius: 2px;
  background: var(--wash);
}
.continue-card--hero .continue-card__track {
  background: var(--line);
}
.continue-card__track i {
  display: block;
  height: 100%;
  border-radius: 2px;
  background: var(--accent);
}
.continue-card__details {
  font-size: 13px;
  color: var(--dim);
  font-variant-numeric: tabular-nums;
}
</style>
