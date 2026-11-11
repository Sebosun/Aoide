import { nextTick, onUnmounted, ref, watch, type Ref } from "vue";

/** A popup menu: opens from a button, focuses its first item, arrow keys move between
 *  items, Escape and outside clicks close it (Escape returns focus to the button). */
export function useMenu(root: Ref<HTMLElement | undefined>, menu: Ref<HTMLElement | undefined>, button: Ref<HTMLElement | undefined>) {
  const open = ref(false);
  const items = () => [...(menu.value?.querySelectorAll<HTMLElement>('[role^="menuitem"]:not(:disabled)') ?? [])];
  function close(refocus = false) { open.value = false; if (refocus) button.value?.focus(); }
  // The menu may be teleported out of `root`, so clicks inside it are not "outside".
  function outside(event: PointerEvent) { const target = event.target as Node; if (!root.value?.contains(target) && !menu.value?.contains(target)) close(); }
  watch(open, async value => {
    if (!value) { document.removeEventListener("pointerdown", outside); return; }
    document.addEventListener("pointerdown", outside);
    await nextTick();
    const list = items();
    (list.find(item => item.getAttribute("aria-current") === "true") ?? list[0])?.focus();
  });
  onUnmounted(() => document.removeEventListener("pointerdown", outside));
  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") { event.stopPropagation(); close(true); return; }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const list = items(), at = list.indexOf(document.activeElement as HTMLElement);
    list[(at + (event.key === "ArrowDown" ? 1 : -1) + list.length) % list.length]?.focus();
  }
  return { open, close, onKeydown };
}
