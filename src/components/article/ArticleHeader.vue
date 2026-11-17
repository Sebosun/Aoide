<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { BookmarkCheck, BookmarkPlus, ExternalLink } from "@lucide/vue";
import type { ArticleView } from "@/api/types";
import { formatDuration, readingMs } from "@/estimate";
import FeedOffer from "@/components/article/FeedOffer.vue";
import { siteOf } from "@/articles";

/** The top of an article in the reader: its site, date and reading time, a way to the
 *  original page, then its title and byline. One opened from a feed offers Save to library,
 *  and says it is in the library once saved. */
const props = defineProps<{ article: ArticleView; title: string; authors: string[]; words: number; wpm: number }>();
const emit = defineEmits<{ original: []; save: [] }>();
const fromFeed = ref(!props.article.inLibrary);
watch(
  () => props.article.inLibrary,
  (kept) => {
    if (!kept) fromFeed.value = true;
  },
);
const published = computed(() =>
  props.article.publishedAt === null
    ? null
    : new Date(props.article.publishedAt).toLocaleDateString(undefined, {
        day: "numeric",
        month: "long",
        year: "numeric",
      }),
);
const byline = computed(() => props.article.byline ?? (props.authors.join(", ") || null));
const readTime = computed(() => `${formatDuration(readingMs(props.words, props.wpm))} read`);
</script>

<template>
  <header class="article-header" aria-label="About this article">
    <div class="article-header__facts">
      <span class="article-header__site">{{ siteOf(article) }}</span>
      <span v-if="published">{{ published }}</span>
      <span>{{ readTime }}</span>
      <button type="button" class="article-header__original" @click="emit('original')">
        Open original<ExternalLink :size="13" aria-hidden="true" />
      </button>
      <button v-if="!article.inLibrary" type="button" class="article-header__save" @click="emit('save')">
        <BookmarkPlus :size="15" aria-hidden="true" />Save to library
      </button>
      <span v-else-if="fromFeed" class="article-header__kept"
        ><BookmarkCheck :size="15" aria-hidden="true" />In your library</span
      >
    </div>
    <h1 class="article-header__title">{{ title }}</h1>
    <p v-if="byline" class="article-header__byline">{{ byline }}</p>
    <FeedOffer class="article-header__offer" :article="article" />
  </header>
</template>

<style scoped>
.article-header {
  display: flex;
  flex-direction: column;
  padding-bottom: var(--space-5);
  font-family: var(--font-ui);
}
.article-header__facts {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3) var(--space-7);
  font-size: 13px;
  color: var(--dim);
}
.article-header__site {
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}
.article-header__original {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  padding: 0;
  background: none;
  color: var(--dim);
  text-decoration: underline;
}
.article-header__original:hover {
  color: var(--ink);
}
.article-header__save,
.article-header__kept {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: var(--space-3);
  font-weight: 600;
}
.article-header__save {
  height: 36px;
  padding: 0 var(--space-7);
  border-radius: var(--radius-lg);
  background: var(--ink);
  color: var(--ground);
}
.article-header__kept {
  color: var(--ink);
}
.article-header__title {
  margin: var(--space-7) 0 var(--space-4);
  font: 500 calc(44px * var(--read-scale)) / 1.08 var(--read-font);
  letter-spacing: -0.01em;
  color: var(--ink);
}
.article-header__byline {
  margin: 0;
  font-size: 15px;
  color: var(--dim);
}
</style>
