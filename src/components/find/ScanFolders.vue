<script setup lang="ts">
import { ref, watch } from "vue";
import UiButton from "@/components/ui/UiButton.vue";

/** The folders a search walks, as Find books and Find audiobooks let the user set them: folders
 *  to search, to skip, and hidden ones to include, one per line, with a switch for searching on
 *  its own (on start, or as books are added). Collapsed to a summary of the folders searched.
 *  `save` stores what was entered; the form says "Saved." when it resolves and emits `error`
 *  when it throws. The default slot explains what the search reads. */
export interface Folders {
  roots: string[];
  exclude: string[];
  includeHidden: string[];
}

const props = defineProps<{
  /** The stored folders, or null while they load. */
  folders: Folders | null;
  /** The stored switch. */
  auto: boolean;
  /** What the switch does ("Look for new books when Aoide starts"). */
  autoLabel: string;
  /** Read out for each box, naming what is searched for. */
  labels: { roots: string; exclude: string; includeHidden: string };
  save: (folders: Folders, auto: boolean) => Promise<void>;
}>();
const emit = defineEmits<{ error: [message: string] }>();

const roots = ref(""),
  exclude = ref(""),
  includeHidden = ref(""),
  auto = ref(props.auto),
  saved = ref(false);
watch(
  () => props.folders,
  (folders) => {
    if (!folders) return;
    roots.value = folders.roots.join("\n");
    exclude.value = folders.exclude.join("\n");
    includeHidden.value = folders.includeHidden.join("\n");
  },
  { immediate: true },
);
watch(
  () => props.auto,
  (on) => (auto.value = on),
);

const lines = (text: string) =>
  text
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
async function submit() {
  saved.value = false;
  try {
    await props.save(
      { roots: lines(roots.value), exclude: lines(exclude.value), includeHidden: lines(includeHidden.value) },
      auto.value,
    );
    saved.value = true;
  } catch (e) {
    emit("error", String(e));
  }
}
</script>

<template>
  <details class="scan-folders">
    <summary class="scan-folders__summary">
      Folders · {{ folders?.roots.length ? folders.roots.join(", ") : "your home folder" }}
    </summary>
    <form class="scan-folders__form" @submit.prevent="submit">
      <label class="scan-folders__field"
        ><span>Search<br /><small>one per line; empty = home folder</small></span
        ><textarea v-model="roots" rows="2" :aria-label="labels.roots" spellcheck="false" />
      </label>
      <label class="scan-folders__field"
        ><span>Skip<br /><small>one per line</small></span
        ><textarea v-model="exclude" rows="2" :aria-label="labels.exclude" spellcheck="false" />
      </label>
      <label class="scan-folders__field"
        ><span>Hidden folders to include<br /><small>skipped by default</small></span
        ><textarea v-model="includeHidden" rows="2" :aria-label="labels.includeHidden" spellcheck="false" />
      </label>
      <label class="scan-folders__check"
        ><input v-model="auto" type="checkbox" :aria-label="autoLabel" /> {{ autoLabel }}</label
      >
      <p class="scan-folders__hint"><slot /></p>
      <div class="scan-folders__row">
        <UiButton size="sm" type="submit">Save folders</UiButton
        ><span v-if="saved" class="scan-folders__ok" role="status">Saved.</span>
      </div>
    </form>
  </details>
</template>

<style scoped>
.scan-folders {
  margin: var(--space-4) 0 var(--space-12);
  padding: var(--space-8) var(--space-10);
  border-radius: var(--radius-xl);
  background: var(--page);
  font-size: 13px;
}
.scan-folders__summary {
  cursor: pointer;
  font-weight: 600;
  color: var(--dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.scan-folders__form {
  display: grid;
  gap: var(--space-6);
  margin-top: var(--space-7);
}
.scan-folders__field {
  display: grid;
  grid-template-columns: 170px minmax(0, 1fr);
  gap: var(--space-6);
  align-items: start;
}
.scan-folders__field small {
  color: var(--faint);
}
.scan-folders__check {
  display: inline-flex;
  align-items: center;
  gap: var(--space-4);
}
.scan-folders__hint {
  font-size: 12px;
  color: var(--dim);
}
.scan-folders__row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-5);
}
.scan-folders__ok {
  color: var(--ok);
  font-size: 12px;
  font-weight: 600;
}
@media (max-width: 560px) {
  .scan-folders__field {
    grid-template-columns: 1fr;
  }
}
</style>
