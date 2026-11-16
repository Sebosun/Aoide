<script setup lang="ts">
import UiCard from "@/components/ui/UiCard.vue";
import UiButton from "@/components/ui/UiButton.vue";
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { Copy, Ellipsis, ExternalLink, Plus, TriangleAlert, Trash2 } from "@lucide/vue";
import type { EntryView, FeedView } from "@/api/types";
import FeedEntry from "@/components/feeds/FeedEntry.vue";
import UiMenu from "@/components/ui/UiMenu.vue";
import type { MenuEntry } from "@/components/ui/types";
import { api } from "@/api";
import { age, dayName, localDay } from "@/days";
import { useFeedsStore, type FeedsSource } from "@/stores/feeds";
import { useNavigationStore } from "@/stores/navigation";
import { useUiStore } from "@/stores/ui";

/** Subscriptions on the left, their entries by day on the right. */
const feeds = useFeedsStore(),
  navigation = useNavigationStore(),
  ui = useUiStore();
onMounted(() => void feeds.start());

const summary = computed(() => {
  const news = feeds.unread ? `${feeds.unread} new` : "Nothing new";
  return feeds.checkedAt ? `${news} · checked ${age(feeds.checkedAt)}` : news;
});
const isCurrent = (source: FeedsSource) =>
  feeds.source.kind === source.kind &&
  (source.kind !== "feed" || (feeds.source.kind === "feed" && feeds.source.id === source.id));
const failingSince = (feed: FeedView) =>
  feed.stage.kind === "failing" ? `Not reachable since ${dayName(feed.stage.sinceMs)}` : "";
const feedName = (feed: FeedView) => feed.title ?? new URL(feed.url).hostname;

/** A subscription list (OPML) from another reader. */
async function importList() {
  try {
    const done = await feeds.importList();
    if (done)
      ui.toast(done.added ? `Added ${done.added} feed${done.added === 1 ? "" : "s"}` : "No new feeds in that list");
  } catch (e) {
    ui.toast(String(e).replace(/^Error: /, ""), "error");
  }
}
async function copyAddress(feed: FeedView) {
  try {
    await navigator.clipboard.writeText(feed.url);
    ui.toast("Feed address copied");
  } catch (e) {
    ui.toast(`Could not copy: ${String(e)}`, "error");
  }
}
const actions = (feed: FeedView): MenuEntry[] => [
  {
    id: "site",
    label: "Open site",
    icon: ExternalLink,
    action: () => void api.openFeedSite({ feedId: feed.id }).catch((e) => ui.toast(String(e), "error")),
  },
  { id: "copy", label: "Copy feed address", icon: Copy, action: () => void copyAddress(feed) },
  { kind: "separator" },
  {
    id: "unsubscribe",
    label: "Unsubscribe",
    icon: Trash2,
    danger: true,
    action: () => void feeds.unsubscribe(feed.id),
  },
];

/** The entries under their local day, newest first. */
const days = computed(() => {
  const groups: { day: number; name: string; entries: EntryView[] }[] = [];
  for (const entry of feeds.entries) {
    const day = localDay(entry.at);
    if (groups.at(-1)?.day !== day) {
      const name = dayName(entry.at);
      groups.push({ day, name: name[0].toUpperCase() + name.slice(1), entries: [] });
    }
    groups.at(-1)!.entries.push(entry);
  }
  return groups;
});
const emptyNote = computed(() =>
  feeds.source.kind === "saved"
    ? "Entries you save to the library show up here."
    : "New entries show up here when the feeds have them.",
);

/** Older entries load as the end of the list comes into view. */
const end = ref<HTMLElement | null>(null);
let watcher: IntersectionObserver | undefined;
watch(end, (element) => {
  watcher?.disconnect();
  if (!element || typeof IntersectionObserver === "undefined") return;
  watcher = new IntersectionObserver((seen) => {
    if (seen.some((s) => s.isIntersecting)) void feeds.loadMore();
  });
  watcher.observe(element);
});
onBeforeUnmount(() => watcher?.disconnect());
</script>

<template>
  <section class="feeds" :aria-busy="!feeds.loaded">
    <header class="feeds__header">
      <div>
        <h1 class="feeds__title">Feeds</h1>
        <p v-if="feeds.feeds.length" class="feeds__summary">{{ summary }}</p>
      </div>
      <div class="feeds__actions">
        <template v-if="feeds.feeds.length">
          <UiButton variant="raised" size="lg" class="feeds__button" @click="feeds.checkNow()">Check now</UiButton>
          <UiButton
            variant="raised"
            size="lg"
            class="feeds__button"
            :disabled="!feeds.unread"
            @click="feeds.markAllRead()"
          >
            Mark all read
          </UiButton>
        </template>
        <UiButton
          variant="primary"
          size="lg"
          class="feeds__button feeds__button--main"
          aria-haspopup="dialog"
          @click="navigation.openAdd('', 'subscribe')"
        >
          <Plus :size="16" aria-hidden="true" />Subscribe
        </UiButton>
      </div>
    </header>
    <p v-if="feeds.error" class="feeds__error" role="alert">{{ feeds.error }}</p>

    <UiCard as="div" pad="lg" v-if="feeds.loaded && !feeds.feeds.length" class="feeds__empty feeds__empty--start">
      <h2 class="feeds__empty-title">No subscriptions yet</h2>
      <p>Subscribe to a site's feed and its new articles arrive here, ready to read or listen to.</p>
      <UiButton
        variant="primary"
        size="lg"
        class="feeds__button feeds__button--main"
        aria-haspopup="dialog"
        @click="navigation.openAdd('', 'subscribe')"
      >
        <Plus :size="16" aria-hidden="true" />Subscribe
      </UiButton>
      <button type="button" class="feeds__link" @click="importList">
        Or import a subscription list from another feed reader…
      </button>
    </UiCard>

    <div v-else-if="feeds.loaded" class="feeds__body">
      <section class="feeds__sources" aria-label="Subscriptions">
        <button
          type="button"
          class="feeds__source"
          :aria-current="isCurrent({ kind: 'all' })"
          @click="feeds.choose({ kind: 'all' })"
        >
          <span class="feeds__line"
            ><span class="feeds__name">All new</span
            ><span v-if="feeds.unread" class="feeds__count">{{ feeds.unread }}</span></span
          >
        </button>
        <button
          type="button"
          class="feeds__source"
          :aria-current="isCurrent({ kind: 'saved' })"
          @click="feeds.choose({ kind: 'saved' })"
        >
          <span class="feeds__line"><span class="feeds__name">Saved for later</span></span>
        </button>
        <h2 class="feeds__heading">Subscriptions</h2>
        <div v-for="feed in feeds.feeds" :key="feed.id" class="feeds__row">
          <button
            type="button"
            class="feeds__source"
            :class="{ 'feeds__source--quiet': !feed.unread }"
            :aria-current="isCurrent({ kind: 'feed', id: feed.id })"
            :title="feed.error ?? feed.url"
            @click="feeds.choose({ kind: 'feed', id: feed.id })"
          >
            <span class="feeds__line">
              <span class="feeds__name">{{ feedName(feed) }}</span><span v-if="feed.unread" class="feeds__count">{{
                feed.unread
              }}</span>
            </span>
            <span v-if="failingSince(feed)" class="feeds__failing"
              ><TriangleAlert :size="13" aria-hidden="true" />{{ failingSince(feed) }}</span
            >
          </button>
          <UiMenu
            v-slot="{ toggle, trigger }"
            class="feeds__more-menu"
            :label="`Actions for ${feedName(feed)}`"
            :items="actions(feed)"
            align="end"
          >
            <button
              type="button"
              class="feeds__icon"
              :aria-label="`More for ${feedName(feed)}`"
              v-bind="trigger"
              @click="toggle"
            >
              <Ellipsis :size="15" aria-hidden="true" />
            </button>
          </UiMenu>
        </div>
      </section>

      <section class="feeds__entries" aria-label="Entries">
        <UiCard as="div" pad="lg" v-if="!feeds.entries.length" class="feeds__empty">
          <h2 class="feeds__empty-title">
            {{ feeds.source.kind === "saved" ? "Nothing saved yet" : "No entries yet" }}
          </h2>
          <p>{{ emptyNote }}</p>
        </UiCard>
        <template v-for="group in days" :key="group.day">
          <h2 class="feeds__day">{{ group.name }}</h2>
          <UiCard as="div" pad="list" class="feeds__card">
            <FeedEntry
              v-for="entry in group.entries"
              :key="entry.id"
              :entry="entry"
              :opening="feeds.opening === entry.id"
              @open="feeds.open(entry)"
              @save="feeds.toggleSaved(entry)"
            />
          </UiCard>
        </template>
        <button v-if="feeds.more" ref="end" type="button" class="feeds__more" @click="feeds.loadMore()">
          Show older entries
        </button>
      </section>
    </div>
  </section>
</template>

<style scoped>
.feeds {
  max-width: 1200px;
  margin: 0 auto;
  padding: var(--space-4) var(--space-16) var(--space-20);
  display: flex;
  flex-direction: column;
  gap: var(--space-10);
}
.feeds__header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-6);
}
.feeds__title {
  margin: 0;
  font-size: 24px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.feeds__summary {
  margin: var(--space-1) 0 0;
  font-size: 13px;
  color: var(--dim);
}
.feeds__actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-5);
}
.feeds__error {
  margin: 0;
  color: var(--warn);
}
.feeds__body {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  gap: var(--space-12);
}
.feeds__sources {
  flex: 1 1 230px;
  max-width: 100%;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}
.feeds__heading {
  margin: 0;
  padding: var(--space-8) var(--space-6) var(--space-3);
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--dim);
}
.feeds__row {
  position: relative;
  display: flex;
  align-items: center;
}
.feeds__row .feeds__source {
  flex: 1;
  min-width: 0;
}
.feeds__source {
  min-height: 44px;
  padding: var(--space-3) var(--space-6);
  display: flex;
  flex-direction: column;
  justify-content: center;
  border-radius: var(--radius-md);
  color: var(--dim);
  font-weight: 500;
  text-align: left;
}
.feeds__line {
  display: flex;
  justify-content: space-between;
  gap: var(--space-4);
  width: 100%;
}
.feeds__source:hover {
  color: var(--ink);
  background: var(--glass);
}
.feeds__source[aria-current="true"] {
  background: var(--page);
  color: var(--ink);
  font-weight: 700;
}
.feeds__name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.feeds__count {
  font-variant-numeric: tabular-nums;
}
.feeds__row:hover .feeds__count,
.feeds__row:focus-within .feeds__count {
  visibility: hidden;
}
.feeds__failing {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  font-size: 12px;
  font-weight: 600;
  color: var(--warn);
}
.feeds__more-menu {
  position: absolute;
  right: 4px;
  top: 7px;
  opacity: 0;
}
.feeds__row:hover .feeds__more-menu,
.feeds__row:focus-within .feeds__more-menu {
  opacity: 1;
}
.feeds__icon {
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
  border-radius: 50%;
  color: var(--dim);
}
.feeds__icon:hover,
.feeds__icon[aria-expanded="true"] {
  color: var(--ink);
  background: var(--wash);
}
.feeds__entries {
  flex: 999 1 460px;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}
.feeds__day {
  margin: var(--space-6) 0 0;
  padding: 0 var(--space-2);
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--dim);
}
.feeds__day:first-child {
  margin-top: 0;
}
.feeds__more {
  align-self: center;
  height: 40px;
  margin-top: var(--space-4);
  padding: 0 var(--space-8);
  border-radius: var(--radius-lg);
  color: var(--dim);
  font-weight: 600;
}
.feeds__more:hover {
  color: var(--ink);
  background: var(--glass);
}
.feeds__empty {
  color: var(--dim);
}
.feeds__empty p {
  margin: var(--space-3) 0 0;
}
.feeds__empty--start {
  max-width: 560px;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-2);
}
.feeds__empty--start .feeds__button {
  margin-top: var(--space-7);
}
.feeds__link {
  margin-top: var(--space-3);
  padding: 0;
  color: var(--dim);
  text-decoration: underline;
}
.feeds__link:hover {
  color: var(--ink);
}
.feeds__empty-title {
  margin: 0;
  font-size: 17px;
  color: var(--ink);
}
@media (max-width: 719px) {
  .feeds {
    padding: var(--space-4) var(--space-8) var(--space-16);
  }
}
</style>
