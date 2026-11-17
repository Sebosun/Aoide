import { defineStore } from "pinia";
import { ref } from "vue";
import { api } from "@/api";
import type { QueueItemView, QueuePlace, WhyNot } from "@/api/types";

/** Why a queued item is passed over when its turn comes, as the queue says it. */
export const WHY_NOT: Record<WhyNot, string> = {
  fetching: "Still being fetched",
  notFetched: "Could not be fetched",
  missing: "Its file is missing",
  noVoice: "No voice to read it",
};

/** The queue: what plays when the item playing ends, as the core keeps it (core/queue.rs). */
export const useQueueStore = defineStore("queue", () => {
  const items = ref<QueueItemView[]>([]);
  let startup: Promise<void> | undefined;
  let unlisten: (() => void) | undefined;
  async function refresh() {
    items.value = (await api.queue()).items;
  }
  function start() {
    return (startup ??= (async () => {
      unlisten = await api.onQueueChanged(() => void refresh());
      await refresh();
    })());
  }
  /** Puts an item next (it plays when the one playing ends) or last. */
  const add = (bookId: string, place: QueuePlace) => api.queueAdd({ bookId, place });
  /** Puts a feed entry's item in the queue, leaving the entry unread. */
  const addEntry = (entryId: number, place: QueuePlace) => api.queueAddEntry({ entryId, place });
  const remove = (bookId: string) => api.queueRemove({ bookId });
  /** Moves a queued item to place `to` (0 is first); shown there at once. */
  async function move(bookId: string, to: number) {
    const list = items.value.filter((item) => item.bookId !== bookId);
    const moved = items.value.find((item) => item.bookId === bookId);
    if (!moved) return;
    list.splice(Math.max(0, Math.min(to, list.length)), 0, moved);
    items.value = list;
    await api.queueMove({ bookId, to });
  }
  const clear = () => api.queueClear();
  function dispose() {
    unlisten?.();
    unlisten = undefined;
    startup = undefined;
  }
  return { items, start, refresh, add, addEntry, remove, move, clear, dispose };
});
