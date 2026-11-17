<script setup lang="ts">
import { computed } from "vue";
import { Pause, Play, X } from "@lucide/vue";
import { useLinkImportStore } from "@/stores/linkImport";

/** An import of links in the sidebar: how far it is, pause and cancel while it fetches, and
 *  its summary when done (with archived copies of what failed), until dismissed. */
const imports = useLinkImportStore();
const view = computed(() => imports.view);
const done = computed(() => view.value.ready + view.value.partial + view.value.failed);
const percent = computed(() => (view.value.of ? Math.round((done.value / view.value.of) * 100) : 0));
const outcomes = computed(() =>
  [
    `${view.value.ready} ready`,
    view.value.partial ? `${view.value.partial} partly captured` : "",
    view.value.failed ? `${view.value.failed} failed` : "",
    view.value.already ? `${view.value.already} already in the library` : "",
  ].filter(Boolean),
);
</script>

<template>
  <section class="link-card" aria-label="Importing links">
    <div class="link-card__head">
      <span class="link-card__title">{{
        view.stage === "done" ? "Links imported" : `Importing from ${view.source ?? "another app"}`
      }}</span>
      <button
        v-if="view.stage === 'done'"
        type="button"
        class="link-card__icon"
        aria-label="Dismiss the import summary"
        @click="imports.dismiss()"
      >
        <X :size="14" aria-hidden="true" />
      </button>
    </div>
    <template v-if="view.stage !== 'done'">
      <span class="link-card__track"><i :style="{ width: `${percent}%` }" /></span>
      <span class="link-card__line">{{ view.stage === "paused" ? "Paused · " : "" }}{{ done }} of {{ view.of }}</span>
      <div class="link-card__actions">
        <button
          type="button"
          class="link-card__action"
          :aria-label="view.stage === 'paused' ? 'Resume importing' : 'Pause importing'"
          @click="imports.pause(view.stage !== 'paused')"
        >
          <component :is="view.stage === 'paused' ? Play : Pause" :size="12" aria-hidden="true" />
          {{ view.stage === "paused" ? "Resume" : "Pause" }}
        </button>
        <button type="button" class="link-card__action" aria-label="Cancel importing" @click="imports.cancel()">
          Cancel
        </button>
      </div>
    </template>
    <template v-else>
      <span class="link-card__line link-card__line--wrap">{{ outcomes.join(" · ") }}</span>
      <button
        v-if="view.failed"
        type="button"
        class="link-card__action"
        :aria-label="`Try archived copies of the ${view.failed} that failed`"
        @click="imports.retryArchived()"
      >
        Try archived copies of the {{ view.failed }} that failed
      </button>
    </template>
  </section>
</template>

<style scoped>
.link-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-6) var(--space-7);
  border-radius: var(--radius-lg);
  background: var(--page);
}
.link-card__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}
.link-card__title {
  font-weight: 700;
  font-size: 13px;
}
.link-card__icon {
  display: grid;
  place-items: center;
  color: var(--dim);
  border-radius: var(--radius-pill);
}
.link-card__track {
  height: 3px;
  border-radius: var(--radius-xs);
  background: var(--wash);
}
.link-card__track i {
  display: block;
  height: 100%;
  border-radius: var(--radius-xs);
  background: var(--accent);
}
.link-card__line {
  font-size: 12px;
  color: var(--dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}
.link-card__line--wrap {
  white-space: normal;
}
.link-card__actions {
  display: flex;
  gap: var(--space-4);
}
.link-card__action {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  font-size: 12px;
  font-weight: 600;
  color: var(--ink);
  text-align: left;
}
.link-card__action:hover {
  color: var(--accent);
}
</style>
