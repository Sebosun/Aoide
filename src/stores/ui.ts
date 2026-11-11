import { defineStore } from "pinia";
import { ref, shallowRef } from "vue";

export interface ConfirmOptions { title: string; body?: string; confirmLabel?: string; cancelLabel?: string; danger?: boolean }
export interface PromptOptions { title: string; label: string; value?: string; confirmLabel?: string; placeholder?: string }
export interface Toast { id: number; text: string; tone: "info" | "error"; key?: string }
interface Pending extends ConfirmOptions { id: number; resolve: (ok: boolean) => void }

/** App-wide dialogs and messages: `confirm()` opens a modal and resolves with the answer;
 *  `toast()` shows a short message that fades on its own (one with a `key` replaces the last
 *  message with that key, for values that change quickly). UiHost renders both. */
export const useUiStore = defineStore("ui", () => {
  let next = 0;
  const pending = shallowRef<Pending | null>(null);
  function confirm(options: ConfirmOptions) {
    pending.value?.resolve(false);
    return new Promise<boolean>(resolve => { pending.value = { ...options, id: ++next, resolve }; });
  }
  function answer(ok: boolean) { const asked = pending.value; pending.value = null; asked?.resolve(ok); }

  /** Asks for one line of text; resolves with it trimmed, or null when cancelled. */
  const asking = shallowRef<(PromptOptions & { id: number; resolve: (text: string | null) => void }) | null>(null);
  function prompt(options: PromptOptions) {
    asking.value?.resolve(null);
    return new Promise<string | null>(resolve => { asking.value = { ...options, id: ++next, resolve }; });
  }
  function reply(text: string | null) { const asked = asking.value; asking.value = null; asked?.resolve(text === null ? null : text.trim()); }

  const toasts = ref<Toast[]>([]);
  const timers = new Map<number, ReturnType<typeof setTimeout>>();
  function dismiss(id: number) { clearTimeout(timers.get(id)); timers.delete(id); toasts.value = toasts.value.filter(t => t.id !== id); }
  function toast(text: string, tone: Toast["tone"] = "info", key?: string) {
    const shown = key ? toasts.value.find(t => t.key === key) : undefined;
    const id = shown?.id ?? ++next;
    toasts.value = shown ? toasts.value.map(t => t.id === id ? { ...t, text, tone } : t) : [...toasts.value.slice(-2), { id, text, tone, key }];
    clearTimeout(timers.get(id));
    timers.set(id, setTimeout(() => dismiss(id), tone === "error" ? 6000 : 3000));
  }
  return { pending, confirm, answer, asking, prompt, reply, toasts, toast, dismiss };
});
