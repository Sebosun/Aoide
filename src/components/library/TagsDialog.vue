<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { Sparkles, X } from "@lucide/vue";
import { api } from "@/api";
import UiButton from "@/components/ui/UiButton.vue";
import UiDialog from "@/components/ui/UiDialog.vue";
import { useLibraryStore } from "@/stores/library";
import { useUiStore } from "@/stores/ui";

/** An item's tags, to remove or add to (each change saved at once). The tags in use are
 *  offered while typing; a tag removed here is not put back by the item's own subjects. With a
 *  tagging model set up (Settings › Library), it can be asked for tags too. */
const props = defineProps<{ bookId: string; title: string }>();
const emit = defineEmits<{ close: [] }>();
const library = useLibraryStore(),
  ui = useUiStore();
const tags = computed(() => library.books.find((b) => b.id === props.bookId)?.tags ?? []);
const typed = ref("");
const offered = computed(() =>
  library.allTags.filter((t) => !tags.value.some((o) => o.toLowerCase() === t.toLowerCase())),
);

/** A tagging model is set up. */
const model = ref(false),
  asking = ref(false);
onMounted(async () => {
  try {
    const tagging = (await api.appSettings()).tagging;
    model.value = tagging.enabled && Boolean(tagging.model);
  } catch {
    model.value = false;
  }
});
async function suggest() {
  asking.value = true;
  try {
    await api.tagWithModel({ bookId: props.bookId });
    await library.refresh();
  } catch (e) {
    ui.toast(String(e), "error");
  } finally {
    asking.value = false;
  }
}
async function save(next: string[]) {
  try {
    await library.setBookTags(props.bookId, next);
  } catch (e) {
    ui.toast(String(e), "error");
  }
}
async function add() {
  const tag = typed.value.trim();
  if (!tag) return;
  typed.value = "";
  if (!tags.value.some((t) => t.toLowerCase() === tag.toLowerCase())) await save([...tags.value, tag]);
}
const remove = (tag: string) => save(tags.value.filter((t) => t !== tag));
</script>

<template>
  <UiDialog :label="`Tags for ${title}`" @close="emit('close')">
    <div class="tags-editor">
      <header class="tags-editor__head">
        <div class="tags-editor__titles">
          <h1 class="tags-editor__title">Tags</h1>
          <span class="tags-editor__book" :title="title">{{ title }}</span>
        </div>
        <UiButton variant="glass" size="sm" icon aria-label="Close tags" @click="emit('close')">
          <X :size="17" />
        </UiButton>
      </header>
      <ul v-if="tags.length" class="tags-editor__list">
        <li v-for="tag in tags" :key="tag" class="tags-editor__tag">
          {{ tag }}
          <button
            type="button"
            class="tags-editor__remove"
            :aria-label="`Remove the tag ${tag}`"
            :title="`Remove the tag ${tag}`"
            @click="remove(tag)"
          >
            <X :size="12" aria-hidden="true" />
          </button>
        </li>
      </ul>
      <p v-else class="tags-editor__none">No tags yet.</p>
      <input
        v-model="typed"
        class="tags-editor__field"
        type="text"
        list="tags-editor-offered"
        aria-label="Add a tag"
        placeholder="Add a tag and press Enter"
        @keydown.enter.prevent="add"
      />
      <datalist id="tags-editor-offered">
        <option v-for="tag in offered" :key="tag" :value="tag" />
      </datalist>
      <div v-if="model">
        <UiButton size="sm" aria-label="Suggest tags with the model" :disabled="asking" @click="suggest">
          <Sparkles :size="14" aria-hidden="true" /> {{ asking ? "Asking the model…" : "Suggest tags" }}
        </UiButton>
      </div>
    </div>
  </UiDialog>
</template>

<style scoped>
.tags-editor {
  width: 440px;
  max-width: 100%;
  display: grid;
  gap: var(--space-7);
  padding: var(--space-12);
}
.tags-editor__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-6);
}
.tags-editor__titles {
  display: grid;
  gap: var(--space-1);
  min-width: 0;
}
.tags-editor__title {
  margin: 0;
  font-size: 20px;
  font-weight: 700;
}
.tags-editor__book {
  color: var(--dim);
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tags-editor__list {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  margin: 0;
  padding: 0;
  list-style: none;
}
.tags-editor__tag {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  height: var(--control-sm);
  padding: 0 var(--space-2) 0 var(--space-5);
  border-radius: var(--radius-pill);
  background: var(--wash);
  font-size: 13px;
}
.tags-editor__remove {
  display: grid;
  place-items: center;
  width: var(--space-10);
  height: var(--space-10);
  border-radius: var(--radius-pill);
  color: var(--dim);
}
.tags-editor__remove:hover {
  background: var(--hero);
  color: var(--ink);
}
.tags-editor__none {
  margin: 0;
  color: var(--dim);
  font-size: 13px;
}
.tags-editor__field {
  height: var(--control-md);
  padding: 0 var(--space-6);
  border-radius: var(--radius-md);
  background: var(--wash);
  color: var(--ink);
}
</style>
