<script setup lang="ts">
import { computed } from "vue";
import { Settings } from "@lucide/vue";
import { useLibraryStore } from "../../stores/library";
import { useNavigationStore } from "../../stores/navigation";
import { usePlayerStore } from "../../stores/player";
import { ACTION_ICONS, ACTION_LABELS, describePreparation, preparationStatus, runPrepareAction, type PrepareAction } from "../../preparation";
import { useRecordingsStore } from "../../stores/recordings";
import { RECORDING_ACTION_ICONS, RECORDING_ACTION_LABELS, recordingStatus, type RecordingAction } from "../../recording";
import { useUiStore } from "../../stores/ui";

/** What the speech engine is working on: audio for the book playing now, then the books
 *  queued for whole-book preparation, in the order they will be made; and the recordings being
 *  synced with their books. */
const emit = defineEmits<{ close: [] }>();
const library = useLibraryStore(), navigation = useNavigationStore(), player = usePlayerStore();

const title = (id: string | null) => library.books.find(b => b.id === id)?.title ?? "Unknown book";
const playing = computed(() => {
  const g = player.generation;
  return g.bookId && (g.pending > 0 || g.generatingSegmentId || g.error) ? g : null;
});
const queue = computed(() => library.books
  .filter(b => b.prepare && b.prepare.state !== "done")
  .sort((a, b) => (a.prepare!.position ?? Infinity) - (b.prepare!.position ?? Infinity))
  .map(book => ({ book, status: preparationStatus(book.prepare) })));
/** Short tooltips; the accessible names carry the full action and title. */
const ACTION_TITLES: Record<PrepareAction, string> = { prepare: "Prepare", start: "Start", pause: "Pause", resume: "Resume", cancel: "Cancel (keeps the audio made so far)" };

const plural = (n: number, word: string) => `${n.toLocaleString()} ${word}${n === 1 ? "" : "s"}`;
function ahead(ms: number) {
  const s = Math.round(ms / 1000);
  return s < 60 ? `${s} s` : `${Math.round(s / 60)} min`;
}

function openBook(id: string) { emit("close"); navigation.openBook(id); }
const recordings = useRecordingsStore(), ui = useUiStore();
void recordings.start();
/** Recordings still syncing or needing a decision, in line order. */
const syncing = computed(() => recordings.syncing
  .map(recording => ({ recording, status: recordingStatus(recording), title: title(recording.bookId) }))
  .sort((a, b) => (a.recording.position ?? Infinity) - (b.recording.position ?? Infinity)));
/** The queue offers the quick actions; the rest are on the book's page. */
const QUEUE_ACTIONS: RecordingAction[] = ["pause", "resume", "retry"];
async function syncAction(bookId: string, action: RecordingAction) {
  try { if (action === "pause" || action === "resume" || action === "retry") await recordings.act(bookId, action); }
  catch (e) { ui.toast(String(e), "error"); }
}
function settings() { emit("close"); navigation.openSettings("engine"); }
</script>

<template>
  <div class="engine-queue">
    <header class="engine-queue__head">
      <p class="engine-queue__engine"><i class="engine-queue__dot" :class="`engine-queue__dot--${player.engine.state}`" aria-hidden="true" />{{ player.engine.profileName }} <span class="engine-queue__dim">{{ player.engine.state }}</span></p>
      <p v-if="player.engine.detail" class="engine-queue__detail">{{ player.engine.detail }}</p>
    </header>

    <section v-if="playing" class="engine-queue__section" aria-label="Playing now">
      <h3 class="engine-queue__heading">Playing now</h3>
      <p class="engine-queue__title">{{ title(playing.bookId) }}</p>
      <p class="engine-queue__meta">{{ playing.pending ? `${plural(playing.pending, "sentence")} to make` : "Making the next sentence" }} · {{ ahead(playing.bufferedAheadMs) }} ready ahead</p>
      <p v-if="playing.error" class="engine-queue__error">{{ playing.error }}</p>
    </section>

    <section class="engine-queue__section">
      <h3 class="engine-queue__heading">Preparing books</h3>
      <ol v-if="queue.length" class="engine-queue__list" aria-label="Preparation queue">
        <li v-for="{ book, status } in queue" :key="book.id" class="engine-queue__row" :class="`engine-queue__row--${status.state}`">
          <button type="button" class="engine-queue__book" :title="`Open ${book.title}`" @click="openBook(book.id)">{{ book.title }}</button>
          <span class="engine-queue__actions">
            <button v-for="action in status.actions" :key="action" type="button" class="engine-queue__icon" :aria-label="`${ACTION_LABELS[action]} ${book.title}`" :title="ACTION_TITLES[action]" @click="runPrepareAction(book.id, action)"><component :is="ACTION_ICONS[action]" :size="13" /></button>
          </span>
          <span class="engine-queue__track" aria-hidden="true"><i :style="{ width: `${status.percent}%` }" /></span>
          <span class="engine-queue__meta">{{ describePreparation(status) }}<template v-if="status.state === 'waiting'"> · starts when you press Start or play a book</template></span>
        </li>
      </ol>
      <p v-else class="engine-queue__empty">Nothing is waiting. “Prepare whole book” on a book’s page makes all of its audio ahead of time.</p>
    </section>

    <section v-if="syncing.length" class="engine-queue__section">
      <h3 class="engine-queue__heading">Syncing recordings</h3>
      <ol class="engine-queue__list" aria-label="Recordings being synced">
        <li v-for="{ recording, status, title: name } in syncing" :key="recording.bookId" class="engine-queue__row" :class="`engine-queue__row--${status.stage}`">
          <button type="button" class="engine-queue__book" :title="`Open ${name}`" @click="openBook(recording.bookId)">{{ name }}</button>
          <span class="engine-queue__actions">
            <button v-for="action in status.actions.filter(a => QUEUE_ACTIONS.includes(a))" :key="action" type="button" class="engine-queue__icon" :aria-label="`${RECORDING_ACTION_LABELS[action]} ${name}`" :title="RECORDING_ACTION_LABELS[action]" @click="syncAction(recording.bookId, action)"><component :is="RECORDING_ACTION_ICONS[action]" :size="13" /></button>
          </span>
          <span class="engine-queue__track" aria-hidden="true"><i :style="{ width: `${status.percent ?? (status.working ? 100 : 0)}%` }" /></span>
          <span class="engine-queue__meta">{{ status.label }}<template v-if="status.stage === 'transcribing' && status.detail"> · {{ status.detail }}</template><template v-else-if="status.stage === 'waiting' && status.detail"> · {{ status.detail }}</template></span>
        </li>
      </ol>
    </section>

    <footer class="engine-queue__foot">
      <button type="button" class="engine-queue__link" aria-label="Speech engine settings" @click="settings"><Settings :size="14" /> Speech engine settings</button>
    </footer>
  </div>
</template>

<style scoped>
.engine-queue { display: grid; gap: 14px; }
.engine-queue__head { display: grid; gap: 3px; }
.engine-queue__engine { display: flex; align-items: center; gap: 8px; font-weight: 700; font-size: 13.5px; }
.engine-queue__dim { color: var(--dim); font-weight: 600; }
.engine-queue__detail { font-size: 12px; color: var(--dim); font-weight: 500; }
.engine-queue__dot { width: 8px; height: 8px; border-radius: 50%; background: var(--faint); flex-shrink: 0; }
.engine-queue__dot--ready { background: var(--ok); }
.engine-queue__dot--busy, .engine-queue__dot--error { background: var(--accent); }
.engine-queue__dot--loading { background: var(--warn); }
.engine-queue__section { display: grid; gap: 6px; padding-top: 12px; border-top: 1px solid var(--line); }
.engine-queue__heading { font-size: 11px; font-weight: 700; letter-spacing: .06em; text-transform: uppercase; color: var(--faint); }
.engine-queue__title { font-weight: 700; font-size: 13px; }
.engine-queue__meta { font-size: 12px; font-weight: 500; color: var(--dim); font-variant-numeric: tabular-nums; }
.engine-queue__error { font-size: 12px; font-weight: 600; color: var(--accent); }
.engine-queue__empty { font-size: 12.5px; font-weight: 500; color: var(--dim); line-height: 1.45; }
.engine-queue__list { display: grid; gap: 4px; list-style: none; max-height: 300px; overflow-y: auto; overscroll-behavior: contain; margin: 0 -6px; padding: 0; }
.engine-queue__row { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; gap: 5px 8px; padding: 8px 6px; border-radius: 10px; }
.engine-queue__row:hover { background: var(--wash); }
.engine-queue__book { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: left; font-weight: 700; font-size: 13px; border-radius: 6px; }
.engine-queue__book:hover { text-decoration: underline; }
.engine-queue__actions { display: flex; gap: 2px; }
.engine-queue__icon { display: grid; place-items: center; width: 26px; height: 26px; border-radius: 50%; color: var(--dim); }
.engine-queue__icon:hover { background: var(--glass); color: var(--ink); }
.engine-queue__track { grid-column: 1 / -1; height: 4px; border-radius: 999px; background: var(--line); overflow: hidden; }
.engine-queue__track i { display: block; height: 100%; border-radius: inherit; background: var(--accent); transition: width var(--dur-2) var(--ease); }
.engine-queue__row--paused .engine-queue__track i, .engine-queue__row--queued .engine-queue__track i { background: var(--faint); }
.engine-queue__row .engine-queue__meta { grid-column: 1 / -1; }
.engine-queue__foot { display: flex; padding-top: 10px; border-top: 1px solid var(--line); }
.engine-queue__link { display: inline-flex; align-items: center; gap: 7px; padding: 6px 10px; margin-left: -10px; border-radius: 999px; font-weight: 600; font-size: 12.5px; color: var(--dim); }
.engine-queue__link:hover { background: var(--wash); color: var(--ink); }
</style>
