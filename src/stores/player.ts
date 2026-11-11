import { defineStore } from "pinia";
import { ref } from "vue";
import { api } from "../api";
import { initialPlayer, initialGeneration } from "../api/mock";
import type { EngineStatusView, BookView } from "../api/types";

export const usePlayerStore = defineStore("player", () => {
  const state = ref(initialPlayer()), generation = ref(initialGeneration());
  const engine = ref<EngineStatusView>({ profileName: "Chatterbox", state: "offline", detail: null });
  const error = ref("");
  const playingBook = ref<BookView | null>(null);
  let metadataId: string | null = null;
  async function receiveBook(id: string | null) {
    if (metadataId === id) return;
    metadataId = id; playingBook.value = null;
    if (!id) return;
    try { const book = await api.bookOverview(id); if (state.value.bookId === id) playingBook.value = book; }
    catch (e) { error.value = String(e); }
  }
  let startup: Promise<void> | undefined;
  const unlisteners: (() => void)[] = [];
  async function action(fn: () => Promise<void>) { error.value = ""; try { await fn(); } catch (e) { error.value = String(e); } }
  function start() {
    return startup ??= (async () => {
      try {
        unlisteners.push(await api.onPlayerState(s => { state.value = s; void receiveBook(s.bookId); }));
        let engineEvent = false;
        unlisteners.push(await api.onEngineStatus(s => { engineEvent = true; engine.value = s; }));
        unlisteners.push(await api.onGenerationState(s => { generation.value = s; }));
        const status = await api.engineStatus(); if (!engineEvent) engine.value = status;
      } catch (e) { error.value = String(e); }
    })();
  }
  /** The level to return to when unmuting. */
  let heard = 1;
  function setVolume(volume: number) {
    if (volume > 0) heard = volume;
    state.value = { ...state.value, volume }; // Ahead of the next state event, so a dragged slider stays put.
    return action(() => api.setVolume(volume));
  }
  const toggleMute = () => setVolume(state.value.volume > 0 ? 0 : heard);
  function dispose() { unlisteners.splice(0).forEach(fn => fn()); startup = undefined; }
  return { state, generation, engine, error, playingBook, start, dispose,
    play: (id: string, chapter: number | null = null, segmentId: string | null = null) => action(() => api.play(id, chapter, segmentId)),
    skipChapter: (delta: number) => action(() => api.skipChapter(delta)),
    setSleepTimer: (minutes: number | null, endOfChapter: boolean) => action(() => api.setSleepTimer(minutes, endOfChapter)),
    pause: () => action(() => api.pause()), resume: () => action(() => api.resume()), stop: () => action(() => api.stop()), setSpeed: (speed: number) => action(() => api.setSpeed(speed)), setVolume, toggleMute,
  };
});
