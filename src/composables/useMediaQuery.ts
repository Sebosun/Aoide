import { onUnmounted, ref } from "vue";

/** Whether a media query matches, from the first render on, kept up to date. False where
 *  matchMedia is unavailable. */
export function useMediaQuery(query: string) {
  const list = typeof window.matchMedia === "function" ? window.matchMedia(query) : undefined;
  const matches = ref(Boolean(list?.matches));
  const update = () => {
    matches.value = Boolean(list?.matches);
  };
  list?.addEventListener?.("change", update);
  onUnmounted(() => list?.removeEventListener?.("change", update));
  return matches;
}
