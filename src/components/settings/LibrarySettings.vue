<script setup lang="ts">
import UiButton from "@/components/ui/UiButton.vue";
import { computed, onMounted, onUnmounted, ref } from "vue";
import { FileDown, FolderOpen, FolderSearch, FolderX, Inbox, Library, Sparkles } from "@lucide/vue";
import { api } from "@/api";
import type { CodeSpeech, Describe, InboxView, PictureSpeech, TableSpeech, TaggingSettings } from "@/api/types";
import UiSelect from "@/components/ui/UiSelect.vue";
import type { CloseBehaviour } from "@/api/contract";
import SettingsSection from "@/components/settings/SettingsSection.vue";
import SettingsField from "@/components/settings/SettingsField.vue";
import SettingsNote from "@/components/settings/SettingsNote.vue";
import { useAppSettings } from "@/composables/useAppSettings";
import { useLibraryStore } from "@/stores/library";
import { useNavigationStore } from "@/stores/navigation";

/** Where the books live, finding more, and online details. Toggles save at once. */
const library = useLibraryStore(),
  navigation = useNavigationStore();
const { settings, error, save } = useAppSettings();
onMounted(() => {
  void library.start();
});
const tagging = computed(() => settings.value?.tagging ?? null);
/** Saves a change to the tagging model's settings. */
/** What closing the window while working does (core/presence.rs). */
const CLOSING: { value: CloseBehaviour; label: string }[] = [
  { value: "ask", label: "Ask each time" },
  { value: "keepWhileWorking", label: "Keep working in the background" },
  { value: "keepAlways", label: "Always stay in the background" },
  { value: "quit", label: "Quit" },
];
/** What narration says for pictures, tables and code (book.rs `Describe`). */
const PICTURES: { value: PictureSpeech; label: string }[] = [
  { value: "off", label: "Nothing" },
  { value: "caption", label: "Their captions" },
  { value: "altAndCaption", label: "Their descriptions and captions" },
];
const TABLES: { value: TableSpeech; label: string }[] = [
  { value: "announce", label: "Say there is a table" },
  { value: "read", label: "Read them row by row" },
  { value: "skip", label: "Skip them" },
];
const CODE: { value: CodeSpeech; label: string }[] = [
  { value: "read", label: "Read it" },
  { value: "announce", label: "Say there is code" },
  { value: "skip", label: "Skip it" },
];
const saveDescribe = (change: Partial<Describe>) =>
  settings.value && save({ describe: { ...settings.value.describe, ...change } });
const saveTagging = (change: Partial<TaggingSettings>) =>
  tagging.value && save({ tagging: { ...tagging.value, ...change } });
const tagRun = ref(""),
  tagging_ = ref(false);
/** Asks the model for the tags of every item that has none. */
async function tagUntagged() {
  tagRun.value = "";
  tagging_.value = true;
  try {
    const run = await api.tagUntaggedWithModel();
    const items = (n: number) => `${n} ${n === 1 ? "item" : "items"}`;
    tagRun.value = `Tagged ${items(run.tagged)}${run.missed ? `; the model gave nothing for ${items(run.missed)}` : ""}.`;
  } catch (e) {
    tagRun.value = String(e);
  } finally {
    tagging_.value = false;
  }
}
const exported = ref("");
/** Writes every book's highlights and bookmarks into a chosen folder, a Markdown file each. */
async function exportAllNotes() {
  exported.value = "";
  try {
    const folder = await api.chooseNotesFolder();
    if (!folder) return;
    const books = await api.exportAllNotes({ folder });
    exported.value = books
      ? `Wrote the notes of ${books} ${books === 1 ? "book" : "books"} into ${folder}.`
      : "No book has highlights or bookmarks yet.";
  } catch (e) {
    exported.value = String(e);
  }
}
function findBooks() {
  navigation.closeSettings();
  navigation.view = "find";
}
/** The inbox folder as the core watches it; a folder it refuses is said beside it. */
const inbox = ref<InboxView | null>(null),
  inboxError = ref("");
let offInbox: (() => void) | undefined;
onMounted(async () => {
  offInbox = await api.onInboxState((view) => (inbox.value = view));
  try {
    inbox.value = await api.inboxStatus();
  } catch (e) {
    inboxError.value = String(e);
  }
});
onUnmounted(() => offInbox?.());
/** Saves the inbox folder (`null`: none), keeping a refusal beside the inbox. */
async function saveInbox(folder: string | null) {
  inboxError.value = "";
  if (!(await save({ inbox: folder }))) {
    inboxError.value = error.value;
    error.value = "";
  }
}
async function chooseInbox() {
  const folder = await api.chooseFolder({ title: "Choose the inbox folder" });
  if (folder) await saveInbox(folder);
}
</script>

<template>
  <div class="library-settings">
    <SettingsSection title="Library" :icon="Library">
      <SettingsField label="Library folder" as="div"
        ><code class="library-settings__path">{{ library.folder || "…" }}</code></SettingsField
      >
      <SettingsNote
        >Books you drop on the window or import are copied here. Aoide never changes the EPUB files.</SettingsNote
      >
      <div>
        <UiButton size="sm" class="library-settings__button" @click="findBooks">
          <FolderSearch :size="15" /> Find books on this computer…
        </UiButton>
      </div>
    </SettingsSection>
    <SettingsSection title="Inbox" :icon="Inbox">
      <div class="library-settings__inbox" role="group" aria-label="Inbox folder">
        <SettingsField label="Inbox folder" as="div"
          ><code class="library-settings__path">{{ inbox?.folder ?? "None" }}</code></SettingsField
        >
        <div class="library-settings__row">
          <UiButton size="sm" @click="chooseInbox"><FolderOpen :size="15" /> Choose a folder…</UiButton>
          <UiButton v-if="inbox?.folder" size="sm" @click="saveInbox(null)"
            ><FolderX :size="15" /> Stop watching</UiButton
          >
        </div>
        <SettingsNote v-if="inbox?.state === 'unavailable'" tone="warn" role="status"
          >The folder is not there; the drive it is on may be taken out. Aoide looks for it every 30 seconds and adds
          what is new once it is back.</SettingsNote
        >
        <p v-if="inboxError" class="library-settings__error" role="alert">{{ inboxError }}</p>
      </div>
      <SettingsNote
        >EPUBs and PDFs saved in this folder, such as Downloads or one a sync tool fills, are copied into the library
        within seconds, and so are those in it now. The files there are left as they are, and a book you removed is not
        added again.</SettingsNote
      >
    </SettingsSection>
    <SettingsSection title="Notes">
      <SettingsNote
        >Your highlights and bookmarks, with their notes, as one Markdown file per book. A book's own are under Notes in
        its book tools.</SettingsNote
      >
      <div>
        <UiButton size="sm" class="library-settings__button library-settings__notes" @click="exportAllNotes">
          <FileDown :size="15" /> Export all notes…
        </UiButton>
      </div>
      <SettingsNote v-if="exported" role="status">{{ exported }}</SettingsNote>
    </SettingsSection>
    <SettingsSection title="Search">
      <label v-if="settings" class="library-settings__check"
        ><input
          :checked="settings.searchText"
          type="checkbox"
          aria-label="Search inside books"
          @change="save({ searchText: ($event.target as HTMLInputElement).checked })"
        />
        Search inside books and articles</label
      >
      <SettingsNote
        >The library's search also finds passages in the text. Aoide keeps an index of the text for it, about as large
        as the text itself (Storage shows how much); it is built in the background, never while speech is being made.
        Turning this off deletes it.</SettingsNote
      >
    </SettingsSection>
    <SettingsSection title="Book details">
      <label v-if="settings" class="library-settings__check"
        ><input
          :checked="settings.metadataLookup"
          type="checkbox"
          aria-label="Look up book details online"
          @change="save({ metadataLookup: ($event.target as HTMLInputElement).checked })"
        />
        Look up book details on Open Library</label
      >
      <SettingsNote
        >Only the title, authors and ISBN are sent, and only when you ask for details. Nothing changes until you pick a
        match.</SettingsNote
      >
    </SettingsSection>
    <SettingsSection title="Tags" :icon="Sparkles" aria-label="Tagging with a model">
      <label v-if="tagging" class="library-settings__check"
        ><input
          :checked="tagging.enabled"
          type="checkbox"
          aria-label="Tag items with a model on this computer"
          @change="saveTagging({ enabled: ($event.target as HTMLInputElement).checked })"
        />
        Tag items with a language model on this computer</label
      >
      <template v-if="tagging?.enabled">
        <SettingsField label="Model server"
          ><input
            class="library-settings__field"
            :value="tagging.baseUrl"
            aria-label="Model server address"
            placeholder="http://127.0.0.1:11434"
            @change="saveTagging({ baseUrl: ($event.target as HTMLInputElement).value.trim() })"
        /></SettingsField>
        <SettingsField label="Model"
          ><input
            class="library-settings__field"
            :value="tagging.model"
            aria-label="Model to tag with"
            placeholder="qwen2.5:3b"
            @change="saveTagging({ model: ($event.target as HTMLInputElement).value.trim() })"
        /></SettingsField>
        <div class="library-settings__row">
          <UiButton
            size="sm"
            aria-label="Tag the items that have no tags"
            :disabled="tagging_ || !tagging.model"
            @click="tagUntagged"
          >
            <Sparkles :size="15" /> {{ tagging_ ? "Tagging…" : "Tag untagged items" }}
          </UiButton>
          <span v-if="tagRun" class="library-settings__result" role="status">{{ tagRun }}</span>
        </div>
      </template>
      <SettingsNote
        >An OpenAI-compatible server such as Ollama, llama.cpp or LM Studio, on this computer. Each item's title,
        authors and description (or its first pages) are sent to it; it answers with a few tags, the ones you use
        preferred. A tag you remove stays removed.</SettingsNote
      >
    </SettingsSection>
    <SettingsSection title="Archive">
      <label v-if="settings" class="library-settings__check"
        ><input
          :checked="settings.archiveFinishedArticles"
          type="checkbox"
          aria-label="Archive articles when I finish them"
          @change="save({ archiveFinishedArticles: ($event.target as HTMLInputElement).checked })"
        />
        Archive articles when I finish them</label
      >
      <SettingsNote
        >Otherwise a finished article offers <b>Archive</b> for a few seconds. Archived items leave Home and the
        library's lists and wait under Archive; nothing of them is deleted.</SettingsNote
      >
      <label v-if="settings" class="library-settings__check"
        ><input
          :checked="settings.keepArticlePictures"
          type="checkbox"
          aria-label="Keep an article's pictures"
          @change="save({ keepArticlePictures: ($event.target as HTMLInputElement).checked })"
        />
        Keep an article's pictures</label
      >
      <SettingsNote
        >Its pictures are fetched when an article is saved, so they show in the reader. That asks more of the site and
        of the places its pictures are kept, so it is off unless you turn it on.</SettingsNote
      >
    </SettingsSection>
    <SettingsSection title="Listening">
      <label v-if="settings" class="library-settings__check"
        ><input
          :checked="settings.stepBackOnResume"
          type="checkbox"
          aria-label="Step back a little when playing after a break"
          @change="save({ stepBackOnResume: ($event.target as HTMLInputElement).checked })"
        />
        Step back a little when playing after a break</label
      >
      <SettingsNote
        >After ten seconds or more, playback starts again from the start of the sentence; after five minutes, a sentence
        earlier; after an hour, two. An audiobook without text goes back 3, 10 or 20 seconds.</SettingsNote
      >
      <label v-if="settings" class="library-settings__check"
        ><input
          :checked="settings.shortenSilences"
          type="checkbox"
          aria-label="Shorten long silences in recordings"
          @change="save({ shortenSilences: ($event.target as HTMLInputElement).checked })"
        />
        Shorten long silences in audiobooks</label
      >
      <SettingsNote
        >Pauses longer than half a second are cut to half a second as they play, in audiobooks on their own and those
        paired with a book (unless the book says otherwise under Narration in its tools). The highlighted word, your
        place and bookmarks stay where they were. Applies the next time one starts playing.</SettingsNote
      >
      <template v-if="settings">
        <SettingsField label="Pictures" as="div"
          ><UiSelect
            :model-value="settings.describe.pictures"
            label="What narration says for pictures"
            :options="PICTURES"
            @update:model-value="(pictures: PictureSpeech) => saveDescribe({ pictures })"
        /></SettingsField>
        <SettingsField label="Tables" as="div"
          ><UiSelect
            :model-value="settings.describe.tables"
            label="What narration says for tables"
            :options="TABLES"
            @update:model-value="(tables: TableSpeech) => saveDescribe({ tables })"
        /></SettingsField>
        <SettingsField label="Code" as="div"
          ><UiSelect
            :model-value="settings.describe.code"
            label="What narration says for code"
            :options="CODE"
            @update:model-value="(code: CodeSpeech) => saveDescribe({ code })"
        /></SettingsField>
        <SettingsNote
          >A caption that is part of a book's text is always read. Saying a table "has 4 columns and 12 rows" keeps it
          from being read as a run of numbers. Changing these makes again only the audio of what they
          change.</SettingsNote
        >
      </template>
    </SettingsSection>
    <SettingsSection title="Preparing audio">
      <label v-if="settings" class="library-settings__check"
        ><input
          :checked="settings.resumePrepare"
          type="checkbox"
          aria-label="Continue preparing books when Aoide starts"
          @change="save({ resumePrepare: ($event.target as HTMLInputElement).checked })"
        />
        Continue preparing books when Aoide starts</label
      >
      <SettingsNote
        >When this is off, books still waiting stay in the queue and start again once you play something or prepare
        another book.</SettingsNote
      >
    </SettingsSection>
    <SettingsSection title="Closing the window">
      <fieldset v-if="settings" class="library-settings__choices">
        <legend class="visually-hidden">When the window is closed while Aoide is working</legend>
        <label v-for="choice in CLOSING" :key="choice.value" class="library-settings__check"
          ><input
            type="radio"
            name="close"
            :value="choice.value"
            :checked="settings.close === choice.value"
            :aria-label="choice.label"
            @change="save({ close: choice.value })"
          />
          {{ choice.label }}</label
        >
      </fieldset>
      <SettingsNote
        >While a book is being prepared, an audiobook synced or something plays, Aoide can carry on with the window
        closed. Its icon (where the desktop shows one) plays and pauses, and has <em>Quit when done</em>. Without an
        icon the window is minimised instead.</SettingsNote
      >
    </SettingsSection>
    <SettingsSection title="Notifications">
      <label v-if="settings" class="library-settings__check"
        ><input
          :checked="settings.notices"
          type="checkbox"
          aria-label="Notify when a long job ends while Aoide is not in front"
          @change="save({ notices: ($event.target as HTMLInputElement).checked })"
        />
        Notify when a long job ends while Aoide is not in front</label
      >
      <SettingsNote
        >A book prepared, a recording synced, an export written or books imported, after more than a minute; and at once
        when preparing stalls, a sync needs a look or fails, or an export fails. Click one to open its
        book.</SettingsNote
      >
    </SettingsSection>
    <p v-if="error" class="library-settings__error" role="alert">{{ error }}</p>
  </div>
</template>

<style scoped>
.library-settings {
  display: grid;
  gap: var(--space-11);
}
.library-settings__path {
  padding: var(--space-4) var(--space-6);
  border-radius: var(--radius-md);
  background: var(--ground);
  font-size: 12.5px;
  overflow-wrap: anywhere;
}
.library-settings__choices {
  display: grid;
  gap: var(--space-3);
  margin: 0;
  padding: 0;
  border: 0;
}
.library-settings__check {
  display: flex;
  align-items: center;
  gap: var(--space-5);
  font-size: 13px;
}
.library-settings__field {
  width: 100%;
  height: var(--control-sm);
  padding: 0 var(--space-5);
  border-radius: var(--radius-md);
  background: var(--ground);
  color: var(--ink);
  font-size: 13px;
}
.library-settings__inbox {
  display: grid;
  gap: var(--space-5);
}
.library-settings__row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-5);
}
.library-settings__result {
  font-size: 12.5px;
  color: var(--dim);
}
.library-settings__error {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--accent);
}
</style>
