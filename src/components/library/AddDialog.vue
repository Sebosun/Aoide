<script setup lang="ts">
import UiButton from "@/components/ui/UiButton.vue";
import { onMounted, ref } from "vue";
import {
  ClipboardPaste,
  Download,
  FilePlus,
  FolderOpen,
  FolderSearch,
  Headphones,
  RefreshCw,
  Rss,
  X,
} from "@lucide/vue";
import { useLinkImportStore } from "@/stores/linkImport";
import type { DiscoveredFeed } from "@/api/types";
import UiDialog from "@/components/ui/UiDialog.vue";
import { api } from "@/api";
import { useLibraryStore } from "@/stores/library";
import { useNavigationStore } from "@/stores/navigation";
import { useRecordingsStore } from "@/stores/recordings";
import { useArticlesStore } from "@/stores/articles";
import { asLink } from "@/articles";

/** Everything that adds to the library, in one place: opened by the sidebar's Add and Ctrl N.
 *  Files can also be dropped anywhere on the window (App routes them). */
const library = useLibraryStore(),
  navigation = useNavigationStore(),
  recordings = useRecordingsStore();
const close = () => navigation.closeAdd();
/** A link to save as an article: filled in when one was pasted or dropped on the window. */
const articles = useArticlesStore();
const link = ref(navigation.addLink),
  linkError = ref(""),
  saving = ref(false),
  field = ref<HTMLInputElement | null>(null);
onMounted(() => {
  if (link.value) field.value?.focus();
});
/** Opened from Feeds to subscribe: the link is a feed, or a site that names one. */
const subscribing = navigation.addMode === "subscribe";
/** The collections what is added here goes on, chosen as chips. */
const into = ref<number[]>([]);
const toggleInto = (id: number) =>
  (into.value = into.value.includes(id) ? into.value.filter((c) => c !== id) : [...into.value, id]);
/** The feeds a site names, to pick one when it names several. */
const choices = ref<DiscoveredFeed[]>([]);
/** Fills the field from the clipboard. Where the window may not read it, the field takes the
 *  focus and the user pastes with Ctrl V. */
async function paste() {
  linkError.value = "";
  try {
    const text = (await navigator.clipboard.readText()).trim();
    if (text) link.value = text;
  } catch {
    linkError.value = "Press Ctrl V to paste.";
  }
  field.value?.focus();
}
async function submitLink() {
  const valid = asLink(link.value);
  if (!valid) {
    linkError.value = "That is not a web address.";
    return;
  }
  linkError.value = "";
  choices.value = [];
  saving.value = true;
  try {
    if (subscribing) await subscribe(valid);
    else await articles.add(valid, into.value);
  } catch (e) {
    linkError.value = String(e).replace(/^Error: /, "");
  } finally {
    saving.value = false;
  }
}
async function subscribe(url: string) {
  const result = await api.subscribe({ url });
  if (result.kind === "subscribed") await articles.showFeed(result.value);
  else if (result.kind === "choose") choices.value = result.value;
  else linkError.value = "No feed was found at that address.";
}
async function choose(feed: DiscoveredFeed) {
  saving.value = true;
  try {
    await subscribe(feed.url);
  } catch (e) {
    linkError.value = String(e).replace(/^Error: /, "");
  } finally {
    saving.value = false;
  }
}
/** EPUBs, PDFs and saved emails join the library; Library opens to show them (and anything that could not be read). */
async function chooseBooks() {
  const paths = await api.chooseBooks().catch((e) => {
    library.error = String(e);
    return null;
  });
  if (!paths?.length) return;
  close();
  navigation.view = "library";
  const collections = into.value;
  await library.addToCollections(await library.importFiles(paths), collections);
}
/** An audiobook (files or a folder) is paired with a book or added on its own, as when dropped. */
async function chooseAudio(folder: boolean) {
  const paths = await api.chooseAudio({ folder }).catch((e) => {
    library.error = String(e);
    return null;
  });
  if (!paths?.length) return;
  close();
  const open = navigation.view === "item" ? navigation.bookId : null;
  await recordings.receive(paths, library.books.find((b) => b.id === open && b.kind === "book")?.id ?? null);
}
/** Links from Pocket and the like: the export is read, and its own dialog offers the choice. */
const linkImport = useLinkImportStore();
async function importLinks() {
  if (await linkImport.choose()) close();
}
function go(view: "find" | "audiobooks") {
  close();
  navigation.view = view;
}
function rescan() {
  close();
  navigation.view = "library";
  void library.rescan();
}
</script>

<template>
  <UiDialog label="Add to your library" @close="close">
    <div class="add-dialog">
      <div class="add-dialog__head">
        <h1 class="add-dialog__title">Add to your library</h1>
        <UiButton size="lg" icon class="add-dialog__close" aria-label="Close" @click="close">
          <X :size="18" aria-hidden="true" />
        </UiButton>
      </div>
      <div
        v-if="!subscribing && library.collections.length"
        class="add-dialog__into"
        role="group"
        aria-label="Collections to add to"
      >
        <span class="add-dialog__into-label">Into</span>
        <button
          v-for="collection in library.collections"
          :key="collection.id"
          type="button"
          class="add-dialog__chip"
          :class="{ 'add-dialog__chip--on': into.includes(collection.id) }"
          :aria-pressed="into.includes(collection.id)"
          @click="toggleInto(collection.id)"
        >
          {{ collection.name }}
        </button>
      </div>
      <form class="add-dialog__section" novalidate @submit.prevent="submitLink">
        <label for="add-link" class="add-dialog__label">{{
          subscribing ? "A feed, or a site that has one" : "A link to an article or a feed"
        }}</label>
        <div class="add-dialog__row">
          <input
            id="add-link"
            ref="field"
            v-model="link"
            type="url"
            class="add-dialog__field"
            placeholder="https://"
            autocomplete="off"
            spellcheck="false"
            aria-describedby="add-link-note"
            :aria-invalid="linkError ? 'true' : undefined"
            @input="linkError = ''"
          />
          <UiButton size="lg" icon class="add-dialog__button" aria-label="Paste link" title="Paste link" @click="paste">
            <ClipboardPaste :size="16" aria-hidden="true" />
          </UiButton>
          <UiButton
            variant="accent"
            size="lg"
            type="submit"
            class="add-dialog__button add-dialog__button--accent"
            :disabled="saving"
          >
            {{ subscribing ? "Subscribe" : "Save article" }}
          </UiButton>
        </div>
        <p v-if="linkError" class="add-dialog__error" role="alert">{{ linkError }}</p>
        <p id="add-link-note" class="add-dialog__hint add-dialog__hint--wide">
          {{
            subscribing
              ? "Feeds are checked in the background, and new entries arrive in Feeds."
              : "The page is fetched once, and its readable text is kept on this computer. A feed's address subscribes to it instead."
          }}
        </p>
        <ul v-if="choices.length" class="add-dialog__choices" aria-label="Feeds on this site">
          <li v-for="feed in choices" :key="feed.url" class="add-dialog__choice">
            <Rss :size="18" aria-hidden="true" />
            <span class="add-dialog__choice-text">
              <b class="add-dialog__choice-title">{{ feed.title ?? feed.url }}</b>
              <span class="add-dialog__choice-url">{{ feed.url }}</span>
            </span>
            <UiButton
              variant="primary"
              size="lg"
              class="add-dialog__button add-dialog__button--main"
              :disabled="saving || feed.subscribed"
              @click="choose(feed)"
            >
              {{ feed.subscribed ? "Subscribed" : "Subscribe" }}
            </UiButton>
          </li>
        </ul>
      </form>
      <section class="add-dialog__section" aria-labelledby="add-files">
        <h2 id="add-files" class="add-dialog__label">Files on this computer</h2>
        <div class="add-dialog__drop">
          <b class="add-dialog__drop-title">Drop books or audiobooks anywhere on this window</b>
          <p class="add-dialog__hint">
            EPUB or PDF for text, or a saved email (.eml) to read as an article. M4B, MP3, FLAC or Ogg for audio: an
            audiobook is paired with its book, or added on its own.
          </p>
          <div class="add-dialog__buttons">
            <UiButton
              variant="primary"
              size="lg"
              class="add-dialog__button add-dialog__button--main"
              @click="chooseBooks"
            >
              <FilePlus :size="16" aria-hidden="true" /> Choose books…
            </UiButton>
            <UiButton size="lg" class="add-dialog__button" @click="chooseAudio(false)">
              <Headphones :size="16" aria-hidden="true" /> Choose an audiobook…
            </UiButton>
            <UiButton size="lg" class="add-dialog__button" @click="chooseAudio(true)">
              <FolderOpen :size="16" aria-hidden="true" /> Choose an audiobook folder…
            </UiButton>
          </div>
        </div>
      </section>
      <section class="add-dialog__section" aria-labelledby="add-apps">
        <h2 id="add-apps" class="add-dialog__label">From another app</h2>
        <p class="add-dialog__hint">
          An export from Pocket, Instapaper, Omnivore, Wallabag or Readwise Reader, your browser's bookmarks, or a list
          of links.
        </p>
        <p v-if="linkImport.error" class="add-dialog__error" role="alert">{{ linkImport.error }}</p>
        <div class="add-dialog__buttons add-dialog__buttons--start">
          <UiButton
            size="lg"
            class="add-dialog__button"
            aria-label="Import links from another app"
            @click="importLinks"
          >
            <Download :size="16" aria-hidden="true" /> Import from another app…
          </UiButton>
        </div>
      </section>
      <section class="add-dialog__section" aria-labelledby="add-find">
        <h2 id="add-find" class="add-dialog__label">Find on this computer</h2>
        <div class="add-dialog__buttons add-dialog__buttons--start">
          <UiButton size="lg" class="add-dialog__button" @click="go('find')">
            <FolderSearch :size="16" aria-hidden="true" /> Scan for books…
          </UiButton>
          <UiButton size="lg" class="add-dialog__button" @click="go('audiobooks')">
            <Headphones :size="16" aria-hidden="true" /> Find audiobooks…
          </UiButton>
          <UiButton size="lg" class="add-dialog__button" :disabled="library.loading" @click="rescan">
            <RefreshCw :size="16" aria-hidden="true" /> Rescan library folder
          </UiButton>
        </div>
      </section>
    </div>
  </UiDialog>
</template>

<style scoped>
.add-dialog {
  width: 560px;
  max-width: 100%;
  display: flex;
  flex-direction: column;
  gap: var(--space-11);
  padding: var(--space-14);
  overflow-y: auto;
}
.add-dialog__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.add-dialog__title {
  margin: 0;
  font-size: 22px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.add-dialog__into {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
}
.add-dialog__into-label {
  margin-right: var(--space-2);
  color: var(--dim);
  font-size: 13px;
  font-weight: 600;
}
.add-dialog__chip {
  height: var(--control-sm);
  padding: 0 var(--space-6);
  border-radius: var(--radius-pill);
  background: var(--wash);
  color: var(--dim);
  font-size: 13px;
  font-weight: 500;
}
.add-dialog__chip:hover:not(.add-dialog__chip--on) {
  color: var(--ink);
}
.add-dialog__chip--on {
  background: var(--ink);
  color: var(--ground);
  font-weight: 600;
}
.add-dialog__section {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}
.add-dialog__label {
  margin: 0;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--dim);
}
.add-dialog__drop {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-6);
  padding: var(--space-14) var(--space-10);
  border: 1.5px dashed var(--faint);
  border-radius: var(--radius-lg);
  background: var(--ground);
  text-align: center;
}
.add-dialog__drop-title {
  font-size: 16px;
}
.add-dialog__hint {
  margin: 0;
  max-width: 380px;
  font-size: 13px;
  line-height: 1.45;
  color: var(--dim);
}
.add-dialog__buttons {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: var(--space-4);
}
.add-dialog__buttons--start {
  justify-content: flex-start;
}
.add-dialog__row {
  display: flex;
  gap: var(--space-4);
}
.add-dialog__field {
  flex: 1;
  min-width: 0;
  height: 48px;
  padding: 0 var(--space-8);
  border-radius: var(--radius-xl);
  border: 1px solid var(--line);
  background: var(--page);
  color: var(--ink);
  font: inherit;
}
.add-dialog__field[aria-invalid="true"] {
  border-color: var(--accent);
}
.add-dialog__choices {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}
.add-dialog__choice {
  display: flex;
  align-items: center;
  gap: var(--space-6);
  padding: var(--space-5) var(--space-5) var(--space-5) var(--space-7);
  border-radius: var(--radius-md);
  background: var(--wash);
}
.add-dialog__choice-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}
.add-dialog__choice-url {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  color: var(--dim);
}
.add-dialog__error {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--accent);
}
.add-dialog__hint--wide {
  max-width: none;
}
</style>
