import { defineStore } from "pinia";
import { ref } from "vue";
import { api } from "@/api";
import { initialPlayer, initialGeneration } from "@/api/initial";
import type { EngineStatusView, BookView } from "@/api/types";

export const usePlayerStore = defineStore("player", () => {
  const state = ref(initialPlayer()),
    generation = ref(initialGeneration());
  const engine = ref<EngineStatusView>({
    profileName: "Chatterbox",
    state: "offline",
    detail: null,
    wanted: false,
    server: null,
  });
  const error = ref("");
  const playingBook = ref<BookView | null>(null);
  let metadataId: string | null = null;
  async function receiveBook(id: string | null) {
    if (metadataId === id) return;
    metadataId = id;
    playingBook.value = null;
    if (!id) return;
    try {
      const book = await api.bookOverview({ bookId: id });
      if (state.value.bookId === id) playingBook.value = book;
    } catch (e) {
      error.value = String(e);
    }
  }
  let startup: Promise<void> | undefined;
  const unlisteners: (() => void)[] = [];
  async function action(fn: () => Promise<void>) {
    error.value = "";
    try {
      await fn();
    } catch (e) {
      error.value = String(e);
    }
  }
  function start() {
    return (startup ??= (async () => {
      try {
        unlisteners.push(
          await api.onPlayerState((s) => {
            state.value = s;
            void receiveBook(s.bookId);
          }),
        );
        let engineEvent = false;
        unlisteners.push(
          await api.onEngineStatus((s) => {
            engineEvent = true;
            engine.value = s;
          }),
        );
        unlisteners.push(
          await api.onGenerationState((s) => {
            generation.value = s;
          }),
        );
        const status = await api.engineStatus();
        if (!engineEvent) engine.value = status;
      } catch (e) {
        error.value = String(e);
      }
    })());
  }
  /** The level to return to when unmuting. */
  let heard = 1;
  function setVolume(volume: number) {
    if (volume > 0) heard = volume;
    state.value = { ...state.value, volume }; // Ahead of the next state event, so a dragged slider stays put.
    return action(() => api.setVolume({ volume }));
  }
  const toggleMute = () => setVolume(state.value.volume > 0 ? 0 : heard);
  function setSpeed(speed: number) {
    state.value = { ...state.value, speed }; // Ahead of the next state event, so quick key presses add up.
    return action(() => api.setSpeed({ speed }));
  }
  /** Gives the book playing its own speed (`on`: the speed it plays at), or takes it away. */
  function setBookSpeed(on: boolean) {
    const id = state.value.bookId;
    return id ? action(() => api.setBookSpeed({ bookId: id, on })) : Promise.resolve();
  }
  /** Rises each time the sleep timer's menu is asked for (the T key); the player's sleep pill
   *  opens on it. */
  const sleepMenu = ref(0);
  const askSleep = () => sleepMenu.value++;
  function dispose() {
    unlisteners.splice(0).forEach((fn) => fn());
    startup = undefined;
  }
  return {
    state,
    generation,
    engine,
    error,
    playingBook,
    start,
    dispose,
    play: (id: string, chapter: number | null = null, segmentId: string | null = null) =>
      action(() => api.play({ bookId: id, chapter, segmentId })),
    /** Throws away a sentence's audio and makes it again, from the playhead put on it. */
    sayAgain: (id: string, segmentId: string) => action(() => api.sayAgain({ bookId: id, segmentId })),
    skipChapter: (delta: number) => action(() => api.skipChapter({ delta })),
    seek: (id: string, positionMs: number) => action(() => api.seek({ bookId: id, positionMs })),
    skip: (id: string, deltaMs: number) => action(() => api.skip({ bookId: id, deltaMs })),
    setSleepTimer: (minutes: number | null, endOfChapter: boolean) =>
      action(() => api.setSleepTimer({ minutes, endOfChapter })),
    pause: () => action(() => api.pause()),
    resume: () => action(() => api.resume()),
    stop: () => action(() => api.stop()),
    setSpeed,
    setBookSpeed,
    setVolume,
    toggleMute,
    sleepMenu,
    askSleep,
  };
});
