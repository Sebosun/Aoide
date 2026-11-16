<script setup lang="ts">
import { ref, watch } from "vue";
import { FolderPlus, X } from "@lucide/vue";
import { api } from "@/api";
import UiButton from "@/components/ui/UiButton.vue";

/** The folders a search walks, as Find books and Find audiobooks let the user set them: folders
 *  to search, to skip, and hidden ones to include, each added with the system's folder chooser
 *  (no path is typed), with a switch for searching on its own (on start, or as books are added).
 *  Each change is saved at once with `save`; the form says "Saved." when it resolves and emits
 *  `error` when it throws. Collapsed to a summary of the folders searched. The default slot
 *  explains what the search reads. */
export interface Folders {
  roots: string[];
  exclude: string[];
  includeHidden: string[];
}
type List = keyof Folders;

const props = defineProps<{
  /** The stored folders, or null while they load. */
  folders: Folders | null;
  /** The stored switch. */
  auto: boolean;
  /** What the switch does ("Look for new books when Aoide starts"). */
  autoLabel: string;
  /** Read out for each list, naming what is searched for. */
  labels: { roots: string; exclude: string; includeHidden: string };
  save: (folders: Folders, auto: boolean) => Promise<void>;
}>();
const emit = defineEmits<{ error: [message: string] }>();

const lists = ref<Folders>({ roots: [], exclude: [], includeHidden: [] }),
  auto = ref(props.auto),
  saved = ref(false);
watch(
  () => props.folders,
  (folders) => {
    if (folders)
      lists.value = {
        roots: [...folders.roots],
        exclude: [...folders.exclude],
        includeHidden: [...folders.includeHidden],
      };
  },
  { immediate: true },
);
watch(
  () => props.auto,
  (on) => (auto.value = on),
);

const LISTS: { list: List; title: string; hint: string; add: string; chooser: string }[] = [
  {
    list: "roots",
    title: "Search",
    hint: "With none chosen, your home folder is searched.",
    add: "Add a folder to search",
    chooser: "Choose a folder to search",
  },
  {
    list: "exclude",
    title: "Skip",
    hint: "Folders inside those searched.",
    add: "Add a folder to skip",
    chooser: "Choose a folder to skip",
  },
  {
    list: "includeHidden",
    title: "Hidden folders to include",
    hint: "Hidden folders are skipped unless chosen here.",
    add: "Add a hidden folder to include",
    chooser: "Choose a hidden folder to include",
  },
];

async function store() {
  saved.value = false;
  try {
    const { roots, exclude, includeHidden } = lists.value;
    await props.save({ roots: [...roots], exclude: [...exclude], includeHidden: [...includeHidden] }, auto.value);
    saved.value = true;
  } catch (e) {
    emit("error", String(e));
  }
}
async function add(list: List, title: string) {
  let folder: string | null;
  try {
    folder = await api.chooseFolder({ title });
  } catch (e) {
    emit("error", String(e));
    return;
  }
  if (!folder || lists.value[list].includes(folder)) return;
  lists.value[list] = [...lists.value[list], folder];
  await store();
}
async function remove(list: List, folder: string) {
  lists.value[list] = lists.value[list].filter((f) => f !== folder);
  await store();
}
async function setAuto(on: boolean) {
  auto.value = on;
  await store();
}
</script>

<template>
  <details class="scan-folders">
    <summary class="scan-folders__summary">
      Folders · {{ folders?.roots.length ? folders.roots.join(", ") : "your home folder" }}
    </summary>
    <div class="scan-folders__form">
      <section v-for="l in LISTS" :key="l.list" class="scan-folders__group">
        <span class="scan-folders__title"
          >{{ l.title }}<small>{{ l.hint }}</small></span
        >
        <ul class="scan-folders__list" :aria-label="labels[l.list]">
          <li v-if="l.list === 'roots' && !lists.roots.length" class="scan-folders__folder scan-folders__folder--home">
            Your home folder
          </li>
          <li v-for="folder in lists[l.list]" :key="folder" class="scan-folders__folder">
            <span class="scan-folders__path" :title="folder">{{ folder }}</span>
            <UiButton
              size="sm"
              icon
              :aria-label="`Remove ${folder}`"
              :title="`Remove ${folder}`"
              @click="remove(l.list, folder)"
            >
              <X :size="14" aria-hidden="true" />
            </UiButton>
          </li>
          <li class="scan-folders__add">
            <UiButton size="sm" :aria-label="l.add" @click="add(l.list, l.chooser)">
              <FolderPlus :size="14" aria-hidden="true" /> Add folder…
            </UiButton>
          </li>
        </ul>
      </section>
      <label class="scan-folders__check"
        ><input
          :checked="auto"
          type="checkbox"
          :aria-label="autoLabel"
          @change="setAuto(($event.target as HTMLInputElement).checked)"
        />
        {{ autoLabel }}</label
      >
      <p class="scan-folders__hint"><slot /><span v-if="saved" class="scan-folders__ok" role="status"> Saved.</span></p>
    </div>
  </details>
</template>

<style scoped>
.scan-folders {
  margin: var(--space-4) 0 var(--space-12);
  padding: var(--space-8) var(--space-10);
  border-radius: var(--radius-xl);
  background: var(--page);
  font-size: 13px;
}
.scan-folders__summary {
  cursor: pointer;
  font-weight: 600;
  color: var(--dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.scan-folders__form {
  display: grid;
  gap: var(--space-8);
  margin-top: var(--space-7);
}
.scan-folders__group {
  display: grid;
  grid-template-columns: 170px minmax(0, 1fr);
  gap: var(--space-6);
  align-items: start;
}
.scan-folders__title {
  display: grid;
  gap: var(--space-1);
  font-weight: 600;
}
.scan-folders__title small {
  font-weight: 400;
  color: var(--faint);
}
.scan-folders__list {
  display: grid;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}
.scan-folders__folder {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  min-height: var(--control-sm);
  padding: 0 var(--space-2) 0 var(--space-5);
  border-radius: var(--radius-md);
  background: var(--wash);
}
.scan-folders__folder--home {
  color: var(--dim);
}
.scan-folders__path {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  direction: rtl;
  text-align: left;
}
.scan-folders__check {
  display: inline-flex;
  align-items: center;
  gap: var(--space-4);
}
.scan-folders__hint {
  margin: 0;
  font-size: 12px;
  color: var(--dim);
}
.scan-folders__ok {
  color: var(--ok);
  font-weight: 600;
}
@media (max-width: 560px) {
  .scan-folders__group {
    grid-template-columns: 1fr;
  }
}
</style>
