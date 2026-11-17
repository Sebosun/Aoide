<script setup lang="ts">
import UiCard from "@/components/ui/UiCard.vue";
import UiButton from "@/components/ui/UiButton.vue";
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  ChevronRight,
  Copy,
  Ellipsis,
  ExternalLink,
  FolderPlus,
  Pencil,
  Plus,
  TriangleAlert,
  Trash2,
} from "@lucide/vue";
import type { EntryView, FeedView } from "@/api/types";
import FeedEntry from "@/components/feeds/FeedEntry.vue";
import UiMenu from "@/components/ui/UiMenu.vue";
import type { MenuEntry } from "@/components/ui/types";
import { api } from "@/api";
import { FEED_DRAG } from "@/drag";
import { age, dayName, localDay } from "@/days";
import { useFeedsStore, type FeedsSource } from "@/stores/feeds";
import { useLibraryStore } from "@/stores/library";
import { useNavigationStore } from "@/stores/navigation";
import { useUiStore } from "@/stores/ui";

/** Subscriptions on the left, their entries by day on the right. */
const feeds = useFeedsStore(),
  library = useLibraryStore(),
  navigation = useNavigationStore(),
  ui = useUiStore();
onMounted(() => void feeds.start());

const summary = computed(() => {
  const news = feeds.unread ? `${feeds.unread} new` : "Nothing new";
  return feeds.checkedAt ? `${news} · checked ${age(feeds.checkedAt)}` : news;
});
const isCurrent = (source: FeedsSource) => JSON.stringify(feeds.source) === JSON.stringify(source);
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
/** The folders, by name, each with its feeds; then the feeds in none. */
const folders = computed(() => {
  const byName = new Map<string, FeedView[]>();
  for (const feed of feeds.feeds) if (feed.folder) byName.set(feed.folder, [...(byName.get(feed.folder) ?? []), feed]);
  return [...byName]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([name, list]) => ({ name, feeds: list, unread: list.reduce((sum, f) => sum + f.unread, 0) }));
});
/** The list's rows: each folder, with its feeds under it unless folded; then the other feeds. */
type Row =
  | { kind: "folder"; key: string; name: string; unread: number; open: boolean }
  | { kind: "feed"; key: string; feed: FeedView; nested: boolean };
const rows = computed<Row[]>(() => [
  ...folders.value.flatMap(({ name, feeds: list, unread }): Row[] => {
    const open = !folded.value.includes(name);
    const folder: Row = { kind: "folder", key: `folder:${name}`, name, unread, open };
    return [
      folder,
      ...(open ? list.map((feed): Row => ({ kind: "feed", key: `feed:${feed.id}`, feed, nested: true })) : []),
    ];
  }),
  ...feeds.feeds
    .filter((f) => !f.folder)
    .map((feed): Row => ({ kind: "feed", key: `feed:${feed.id}`, feed, nested: false })),
]);

/** Folders folded away, remembered on this computer. */
const FOLDED = "aoide.feeds.folded";
const folded = ref<string[]>(remembered());
function remembered(): string[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(FOLDED) ?? "[]");
    return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
  } catch {
    return [];
  }
}
function fold(name: string) {
  folded.value = folded.value.includes(name) ? folded.value.filter((n) => n !== name) : [...folded.value, name];
  try {
    localStorage.setItem(FOLDED, JSON.stringify(folded.value));
  } catch {
    // Not remembered; it still folds.
  }
}

async function newFolder(feed: FeedView) {
  const name = await ui.prompt({
    title: "New folder",
    label: "Folder name",
    confirmLabel: "Move",
    placeholder: "News",
  });
  if (name) await feeds.setFolder(feed.id, name);
}
async function renameFolder(name: string) {
  const to = await ui.prompt({ title: "Rename folder", label: "Folder name", value: name, confirmLabel: "Rename" });
  if (!to || to === name) return;
  if (folded.value.includes(name)) {
    fold(name);
    if (!folded.value.includes(to)) fold(to);
  }
  await feeds.renameFolder(name, to);
}
const folderActions = (name: string): MenuEntry[] => [
  { id: "rename", label: "Rename…", icon: Pencil, action: () => void renameFolder(name) },
];

/** The feed being dragged onto a folder, and the folder it is over ("" for none). */
const dropOn = ref<string | null>(null);
function dragFeed(event: DragEvent, feed: FeedView) {
  if (!event.dataTransfer) return;
  event.dataTransfer.setData(FEED_DRAG, String(feed.id));
  event.dataTransfer.effectAllowed = "move";
}
function dragOver(event: DragEvent, folder: string) {
  if (!Array.from(event.dataTransfer?.types ?? []).includes(FEED_DRAG)) return;
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  dropOn.value = folder;
}
function drop(event: DragEvent, folder: string | null) {
  dropOn.value = null;
  const id = Number(event.dataTransfer?.getData(FEED_DRAG));
  if (!id) return;
  event.preventDefault();
  // The window would otherwise take it for a link or files.
  event.stopPropagation();
  if (feeds.feeds.find((f) => f.id === id)?.folder !== folder) void feeds.setFolder(id, folder);
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
  // The collection its entries go on when saved to the library.
  { kind: "heading", label: "Save entries into" },
  ...[{ id: null, name: "No collection" }, ...library.collections].map((c) => ({
    id: `collection:${c.id ?? "none"}`,
    label: c.name,
    ariaLabel: c.name,
    checked: feed.collectionId === c.id,
    action: () => void feeds.setCollection(feed.id, c.id),
  })),
  { kind: "separator" },
  { kind: "heading", label: "Move to folder" },
  ...[null, ...folders.value.map((f) => f.name)].map((name) => ({
    id: `folder:${name ?? ""}`,
    label: name ?? "No folder",
    ariaLabel: name ?? "No folder",
    checked: (feed.folder ?? null) === name,
    action: () => void feeds.setFolder(feed.id, name),
  })),
  { id: "new-folder", label: "New folder…", icon: FolderPlus, action: () => void newFolder(feed) },
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
        <h2
          class="feeds__heading"
          :class="{ 'feeds__heading--drop': dropOn === '' }"
          @dragover="dragOver($event, '')"
          @dragleave="dropOn = null"
          @drop="drop($event, null)"
        >
          Subscriptions
        </h2>
        <template v-for="row in rows" :key="row.key">
          <div
            v-if="row.kind === 'folder'"
            class="feeds__row feeds__row--folder"
            :class="{ 'feeds__row--drop': dropOn === row.name }"
            @dragover="dragOver($event, row.name)"
            @dragleave="dropOn = null"
            @drop="drop($event, row.name)"
          >
            <button
              type="button"
              class="feeds__fold"
              :class="{ 'feeds__fold--open': row.open }"
              :aria-label="`Show or hide ${row.name}`"
              :aria-expanded="row.open"
              @click="fold(row.name)"
            >
              <ChevronRight :size="15" aria-hidden="true" />
            </button>
            <button
              type="button"
              class="feeds__source"
              :class="{ 'feeds__source--quiet': !row.unread }"
              :aria-current="isCurrent({ kind: 'folder', name: row.name })"
              @click="feeds.choose({ kind: 'folder', name: row.name })"
            >
              <span class="feeds__line">
                <span class="feeds__name">{{ row.name }}</span><span v-if="row.unread" class="feeds__count">{{
                  row.unread
                }}</span>
              </span>
            </button>
            <UiMenu
              v-slot="{ toggle, trigger }"
              class="feeds__more-menu"
              :label="`Actions for folder ${row.name}`"
              :items="folderActions(row.name)"
              align="end"
            >
              <button
                type="button"
                class="feeds__icon"
                :aria-label="`More for folder ${row.name}`"
                v-bind="trigger"
                @click="toggle"
              >
                <Ellipsis :size="15" aria-hidden="true" />
              </button>
            </UiMenu>
          </div>
          <div
            v-else
            class="feeds__row"
            :class="{ 'feeds__row--nested': row.nested }"
            draggable="true"
            @dragstart="dragFeed($event, row.feed)"
          >
            <button
              type="button"
              class="feeds__source"
              :class="{ 'feeds__source--quiet': !row.feed.unread }"
              :aria-current="isCurrent({ kind: 'feed', id: row.feed.id })"
              :title="row.feed.error ?? row.feed.url"
              @click="feeds.choose({ kind: 'feed', id: row.feed.id })"
            >
              <span class="feeds__line">
                <span class="feeds__name">{{ feedName(row.feed) }}</span><span
                  v-if="row.feed.unread"
                  class="feeds__count"
                  >{{ row.feed.unread }}</span
                >
              </span>
              <span v-if="failingSince(row.feed)" class="feeds__failing"
                ><TriangleAlert :size="13" aria-hidden="true" />{{ failingSince(row.feed) }}</span
              >
            </button>
            <UiMenu
              v-slot="{ toggle, trigger }"
              class="feeds__more-menu"
              :label="`Actions for ${feedName(row.feed)}`"
              :items="actions(row.feed)"
              align="end"
            >
              <button
                type="button"
                class="feeds__icon"
                :aria-label="`More for ${feedName(row.feed)}`"
                v-bind="trigger"
                @click="toggle"
              >
                <Ellipsis :size="15" aria-hidden="true" />
              </button>
            </UiMenu>
          </div>
        </template>
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
              @queue="(place) => feeds.queue(entry, place)"
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
.feeds__row--nested {
  padding-left: var(--space-12);
}
.feeds__row--drop .feeds__source,
.feeds__heading--drop {
  color: var(--ink);
  background: var(--glass);
  box-shadow: inset 0 0 0 2px var(--accent);
  border-radius: var(--radius-md);
}
.feeds__fold {
  flex: none;
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  border-radius: var(--radius-sm);
  color: var(--dim);
}
.feeds__fold:hover {
  color: var(--ink);
  background: var(--glass);
}
.feeds__fold svg {
  transition: transform 0.15s ease;
}
.feeds__fold--open svg {
  transform: rotate(90deg);
}
@media (prefers-reduced-motion: reduce) {
  .feeds__fold svg {
    transition: none;
  }
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
