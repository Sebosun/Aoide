<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { ImagePlus, RotateCcw, X } from "@lucide/vue";
import { api } from "@/api";
import type { DetailField, DetailsPatch } from "@/api/contract";
import UiButton from "@/components/ui/UiButton.vue";
import UiDialog from "@/components/ui/UiDialog.vue";
import { useLibraryStore } from "@/stores/library";
import { useUiStore } from "@/stores/ui";

/** An item's title, authors, publication date, series and cover, corrected by hand. The file is never
 *  changed: what the core keeps is the correction, and each corrected field can go back to the
 *  file's at once. Typed changes are saved together; a cover is used as soon as it is chosen. */
const props = defineProps<{ bookId: string }>();
const emit = defineEmits<{ close: []; changed: [] }>();
const library = useLibraryStore(),
  ui = useUiStore();
const summary = computed(() => library.books.find((b) => b.id === props.bookId) ?? null);
const edited = (field: DetailField) => summary.value?.edited.includes(field) ?? false;

const title = ref(""),
  authors = ref(""),
  published = ref(""),
  series = ref(""),
  position = ref("");
const fields = { title, authors, published, series, position };
type Typed = keyof typeof fields;
/** What the fields held when the dialog opened (or a field went back to the file's). */
const shown: Record<Typed, string> = { title: "", authors: "", published: "", series: "", position: "" };
async function fill(names: Typed[]) {
  const book = summary.value;
  const date = names.includes("published")
    ? ((await api.bookDetails({ bookId: props.bookId }).catch(() => null))?.published ?? "")
    : "";
  const values: Record<Typed, string> = {
    title: book?.title ?? "",
    authors: (book?.authors ?? []).join("\n"),
    published: date,
    series: book?.series?.name ?? "",
    position: book?.series?.position == null ? "" : String(book.series.position),
  };
  for (const name of names) {
    shown[name] = values[name];
    fields[name].value = values[name];
  }
}
onMounted(() => fill(["title", "authors", "published", "series", "position"]));

const busy = ref(false);
async function send(patch: DetailsPatch) {
  busy.value = true;
  try {
    await api.editDetails({ bookId: props.bookId, patch });
    await library.refresh();
    emit("changed");
    return true;
  } catch (e) {
    ui.toast(String(e).replace(/^Error: /, ""), "error");
    return false;
  } finally {
    busy.value = false;
  }
}
/** Saves the typed changes (only the fields that changed) and closes. */
async function save() {
  const patch: DetailsPatch = {};
  if (title.value.trim() !== shown.title.trim()) patch.title = title.value.trim();
  if (authors.value.trim() !== shown.authors.trim())
    patch.authors = authors.value
      .split("\n")
      .map((a) => a.trim())
      .filter(Boolean);
  if (published.value.trim() !== shown.published.trim()) patch.published = published.value.trim() || null;
  if (series.value.trim() !== shown.series.trim()) patch.series = series.value.trim();
  if (position.value.trim() !== shown.position.trim()) {
    const place = position.value.trim() ? Number(position.value.trim().replace(",", ".")) : null;
    if (place !== null && !Number.isFinite(place)) {
      ui.toast("A place in a series is a number, such as 2 or 2.5.", "error");
      return;
    }
    patch.seriesPosition = place;
  }
  if (Object.keys(patch).length && !(await send(patch))) return;
  emit("close");
}
/** Sets one field back to the file's, at once. */
async function useFiles(field: DetailField) {
  const patch: DetailsPatch = field === "series" ? { series: null, seriesPosition: null } : { [field]: null };
  if (!(await send(patch))) return;
  if (field === "series") await fill(["series", "position"]);
  else if (field !== "cover") await fill([field]);
}
async function chooseCover() {
  const path = await api.chooseImage().catch(() => null);
  if (path) await send({ cover: path });
}
</script>

<template>
  <UiDialog :label="`Details of ${summary?.title ?? ''}`" initial-focus="input" @close="emit('close')">
    <form class="details-editor" @submit.prevent="save">
      <header class="details-editor__head">
        <div class="details-editor__titles">
          <h1 class="details-editor__title">Edit details</h1>
          <span class="details-editor__note">The file is not changed.</span>
        </div>
        <UiButton variant="glass" size="sm" icon aria-label="Close details" @click="emit('close')">
          <X :size="17" />
        </UiButton>
      </header>
      <label class="details-editor__field">
        <span class="details-editor__label">Title</span>
        <input v-model="title" type="text" aria-label="Title" maxlength="300" spellcheck="false" />
      </label>
      <button
        v-if="edited('title')"
        type="button"
        class="details-editor__reset"
        aria-label="Use the file's title"
        :disabled="busy"
        @click="useFiles('title')"
      >
        <RotateCcw :size="13" aria-hidden="true" /> Use the file's
      </button>
      <label class="details-editor__field">
        <span class="details-editor__label">Authors</span>
        <textarea v-model="authors" rows="3" aria-label="Authors, one per line" spellcheck="false" />
        <span class="details-editor__hint">One per line.</span>
      </label>
      <button
        v-if="edited('authors')"
        type="button"
        class="details-editor__reset"
        aria-label="Use the file's authors"
        :disabled="busy"
        @click="useFiles('authors')"
      >
        <RotateCcw :size="13" aria-hidden="true" /> Use the file's
      </button>
      <label class="details-editor__field">
        <span class="details-editor__label">Published</span>
        <input
          v-model="published"
          type="text"
          aria-label="Published"
          maxlength="40"
          placeholder="A year, such as 1998"
        />
      </label>
      <button
        v-if="edited('published')"
        type="button"
        class="details-editor__reset"
        aria-label="Use the file's date"
        :disabled="busy"
        @click="useFiles('published')"
      >
        <RotateCcw :size="13" aria-hidden="true" /> Use the looked-up date
      </button>
      <div class="details-editor__series">
        <label class="details-editor__field">
          <span class="details-editor__label">Series</span>
          <input v-model="series" type="text" aria-label="Series" maxlength="300" spellcheck="false" />
        </label>
        <label class="details-editor__field">
          <span class="details-editor__label">Number</span>
          <input v-model="position" type="text" inputmode="decimal" aria-label="Number in series" maxlength="12" />
        </label>
      </div>
      <button
        v-if="edited('series')"
        type="button"
        class="details-editor__reset"
        aria-label="Use the file's series"
        :disabled="busy"
        @click="useFiles('series')"
      >
        <RotateCcw :size="13" aria-hidden="true" /> Use the file's
      </button>
      <div class="details-editor__field">
        <span class="details-editor__label">Cover</span>
        <div class="details-editor__cover">
          <UiButton size="sm" aria-label="Choose a cover picture" :disabled="busy" @click="chooseCover">
            <ImagePlus :size="14" aria-hidden="true" /> Choose a picture…
          </UiButton>
          <UiButton
            v-if="edited('cover')"
            size="sm"
            variant="outline"
            aria-label="Use the file's cover"
            :disabled="busy"
            @click="useFiles('cover')"
          >
            <RotateCcw :size="14" aria-hidden="true" /> Use the file's
          </UiButton>
        </div>
      </div>
      <div class="details-editor__actions">
        <UiButton variant="outline" @click="emit('close')">Cancel</UiButton>
        <UiButton variant="primary" type="submit" :disabled="busy">Save</UiButton>
      </div>
    </form>
  </UiDialog>
</template>

<style scoped>
.details-editor {
  width: min(460px, calc(100vw - 32px));
  display: grid;
  gap: var(--space-5);
  padding: var(--space-12);
}
.details-editor__series {
  display: grid;
  grid-template-columns: 1fr 5.5em;
  gap: var(--space-4);
}
.details-editor__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-6);
  margin-bottom: var(--space-2);
}
.details-editor__titles {
  display: grid;
  gap: var(--space-1);
}
.details-editor__title {
  margin: 0;
  font-size: 20px;
  font-weight: 700;
}
.details-editor__note,
.details-editor__hint {
  color: var(--dim);
  font-size: 13px;
}
.details-editor__field {
  display: grid;
  gap: var(--space-3);
}
.details-editor__label {
  font-size: 13px;
  font-weight: 600;
  color: var(--dim);
}
.details-editor__field input,
.details-editor__field textarea {
  color: var(--ink);
  font: inherit;
  font-size: 14px;
  padding: var(--space-5) var(--space-6);
  resize: vertical;
}
.details-editor__reset {
  justify-self: start;
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  margin-top: calc(-1 * var(--space-2));
  padding: 0;
  font-size: 13px;
  color: var(--dim);
  text-decoration: underline;
}
.details-editor__reset:hover {
  color: var(--ink);
}
.details-editor__cover {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-4);
}
.details-editor__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-4);
  margin-top: var(--space-4);
}
</style>
