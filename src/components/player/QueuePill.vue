<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { ChevronDown, ChevronUp, GripVertical, ListMusic, X } from "@lucide/vue";
import { QUEUE_DRAG } from "@/drag";
import { usePopover } from "@/composables/usePopover";
import { useQueueStore, WHY_NOT } from "@/stores/queue";
import { useUiStore } from "@/stores/ui";

/** The queue's pill in the player: how many items wait, and a panel listing them in the order
 *  they play, to move up or down (or drag), take off, or clear. When the item playing ends, the
 *  first that can be listened to plays. */
const queue = useQueueStore(),
  ui = useUiStore();
onMounted(() => void queue.start().catch((e) => ui.toast(String(e), "error")));
const count = computed(() => queue.items.length);
const title = computed(() =>
  count.value ? `Queue: ${count.value} ${count.value === 1 ? "item" : "items"} to listen to next` : "Queue",
);

const open = ref(false),
  button = ref<HTMLElement>(),
  panel = ref<HTMLElement>();
const { style, above } = usePopover(button, panel, open, { align: "end", maxHeight: 420, matchWidth: false });
function close(focus = false) {
  open.value = false;
  if (focus) button.value?.focus();
}
function outside(event: PointerEvent) {
  const target = event.target as Node;
  if (!button.value?.contains(target) && !panel.value?.contains(target)) close();
}
watch(open, async (value) => {
  document.removeEventListener("pointerdown", outside, true);
  if (!value) return;
  // Why an item is passed over can change without the queue changing (a voice chosen).
  void queue.refresh().catch((e) => ui.toast(String(e), "error"));
  document.addEventListener("pointerdown", outside, true);
  await nextTick();
  panel.value?.focus();
});
onUnmounted(() => document.removeEventListener("pointerdown", outside, true));

async function act(action: () => Promise<unknown>) {
  try {
    await action();
  } catch (e) {
    ui.toast(String(e), "error");
  }
}
const move = (id: string, to: number) => act(() => queue.move(id, to));
const remove = (id: string) => act(() => queue.remove(id));
const clear = () => act(() => queue.clear());

/** The item being dragged, and the row it would land on. */
const dragging = ref<string | null>(null),
  over = ref<number | null>(null);
function dragged(event: DragEvent, id: string) {
  if (!event.dataTransfer) return;
  event.dataTransfer.setData(QUEUE_DRAG, id);
  event.dataTransfer.effectAllowed = "move";
  dragging.value = id;
}
function hover(event: DragEvent, index: number) {
  if (!event.dataTransfer?.types.includes(QUEUE_DRAG)) return;
  event.preventDefault();
  event.dataTransfer.dropEffect = "move";
  over.value = index;
}
function dropped(event: DragEvent, index: number) {
  const id = event.dataTransfer?.getData(QUEUE_DRAG);
  ended();
  if (id) void move(id, index);
}
function ended() {
  dragging.value = null;
  over.value = null;
}
</script>

<template>
  <button
    ref="button"
    type="button"
    class="queue-pill"
    :class="{ 'queue-pill--on': count }"
    aria-label="Queue"
    aria-haspopup="dialog"
    :aria-expanded="open"
    :title="title"
    @click="open ? close() : (open = true)"
  >
    <ListMusic :size="16" aria-hidden="true" /><span v-if="count" class="queue-pill__count">{{ count }}</span>
    <!-- Inside the button, so the pill is the component's one root (its parent styles it). -->
    <Teleport to="body">
      <div
        v-if="open"
        ref="panel"
        class="queue-pill__panel"
        :class="{ 'queue-pill__panel--above': above }"
        role="dialog"
        aria-label="Queue"
        tabindex="-1"
        :style="style"
        @keydown.esc.stop="close(true)"
      >
        <div class="queue-pill__head">
          <b class="queue-pill__heading">Up next</b>
          <button v-if="count" type="button" class="queue-pill__clear" @click="clear">Clear</button>
        </div>
        <p v-if="!count" class="queue-pill__empty">
          Nothing is queued. Choose Listen next or Add to queue in an item's menu, and it plays when what plays now
          ends.
        </p>
        <ol v-else class="queue-pill__list">
          <li
            v-for="(item, index) in queue.items"
            :key="item.bookId"
            class="queue-pill__item"
            :class="{
              'queue-pill__item--dragging': dragging === item.bookId,
              'queue-pill__item--over': over === index && dragging !== item.bookId,
            }"
            draggable="true"
            @dragstart="dragged($event, item.bookId)"
            @dragover="hover($event, index)"
            @dragleave="over = over === index ? null : over"
            @drop="dropped($event, index)"
            @dragend="ended"
          >
            <GripVertical class="queue-pill__grip" :size="14" aria-hidden="true" />
            <span class="queue-pill__text">
              <span class="queue-pill__title">{{ item.title }}</span>
              <small v-if="item.whyNot" class="queue-pill__why">{{ WHY_NOT[item.whyNot] }}: passed over for now</small>
            </span>
            <span class="queue-pill__actions">
              <button
                type="button"
                class="queue-pill__icon"
                :aria-label="`Move ${item.title} up`"
                title="Move up"
                :disabled="index === 0"
                @click="move(item.bookId, index - 1)"
              >
                <ChevronUp :size="15" aria-hidden="true" />
              </button>
              <button
                type="button"
                class="queue-pill__icon"
                :aria-label="`Move ${item.title} down`"
                title="Move down"
                :disabled="index === count - 1"
                @click="move(item.bookId, index + 1)"
              >
                <ChevronDown :size="15" aria-hidden="true" />
              </button>
              <button
                type="button"
                class="queue-pill__icon"
                :aria-label="`Take ${item.title} off the queue`"
                title="Take off the queue"
                @click="remove(item.bookId)"
              >
                <X :size="15" aria-hidden="true" />
              </button>
            </span>
          </li>
        </ol>
      </div>
    </Teleport>
  </button>
</template>

<style scoped>
.queue-pill {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  min-width: 34px;
  height: 34px;
  padding: 0 var(--space-5);
  border-radius: var(--radius-pill);
  background: var(--glass);
  color: var(--dim);
  font-weight: 700;
  font-size: 13px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
.queue-pill--on {
  color: var(--ink);
}
.queue-pill__count {
  color: var(--accent);
}
.queue-pill__panel {
  position: fixed;
  z-index: 40;
  display: grid;
  align-content: start;
  gap: var(--space-3);
  width: min(340px, calc(100vw - 16px));
  padding: var(--space-4);
  border-radius: var(--radius-md);
  background: var(--page);
  color: var(--ink);
  box-shadow:
    0 18px 40px -16px var(--shadow),
    0 0 0 1px var(--line);
  font: 600 13px/1.3 var(--font-ui);
  overflow-y: auto;
  transform-origin: top right;
  animation: queue-in var(--dur-1) var(--ease);
}
.queue-pill__panel:focus-visible {
  outline: none;
}
.queue-pill__panel--above {
  transform-origin: bottom right;
}
@keyframes queue-in {
  from {
    opacity: 0;
    transform: scale(0.97);
  }
}
.queue-pill__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  padding: var(--space-1) var(--space-2);
}
.queue-pill__heading {
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--dim);
}
.queue-pill__clear {
  padding: var(--space-1) var(--space-3);
  border-radius: var(--radius-sm);
  color: var(--dim);
  font-weight: 600;
}
.queue-pill__clear:hover {
  background: var(--wash);
  color: var(--ink);
}
.queue-pill__empty {
  padding: var(--space-2) var(--space-2) var(--space-3);
  font-weight: 500;
  color: var(--dim);
}
.queue-pill__list {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}
.queue-pill__item {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-2);
  border-radius: var(--radius-sm);
  box-shadow: inset 0 2px 0 transparent;
}
.queue-pill__item:hover {
  background: var(--wash);
}
.queue-pill__item--dragging {
  opacity: 0.5;
}
.queue-pill__item--over {
  box-shadow: inset 0 2px 0 var(--accent);
}
.queue-pill__grip {
  flex: none;
  color: var(--faint);
  cursor: grab;
}
.queue-pill__text {
  flex: 1;
  min-width: 0;
  display: grid;
  gap: var(--space-1);
}
.queue-pill__title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.queue-pill__why {
  font-size: 12px;
  font-weight: 500;
  color: var(--warn);
}
.queue-pill__actions {
  flex: none;
  display: flex;
  gap: var(--space-1);
}
.queue-pill__icon {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border-radius: 50%;
  color: var(--dim);
}
.queue-pill__icon:hover:not(:disabled) {
  background: var(--glass);
  color: var(--ink);
}
.queue-pill__icon:disabled {
  opacity: 0.35;
}
</style>
