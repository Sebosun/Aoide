<script setup lang="ts">
import { computed, ref } from "vue";
import { Rss } from "@lucide/vue";
import type { ArticleView } from "@/api/types";
import { api } from "@/api";
import { useArticlesStore } from "@/stores/articles";
import { useFeedsStore } from "@/stores/feeds";
import { useUiStore } from "@/stores/ui";

/** "This site has a feed": the first feed an article's page names that is not subscribed. */
const props = defineProps<{ article: ArticleView }>();
const feed = computed(() => props.article.feeds.find((f) => !f.subscribed) ?? null);
const name = computed(() => (feed.value ? (feed.value.title ?? new URL(feed.value.url).hostname) : ""));
const busy = ref(false);
const articles = useArticlesStore(),
  ui = useUiStore();
async function subscribe() {
  if (!feed.value) return;
  busy.value = true;
  try {
    const result = await api.subscribe({ url: feed.value.url });
    if (result.kind !== "subscribed") throw new Error("No feed was found at that address.");
    ui.toast(`Subscribed to ${result.value.title ?? result.value.url}`);
    await Promise.all([articles.load(props.article.id), useFeedsStore().refresh()]);
  } catch (e) {
    ui.toast(String(e).replace(/^Error: /, ""), "error");
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div v-if="feed" class="feed-offer">
    <Rss :size="18" aria-hidden="true" />
    <span class="feed-offer__text">This site has a feed: {{ name }}</span>
    <button type="button" class="feed-offer__button" :disabled="busy" @click="subscribe">Subscribe</button>
  </div>
</template>

<style scoped>
.feed-offer {
  display: flex;
  align-items: center;
  gap: var(--space-6);
  margin-top: var(--space-8);
  padding: var(--space-4) var(--space-4) var(--space-4) var(--space-7);
  border-radius: var(--radius-md);
  background: var(--wash);
  font-family: var(--font-ui);
  font-size: 14px;
}
.feed-offer__text {
  flex: 1;
  min-width: 0;
  font-weight: 600;
}
.feed-offer__button {
  height: 36px;
  padding: 0 var(--space-7);
  border-radius: var(--radius-lg);
  background: var(--ink);
  color: var(--ground);
  font-weight: 600;
}
</style>
