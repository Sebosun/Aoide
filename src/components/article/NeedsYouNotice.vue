<script setup lang="ts">
import { computed } from "vue";
import { TriangleAlert } from "@lucide/vue";
import type { ArticleView, FetchReason } from "../../api/types";
import { fetching, siteOf } from "../../articles";

/** Shown after an article's captured text when the fetch got only part of it: what is
 *  missing and why (`reason`, the last one the core gave), and the ways to get the rest. The
 *  kept copy stays until a fuller one arrives; a fetch under way or failed says so. */
const props = defineProps<{ article: ArticleView; reason: FetchReason }>();
/** A fetch is under way: the actions wait for it. */
const busy = computed(() => fetching(props.article.stage));
const emit = defineEmits<{ fetch: []; archive: []; original: [] }>();
const words = (n: number) => n.toLocaleString("en-US");
const copy = computed(() => {
  const site = siteOf(props.article);
  const rest = "An archived copy may have the rest, or you can read it on the site.";
  return {
    signIn: {
      heading: "The site asked for a sign-in",
      body: `${site} showed a sign-in page partway through, so only the opening was captured. ${rest}`,
    },
    paywall: {
      heading: "The article is behind a paywall",
      body: `${site} shows only the opening without a subscription, so that is what was captured. ${rest}`,
    },
    check: {
      heading: "The site wants to check you are a person",
      body: `${site} put a verification step in front of the article. Fetching again later, or an archived copy, may get past it.`,
    },
    cutShort: {
      heading: "The article looks cut short",
      body:
        props.article.declaredWords === null
          ? `Less of the article was captured than the page promised. ${rest}`
          : `The page says the article is about ${words(props.article.declaredWords)} words, and ${words(props.article.capturedWords)} were captured. ${rest}`,
    },
    empty: {
      heading: "No text was found",
      body: `${site} sent a page with no article text to read; it may need scripts to show it. ${rest}`,
    },
  }[props.reason];
});
</script>

<template>
  <section class="needs-you" aria-label="This article needs you">
    <div class="needs-you__kicker"><TriangleAlert :size="15" aria-hidden="true" /> Needs you</div>
    <h2 class="needs-you__heading">{{ copy.heading }}</h2>
    <p class="needs-you__body">{{ copy.body }}</p>
    <div class="needs-you__actions">
      <button type="button" class="needs-you__button needs-you__button--main" :disabled="busy" @click="emit('archive')">
        Try an archived copy
      </button>
      <button type="button" class="needs-you__button" :disabled="busy" @click="emit('fetch')">Fetch again</button>
      <button type="button" class="needs-you__button" @click="emit('original')">Open original</button>
    </div>
    <p v-if="busy" class="needs-you__status" role="status">Fetching again…</p>
    <p v-else-if="article.stage.kind === 'failed' && article.error" class="needs-you__status" role="alert">
      The last fetch failed: {{ article.error }}
    </p>
    <p class="needs-you__kept">
      Kept so far: {{ words(article.capturedWords) }} words. Nothing is replaced until a fuller copy arrives.
    </p>
  </section>
</template>

<style scoped>
.needs-you {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-top: 10px;
  padding: 22px;
  border-radius: 20px;
  background: color-mix(in srgb, var(--warn) 14%, var(--page));
  font: 400 15px / 1.5 var(--font-ui);
  color: var(--ink);
}
.needs-you__kicker {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--warn);
}
.needs-you__heading {
  margin: 0;
  font-size: 22px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.needs-you__body {
  margin: 0;
  max-width: 58ch;
}
.needs-you__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  margin-top: 6px;
}
.needs-you__button {
  height: 48px;
  padding: 0 20px;
  border-radius: 24px;
  background: var(--page);
  font-weight: 600;
}
.needs-you__button--main {
  padding: 0 22px;
  background: var(--ink);
  color: var(--ground);
  font-weight: 700;
}
.needs-you__button:disabled {
  opacity: 0.55;
  cursor: default;
}
.needs-you__status {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}
.needs-you__kept {
  margin: 0;
  font-size: 13px;
  color: var(--dim);
}
</style>
