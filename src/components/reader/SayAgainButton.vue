<script setup lang="ts">
import UiButton from "@/components/ui/UiButton.vue";
import { computed, ref } from "vue";
import { LoaderCircle, RotateCcw } from "@lucide/vue";
import { usePlayerStore } from "@/stores/player";
import { useReaderStore } from "@/stores/reader";

/** "Say this again" for the sentence being spoken in the open book, when the speech engine says
 *  it (the core decides: `canSayAgain`). Busy while the sentence is asked for and made. */
const player = usePlayerStore(),
  reader = useReaderStore();
const sentence = computed(() => {
  const id = player.state.segmentId;
  if (!reader.book || !id || player.state.bookId !== reader.book.id) return null;
  const segment = reader.chapter?.blocks.flatMap((b) => b.segments).find((s) => s.id === id);
  return segment?.canSayAgain ? segment.id : null;
});
const asking = ref(false);
const busy = computed(
  () =>
    asking.value ||
    (player.generation.bookId === reader.book?.id && player.generation.generatingSegmentId === sentence.value),
);
async function sayAgain() {
  if (!reader.book || !sentence.value || busy.value) return;
  asking.value = true;
  try {
    await player.sayAgain(reader.book.id, sentence.value);
  } finally {
    asking.value = false;
  }
}
</script>

<template>
  <UiButton
    v-if="sentence"
    variant="glass"
    size="lg"
    icon
    class="say-again"
    aria-label="Say this again"
    :title="busy ? 'Making this sentence again…' : 'Say this again: make the sentence being spoken again'"
    :aria-busy="busy"
    :disabled="busy"
    @click="sayAgain"
  >
    <LoaderCircle v-if="busy" :size="18" class="say-again__spin" aria-hidden="true" />
    <RotateCcw v-else :size="18" aria-hidden="true" />
  </UiButton>
</template>

<style scoped>
.say-again:disabled {
  opacity: 1;
  cursor: progress;
}
.say-again__spin {
  animation: say-again-spin 1s linear infinite;
}
@keyframes say-again-spin {
  to {
    transform: rotate(1turn);
  }
}
@media (prefers-reduced-motion: reduce) {
  .say-again__spin {
    animation: none;
  }
}
</style>
