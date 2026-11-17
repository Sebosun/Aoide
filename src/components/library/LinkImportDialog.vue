<script setup lang="ts">
import { computed, ref } from "vue";
import { Download } from "@lucide/vue";
import type { LinkChoice } from "@/api/contract";
import UiButton from "@/components/ui/UiButton.vue";
import UiDialog from "@/components/ui/UiDialog.vue";
import { useLinkImportStore } from "@/stores/linkImport";

/** The links an export holds, and which to import: all, only the unread ones, or the most
 *  recent. Nothing is made until Import. */
const imports = useLinkImportStore();
const view = computed(() => imports.view);
const which = ref<"all" | "unread" | "recent">("all"),
  count = ref(100);
const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
const notes = computed(() => {
  const v = view.value;
  return [
    v.already ? `${v.already} already in your library` : "",
    v.duplicates ? `${plural(v.duplicates, "link")} listed twice` : "",
    v.notWeb ? `${plural(v.notWeb, "entry", "entries")} that ${v.notWeb === 1 ? "is" : "are"} not a web page` : "",
  ].filter(Boolean);
});
function choice(): LinkChoice {
  if (which.value === "recent") return { which: "recent", count: Math.max(1, Math.round(count.value)) };
  return { which: which.value };
}
</script>

<template>
  <UiDialog :label="`Import from ${view.source ?? 'another app'}`" initial-focus="input" @close="imports.cancel()">
    <form class="link-import" @submit.prevent="imports.begin(choice())">
      <h1 class="link-import__title">Import from {{ view.source ?? "another app" }}</h1>
      <p class="link-import__found">
        {{ plural(view.found, "link") }}<template v-if="notes.length"> ({{ notes.join(", ") }})</template>.
      </p>
      <fieldset class="link-import__choices">
        <legend class="visually-hidden">Which links to import</legend>
        <label class="link-import__choice"
          ><input v-model="which" type="radio" value="all" :aria-label="`All of them (${view.found})`" /> All of them
          ({{ view.found }})</label
        >
        <label v-if="view.unread !== view.found" class="link-import__choice"
          ><input v-model="which" type="radio" value="unread" :aria-label="`Only the unread ones (${view.unread})`" />
          Only the unread ones ({{ view.unread }})</label
        >
        <label class="link-import__choice"
          ><input v-model="which" type="radio" value="recent" aria-label="Only the most recent" /> Only the most recent
          <input
            v-model.number="count"
            class="link-import__count"
            type="number"
            min="1"
            :max="view.found"
            aria-label="How many of the most recent"
            @focus="which = 'recent'"
          />
        </label>
      </fieldset>
      <p class="link-import__hint">
        Each becomes an article in your library, fetched quietly in the background, one site at a time; anything you
        save meanwhile goes first. Tags and folders become collections, and links you had archived arrive archived.
      </p>
      <p v-if="imports.error" class="link-import__error" role="alert">{{ imports.error }}</p>
      <div class="link-import__actions">
        <UiButton variant="outline" @click="imports.cancel()">Cancel</UiButton>
        <UiButton variant="primary" type="submit" aria-label="Import links">
          <Download :size="15" aria-hidden="true" /> Import
        </UiButton>
      </div>
    </form>
  </UiDialog>
</template>

<style scoped>
.link-import {
  width: min(480px, calc(100vw - 32px));
  display: grid;
  gap: var(--space-5);
  padding: var(--space-12);
}
.link-import__title {
  font-size: 18px;
  font-weight: 800;
  letter-spacing: -0.01em;
}
.link-import__found,
.link-import__hint {
  color: var(--dim);
  line-height: 1.5;
  font-size: 13px;
}
.link-import__choices {
  display: grid;
  gap: var(--space-3);
  margin: 0;
  padding: 0;
  border: 0;
}
.link-import__choice {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  font-size: 13.5px;
}
.link-import__count {
  width: 6em;
  padding: var(--space-2) var(--space-4);
  color: var(--ink);
}
.link-import__error {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--accent);
}
.link-import__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-4);
  margin-top: var(--space-5);
}
</style>
