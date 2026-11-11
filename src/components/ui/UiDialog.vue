<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref } from "vue";

/** A modal: a backdrop and a panel that keeps keyboard focus inside it. Escape or the backdrop
 *  closes it; focus returns to whatever opened it (or to `returnFocus` when WebKit had not
 *  focused the opener). `sheet` rises from the bottom, `center` sits in the middle. */
const props = withDefaults(defineProps<{ label: string; variant?: "sheet" | "center"; role?: "dialog" | "alertdialog"; initialFocus?: string; returnFocus?: string; describedby?: string }>(), { variant: "center", role: "dialog" });
const emit = defineEmits<{ close: [] }>();
const panel = ref<HTMLElement>();
const FOCUSABLE = 'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';
const focusable = () => [...(panel.value?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? [])].filter(el => !el.hasAttribute("disabled") && el.tabIndex >= 0);

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") { event.stopPropagation(); emit("close"); return; }
  if (event.key !== "Tab") return;
  const list = focusable(), first = list[0], last = list.at(-1);
  if (!first) { event.preventDefault(); return; }
  if (event.shiftKey && (document.activeElement === first || !panel.value?.contains(document.activeElement))) { event.preventDefault(); last?.focus(); }
  else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
}
const active = document.activeElement as HTMLElement | null;
const opener = active && active !== document.body ? active : props.returnFocus ? document.querySelector<HTMLElement>(props.returnFocus) : null;
onMounted(async () => {
  await nextTick();
  const wanted = props.initialFocus ? panel.value?.querySelector<HTMLElement>(props.initialFocus) : null;
  (wanted ?? focusable()[0] ?? panel.value)?.focus();
});
onUnmounted(() => { if (opener?.isConnected) opener.focus(); });
</script>

<template>
  <div class="ui-dialog" :class="`ui-dialog--${variant}`">
    <div class="ui-dialog__backdrop" @click="emit('close')" />
    <section ref="panel" class="ui-dialog__panel" :role="role" aria-modal="true" :aria-label="label" :aria-describedby="describedby" tabindex="-1" @keydown="onKeydown">
      <slot />
    </section>
  </div>
</template>

<style scoped>
.ui-dialog { position: fixed; inset: 0; z-index: 30; display: grid; grid-template-rows: minmax(0, 1fr); justify-items: center; }
.ui-dialog--sheet { align-items: end; }
.ui-dialog--center { align-items: center; padding: 24px 16px; }
.ui-dialog__backdrop { position: absolute; inset: 0; background: color-mix(in srgb, var(--ground) 50%, transparent); backdrop-filter: blur(2px); animation: ui-dialog-fade var(--dur-2) var(--ease); }
/* The one row is exactly the window, so max-height is a real limit in every engine (WebKitGTK
   let a tall sheet grow past it under page zoom), and nothing draws outside the panel: long
   content scrolls inside it instead. */
.ui-dialog__panel { position: relative; display: flex; flex-direction: column; max-width: 100%; max-height: 100%; overflow: hidden; background: var(--page); color: var(--ink); }
.ui-dialog__panel:focus-visible { outline: none; }
.ui-dialog__panel > :deep(*) { flex: 1 1 auto; min-height: 0; }
.ui-dialog--sheet .ui-dialog__panel { border-radius: 22px 22px 0 0; box-shadow: 0 -18px 50px -20px var(--shadow); animation: ui-dialog-rise var(--dur-2) var(--ease); }
.ui-dialog--center .ui-dialog__panel { border-radius: 20px; box-shadow: 0 30px 70px -24px var(--shadow), 0 0 0 1px var(--line); animation: ui-dialog-pop var(--dur-2) var(--ease); }
@keyframes ui-dialog-fade { from { opacity: 0; } }
@keyframes ui-dialog-rise { from { transform: translateY(24px); opacity: 0; } }
@keyframes ui-dialog-pop { from { transform: scale(.97); opacity: 0; } }
</style>
