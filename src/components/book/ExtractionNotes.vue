<script setup lang="ts">
import { computed, ref } from "vue";
import { TriangleAlert } from "@lucide/vue";
import type { ExcludedView } from "@/api/types";

/** What extraction noted about a text: sections left out (with why) and warnings, behind a
 *  link that counts them. Nothing, when there is nothing to note. */
const props = defineProps<{ warnings: string[]; excluded: ExcludedView[] }>();
const count = computed(() => props.warnings.length + props.excluded.length);
const open = ref(false);
</script>

<template>
  <template v-if="count">
    <button
      type="button"
      class="extraction-notes__link"
      :aria-expanded="open"
      aria-controls="extraction-notes"
      @click="open = !open"
    >
      <TriangleAlert :size="15" aria-hidden="true" /> {{ count }} extraction {{ count === 1 ? "note" : "notes" }}
    </button>
    <div v-if="open" id="extraction-notes" class="extraction-notes">
      <p v-for="section in excluded" :key="section.title">
        <strong>{{ section.title }}</strong> left out: {{ section.reason }}
      </p>
      <p v-for="warning in warnings" :key="warning">{{ warning }}</p>
    </div>
  </template>
</template>

<style scoped>
.extraction-notes__link {
  margin-top: auto;
  align-self: flex-start;
  display: inline-flex;
  align-items: center;
  gap: var(--space-4);
  color: var(--warn);
  font-weight: 600;
}
.extraction-notes {
  font-size: 13px;
  color: var(--dim);
}
.extraction-notes p {
  margin: 0 0 var(--space-3);
}
</style>
