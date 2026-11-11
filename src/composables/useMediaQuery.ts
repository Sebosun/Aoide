import { onMounted, onUnmounted, ref } from "vue";

/** Whether a media query matches, kept up to date. False where matchMedia is unavailable. */
export function useMediaQuery(query: string) {
  const matches = ref(false);
  let list: MediaQueryList | undefined;
  const update = () => { matches.value = Boolean(list?.matches); };
  onMounted(() => { list = typeof window.matchMedia === "function" ? window.matchMedia(query) : undefined; update(); list?.addEventListener?.("change", update); });
  onUnmounted(() => list?.removeEventListener?.("change", update));
  return matches;
}
