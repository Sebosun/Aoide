<script setup lang="ts">
import { onMounted, ref } from "vue";
import { LoaderCircle, Pencil, Volume2, X } from "@lucide/vue";
import { api } from "@/api";
import type { Pronunciation } from "@/api/types";
import UiButton from "@/components/ui/UiButton.vue";
import PronunciationForm from "@/components/settings/PronunciationForm.vue";
import { useSample } from "@/composables/useVoicePreview";
import { useLibraryStore } from "@/stores/library";

/** A pronunciation list: book `bookId`'s, or with null the one for all books. Each word can be
 *  heard as it is said, changed (it fills the form) and removed; the form adds one. */
const props = defineProps<{ bookId: string | null }>();
const library = useLibraryStore();
const entries = ref<Pronunciation[] | null>(null),
  error = ref(""),
  message = ref("");
/** The entry being changed, if one is; the form starts again (its key) when this changes. */
const changing = ref<Pronunciation | null>(null),
  formKey = ref(0);
const { state: hearing, play } = useSample();
const heard = ref<number | null>(null);

async function load() {
  try {
    entries.value = await api.pronunciations({ bookId: props.bookId });
  } catch (e) {
    error.value = String(e);
  }
}
onMounted(load);

function restart(entry: Pronunciation | null) {
  changing.value = entry;
  formKey.value += 1;
}
async function saved(entry: Pronunciation) {
  message.value = `Saved. Sentences with “${entry.written}” are made again when they are next needed.`;
  restart(null);
  await load();
  if (props.bookId) void library.refresh(); // A prepared book it changes is prepared no longer.
}
async function remove(entry: Pronunciation) {
  error.value = "";
  message.value = "";
  try {
    await api.deletePronunciation({ id: entry.id });
    if (changing.value?.id === entry.id) restart(null);
    await load();
    if (props.bookId) void library.refresh();
  } catch (e) {
    error.value = String(e);
  }
}
function hear(entry: Pronunciation) {
  heard.value = entry.id;
  const bookId = props.bookId;
  void play(`${bookId}:${entry.spoken}`, () => api.previewPronunciation({ bookId, spoken: entry.spoken }));
}
</script>

<template>
  <section
    class="pronunciations"
    :aria-label="bookId ? 'Pronunciations for this book' : 'Pronunciations for all books'"
  >
    <ul v-if="entries?.length" class="pronunciations__list">
      <li v-for="entry in entries" :key="entry.id" class="pronunciations__row">
        <span class="pronunciations__words"
          ><span class="pronunciations__written">{{ entry.written }}</span>
          <span class="pronunciations__arrow" aria-hidden="true">→</span>
          <span class="pronunciations__spoken">{{ entry.spoken }}</span><small
            v-if="entry.matchCase"
            class="pronunciations__case"
            title="Only with this exact case"
            >Aa</small
          ></span
        >
        <UiButton size="sm" icon :aria-label="`Hear ${entry.written}`" title="Hear it" @click="hear(entry)">
          <LoaderCircle v-if="heard === entry.id && hearing === 'loading'" :size="14" class="pronunciations__spin" />
          <Volume2 v-else :size="14" />
        </UiButton>
        <UiButton size="sm" icon :aria-label="`Change ${entry.written}`" title="Change" @click="restart(entry)">
          <Pencil :size="14" />
        </UiButton>
        <UiButton size="sm" icon danger :aria-label="`Remove ${entry.written}`" title="Remove" @click="remove(entry)">
          <X :size="14" />
        </UiButton>
      </li>
    </ul>
    <p v-else-if="entries" class="pronunciations__empty">No words yet.</p>
    <PronunciationForm
      :key="formKey"
      :book-id="bookId"
      :written="changing?.written"
      :spoken="changing?.spoken"
      :match-case="changing?.matchCase"
      :save-label="changing ? 'Save' : 'Add'"
      @saved="saved"
    />
    <p v-if="message" class="pronunciations__ok" role="status">{{ message }}</p>
    <p v-if="error" class="pronunciations__error" role="alert">{{ error }}</p>
  </section>
</template>

<style scoped>
.pronunciations {
  display: grid;
  gap: var(--space-6);
}
.pronunciations__list {
  display: grid;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}
.pronunciations__row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-2) var(--space-2) var(--space-6);
  border-radius: var(--radius-pill);
  background: var(--ground);
  font-size: 13px;
}
.pronunciations__words {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0 var(--space-3);
  overflow-wrap: anywhere;
}
.pronunciations__written {
  font-weight: 600;
}
.pronunciations__arrow {
  color: var(--dim);
}
.pronunciations__case {
  font-size: 11px;
  color: var(--dim);
}
.pronunciations__spin {
  animation: pronunciations-spin 1s linear infinite;
}
@keyframes pronunciations-spin {
  to {
    transform: rotate(1turn);
  }
}
.pronunciations__empty {
  margin: 0;
  font-size: 12px;
  color: var(--dim);
}
.pronunciations__ok,
.pronunciations__error {
  margin: 0;
  font-size: 12px;
  color: var(--accent);
  overflow-wrap: anywhere;
}
</style>
