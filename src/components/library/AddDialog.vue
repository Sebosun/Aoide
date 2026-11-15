<script setup lang="ts">
import { onMounted, ref } from "vue";
import { FilePlus, FolderOpen, FolderSearch, Headphones, RefreshCw, Rss, X } from "@lucide/vue";
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
/** The feeds a site names, to pick one when it names several. */
const choices = ref<DiscoveredFeed[]>([]);
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
    else await articles.add(valid);
  } catch (e) {
    linkError.value = String(e).replace(/^Error: /, "");
  } finally {
    saving.value = false;
  }
}
async function subscribe(url: string) {
  const result = await api.subscribe(url);
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
/** EPUBs join the library; Library opens to show them (and anything that could not be read). */
async function chooseBooks() {
  const paths = await api.chooseBooks().catch((e) => {
    library.error = String(e);
    return null;
  });
  if (!paths?.length) return;
  close();
  navigation.view = "library";
  await library.importFiles(paths);
}
/** An audiobook (files or a folder) is paired with a book or added on its own, as when dropped. */
async function chooseAudio(folder: boolean) {
  const paths = await api.chooseAudio(folder).catch((e) => {
    library.error = String(e);
    return null;
  });
  if (!paths?.length) return;
  close();
  const open = navigation.view === "item" ? navigation.bookId : null;
  await recordings.receive(paths, library.books.find((b) => b.id === open && b.kind === "book")?.id ?? null);
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
        <button type="button" class="add-dialog__close" aria-label="Close" @click="close">
          <X :size="18" aria-hidden="true" />
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
          <button type="submit" class="add-dialog__button add-dialog__button--accent" :disabled="saving">
            {{ subscribing ? "Subscribe" : "Save article" }}
          </button>
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
            <button
              type="button"
              class="add-dialog__button add-dialog__button--main"
              :disabled="saving || feed.subscribed"
              @click="choose(feed)"
            >
              {{ feed.subscribed ? "Subscribed" : "Subscribe" }}
            </button>
          </li>
        </ul>
      </form>
      <section class="add-dialog__section" aria-labelledby="add-files">
        <h2 id="add-files" class="add-dialog__label">Files on this computer</h2>
        <div class="add-dialog__drop">
          <b class="add-dialog__drop-title">Drop books or audiobooks anywhere on this window</b>
          <p class="add-dialog__hint">
            EPUB for text. M4B, MP3, FLAC or Ogg for audio: an audiobook is paired with its book, or added on its own.
          </p>
          <div class="add-dialog__buttons">
            <button type="button" class="add-dialog__button add-dialog__button--main" @click="chooseBooks">
              <FilePlus :size="16" aria-hidden="true" /> Choose books…
            </button>
            <button type="button" class="add-dialog__button" @click="chooseAudio(false)">
              <Headphones :size="16" aria-hidden="true" /> Choose an audiobook…
            </button>
            <button type="button" class="add-dialog__button" @click="chooseAudio(true)">
              <FolderOpen :size="16" aria-hidden="true" /> Choose an audiobook folder…
            </button>
          </div>
        </div>
      </section>
      <section class="add-dialog__section" aria-labelledby="add-find">
        <h2 id="add-find" class="add-dialog__label">Find on this computer</h2>
        <div class="add-dialog__buttons add-dialog__buttons--start">
          <button type="button" class="add-dialog__button" @click="go('find')">
            <FolderSearch :size="16" aria-hidden="true" /> Scan for books…
          </button>
          <button type="button" class="add-dialog__button" @click="go('audiobooks')">
            <Headphones :size="16" aria-hidden="true" /> Find audiobooks…
          </button>
          <button type="button" class="add-dialog__button" :disabled="library.loading" @click="rescan">
            <RefreshCw :size="16" aria-hidden="true" /> Rescan library folder
          </button>
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
  gap: 22px;
  padding: 28px;
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
.add-dialog__close {
  display: grid;
  place-items: center;
  width: 44px;
  height: 44px;
  border-radius: 50%;
  background: var(--ground);
}
.add-dialog__section {
  display: flex;
  flex-direction: column;
  gap: 8px;
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
  gap: 12px;
  padding: 28px 20px;
  border: 1.5px dashed var(--faint);
  border-radius: 18px;
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
  gap: 8px;
}
.add-dialog__buttons--start {
  justify-content: flex-start;
}
.add-dialog__button {
  height: 44px;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 0 18px;
  border-radius: 22px;
  background: var(--wash);
  font-weight: 600;
}
.add-dialog__row {
  display: flex;
  gap: 8px;
}
.add-dialog__field {
  flex: 1;
  min-width: 0;
  height: 48px;
  padding: 0 16px;
  border-radius: 24px;
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
  gap: 8px;
}
.add-dialog__choice {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 10px 10px 14px;
  border-radius: 14px;
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
.add-dialog__button--accent {
  height: 48px;
  padding: 0 22px;
  border-radius: 24px;
  background: var(--accent);
  color: var(--on-accent);
  font-weight: 700;
}
.add-dialog__button--main {
  background: var(--ink);
  color: var(--ground);
}
</style>
