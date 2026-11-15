<script setup lang="ts">
import UiButton from "@/components/ui/UiButton.vue";
import { computed, useId } from "vue";
import { Check, X } from "@lucide/vue";
import UiDialog from "@/components/ui/UiDialog.vue";
import { useSignInStore } from "@/stores/signin";
import { useLibraryStore } from "@/stores/library";

/** A sign-in the core runs: three steps drawn from its stage, "Stay signed in to this site",
 *  and Cancel, Show the window, "I'm signed in, fetch again". It closes itself once the
 *  article is fetched (the store reports the result as a toast). */
const signIn = useSignInStore(),
  library = useLibraryStore();
const id = useId();
const view = computed(() => signIn.view!);
const stage = computed(() => view.value.stage.kind);
const title = computed(() => {
  const site = view.value.site;
  switch (view.value.reason) {
    case "check":
      return `Pass the check on ${site}`;
    case "cutShort":
    case "empty":
      return `Open ${site} in a window`;
    default:
      return `Sign in to ${site}`;
  }
});
const article = computed(() => library.books.find((b) => b.id === view.value.articleId)?.title ?? null);
/** Where each step is: done, the current one, or still to come. */
const steps = computed(() => {
  const at = { opening: 0, waiting: 1, fetching: 2, done: 3, cancelled: -1, failed: -1 }[stage.value];
  const state = (i: number) => (at > i ? "done" : at === i ? "current" : "todo");
  return [
    {
      title: stage.value === "opening" ? "Opening a browser window…" : "A browser window is open",
      hint: at > 0 ? "Done" : "",
      state: state(0),
    },
    {
      title:
        view.value.reason === "check"
          ? "Complete the site’s check there"
          : "Sign in there, or complete the site’s check",
      hint: "Aoide is watching that window and carries on when the article appears.",
      state: state(1),
    },
    { title: "The article is fetched again", hint: "The window closes by itself", state: state(2) },
  ];
});
const waiting = computed(() => stage.value === "waiting");
const failed = computed(() => (view.value.stage.kind === "failed" ? view.value.stage.error : null));
function close() {
  if (failed.value) signIn.dismiss();
  else if (stage.value === "opening" || stage.value === "waiting") void signIn.cancel();
}
</script>

<template>
  <UiDialog :label="title" :describedby="`${id}-note`" initial-focus="[data-signin='done']" @close="close">
    <div class="signin">
      <header class="signin__head">
        <div>
          <h2 class="signin__title">{{ title }}</h2>
          <p v-if="article" class="signin__for">For “{{ article }}”</p>
        </div>
        <UiButton size="sm" icon class="signin__close" aria-label="Close" @click="close">
          <X :size="14" aria-hidden="true" />
        </UiButton>
      </header>

      <p v-if="failed" class="signin__failed" role="alert">{{ failed }}</p>
      <ol v-else class="signin__steps">
        <li
          v-for="step in steps"
          :key="step.title"
          class="signin__step"
          :class="`signin__step--${step.state}`"
          :aria-current="step.state === 'current' ? 'step' : undefined"
        >
          <span class="signin__mark" aria-hidden="true"
            ><Check v-if="step.state === 'done'" :size="12" :stroke-width="2.5"
          /></span>
          <span class="signin__text">
            <span class="signin__step-title">{{ step.title }}</span>
            <span v-if="step.hint" class="signin__hint">{{ step.hint }}</span>
          </span>
        </li>
      </ol>

      <label class="signin__stay">
        <input
          type="checkbox"
          :checked="view.staySignedIn"
          :disabled="!!failed || stage === 'fetching'"
          @change="signIn.stay(($event.target as HTMLInputElement).checked)"
        />
        <span class="signin__text">
          <span class="signin__step-title">Stay signed in to this site</span>
          <span class="signin__hint"
            >Later articles and feed entries from {{ view.site }} are fetched with this sign-in.</span
          >
        </span>
      </label>

      <p :id="`${id}-note`" class="signin__note">
        The window uses Aoide’s own browser profile. Your sign-in is stored there, on this computer, apart from your
        everyday browser. Manage it in Settings, under Sites.
      </p>
      <p v-if="signIn.error" class="signin__failed" role="alert">{{ signIn.error }}</p>

      <div class="signin__actions">
        <UiButton size="lg" v-if="failed" class="signin__button" @click="signIn.dismiss()">Close</UiButton>
        <template v-else>
          <UiButton size="lg" class="signin__button" :disabled="stage === 'fetching'" @click="signIn.cancel()">
            Cancel
          </UiButton>
          <UiButton size="lg" class="signin__button" :disabled="!waiting" @click="signIn.showWindow()">
            Show the window
          </UiButton>
          <UiButton
            variant="primary"
            size="lg"
            class="signin__button signin__button--main"
            data-signin="done"
            :disabled="!waiting"
            @click="signIn.finish()"
          >
            I’m signed in, fetch again
          </UiButton>
        </template>
      </div>
    </div>
  </UiDialog>
</template>

<style scoped>
.signin {
  width: min(520px, calc(100vw - 32px));
  display: grid;
  gap: var(--space-9);
  padding: var(--space-12);
  font: 400 14px / 1.5 var(--font-ui);
  color: var(--ink);
}
.signin__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-8);
}
.signin__title {
  margin: 0;
  font-size: 20px;
  font-weight: 700;
  letter-spacing: -0.01em;
  overflow-wrap: anywhere;
}
.signin__for {
  margin: var(--space-2) 0 0;
  color: var(--dim);
  font-size: 13px;
}
.signin__steps {
  display: grid;
  gap: var(--space-7);
  margin: 0;
  padding: 0;
  list-style: none;
}
.signin__step,
.signin__stay {
  display: flex;
  align-items: flex-start;
  gap: var(--space-6);
}
.signin__mark {
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  flex: none;
  margin-top: 1px;
  border-radius: 50%;
  border: 2px solid var(--line);
}
.signin__step--done .signin__mark {
  background: var(--ok);
  border-color: var(--ok);
  color: var(--page);
}
.signin__step--current .signin__mark {
  border-color: var(--accent);
  box-shadow: inset 0 0 0 4px var(--page);
  background: var(--accent);
}
.signin__step--todo {
  color: var(--dim);
}
.signin__text {
  display: grid;
  gap: var(--space-1);
}
.signin__step-title {
  font-weight: 700;
}
.signin__hint {
  font-size: 13px;
  color: var(--dim);
}
.signin__stay {
  padding: var(--space-7) var(--space-8);
  border-radius: var(--radius-lg);
  background: var(--ground);
  cursor: pointer;
}
.signin__stay input {
  width: 18px;
  height: 18px;
  margin-top: var(--space-1);
  accent-color: var(--accent);
}
.signin__note {
  margin: 0;
  font-size: 13px;
  color: var(--dim);
}
.signin__failed {
  margin: 0;
  font-weight: 600;
  color: var(--accent);
}
.signin__actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: var(--space-5);
}
</style>
