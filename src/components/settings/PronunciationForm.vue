<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from "vue";
import { LoaderCircle, Square, Volume2 } from "@lucide/vue";
import { api } from "@/api";
import type { AffectedView, Pronunciation } from "@/api/types";
import UiButton from "@/components/ui/UiButton.vue";
import { useSample } from "@/composables/useVoicePreview";

/** One word said another way: what is written, how it is said, whether case must match. Hear it
 *  says the spoken form in the book's voice (`bookId`; null: the list for all books). In a
 *  book's list it says, before saving, how many sentences the word is in (the core counts). */
const props = withDefaults(
  defineProps<{
    bookId: string | null;
    written?: string;
    spoken?: string;
    matchCase?: boolean;
    /** The words for the save button. */
    saveLabel?: string;
  }>(),
  { written: "", spoken: "", matchCase: false, saveLabel: "Add" },
);
const emit = defineEmits<{ saved: [entry: Pronunciation] }>();
const written = ref(props.written),
  spoken = ref(props.spoken),
  matchCase = ref(props.matchCase);
const error = ref(""),
  saving = ref(false),
  affected = ref<AffectedView | null>(null);
const { state: hearing, error: hearError, play, stop } = useSample();
const ready = computed(() => written.value.trim() !== "" && spoken.value.trim() !== "");

/** Counts again a moment after the words stop changing; an older answer is dropped. */
let asked = 0,
  wait: ReturnType<typeof setTimeout> | undefined;
watch(
  [written, spoken, matchCase],
  () => {
    clearTimeout(wait);
    asked += 1;
    affected.value = null;
    const bookId = props.bookId;
    if (!bookId || !ready.value) return;
    const id = asked;
    const words = { bookId, written: written.value, spoken: spoken.value, matchCase: matchCase.value };
    wait = setTimeout(async () => {
      try {
        const answer = await api.pronunciationAffected(words);
        if (id === asked) affected.value = answer;
      } catch {
        if (id === asked) affected.value = null;
      }
    }, 300);
  },
  { immediate: true },
);
onUnmounted(() => clearTimeout(wait));

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
const reach = computed(() => {
  const a = affected.value;
  if (!a) return "";
  if (!a.sentences) return "Nothing in this book is said differently.";
  const made = a.cached ? `; ${plural(a.cached, "is", "are")} made already and will be made again` : "";
  return `Said differently in ${plural(a.sentences, "sentence", "sentences")}${made}.`;
});

function hear() {
  const bookId = props.bookId,
    said = spoken.value;
  void play(`${bookId}:${said}`, () => api.previewPronunciation({ bookId, spoken: said }));
}
async function save() {
  if (!ready.value || saving.value) return;
  error.value = "";
  saving.value = true;
  try {
    const entry = await api.savePronunciation({
      bookId: props.bookId,
      written: written.value,
      spoken: spoken.value,
      matchCase: matchCase.value,
    });
    emit("saved", entry);
  } catch (e) {
    error.value = String(e);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <form class="pronunciation-form" @submit.prevent="save">
    <div class="pronunciation-form__words">
      <label class="pronunciation-form__field"
        ><span>Written</span
        ><input
          v-model="written"
          type="text"
          aria-label="Written word"
          placeholder="Hermione"
          maxlength="80"
          spellcheck="false"
      /></label>
      <label class="pronunciation-form__field"
        ><span>Said as</span
        ><input
          v-model="spoken"
          type="text"
          aria-label="Said as"
          placeholder="her-MY-oh-nee"
          maxlength="120"
          spellcheck="false"
      /></label>
    </div>
    <label class="pronunciation-form__check"
      ><input v-model="matchCase" type="checkbox" aria-label="Match case" /> Only with this exact case</label
    >
    <p v-if="reach" class="pronunciation-form__reach" role="status">{{ reach }}</p>
    <div class="pronunciation-form__actions">
      <UiButton v-if="hearing === 'idle'" size="sm" aria-label="Hear it" :disabled="!spoken.trim()" @click="hear">
        <Volume2 :size="15" /> Hear it
      </UiButton>
      <UiButton v-else variant="primary" size="sm" aria-label="Stop hearing it" @click="stop">
        <LoaderCircle v-if="hearing === 'loading'" :size="15" class="pronunciation-form__spin" /><Square
          v-else
          :size="13"
          fill="currentColor"
        />
        {{ hearing === "loading" ? "Preparing…" : "Stop" }}
      </UiButton>
      <UiButton variant="accent" size="sm" type="submit" :disabled="!ready || saving">{{ saveLabel }}</UiButton>
    </div>
    <p v-if="error || hearError" class="pronunciation-form__error" role="alert">
      {{ error || `Could not say it: ${hearError}` }}
    </p>
  </form>
</template>

<style scoped>
.pronunciation-form {
  display: grid;
  gap: var(--space-5);
}
.pronunciation-form__words {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--space-5);
}
.pronunciation-form__field {
  display: grid;
  gap: var(--space-2);
  font-size: 12px;
  color: var(--dim);
}
.pronunciation-form__field input {
  min-width: 0;
  color: var(--ink);
  font-size: 13px;
}
.pronunciation-form__check {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  font-size: 13px;
}
.pronunciation-form__reach {
  margin: 0;
  font-size: 12px;
  color: var(--dim);
}
.pronunciation-form__actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-4);
}
.pronunciation-form__spin {
  animation: pronunciation-form-spin 1s linear infinite;
}
@keyframes pronunciation-form-spin {
  to {
    transform: rotate(1turn);
  }
}
.pronunciation-form__error {
  margin: 0;
  font-size: 12px;
  font-weight: 600;
  color: var(--accent);
  overflow-wrap: anywhere;
}
@media (max-width: 420px) {
  .pronunciation-form__words {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
