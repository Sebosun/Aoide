<script setup lang="ts">
/** Aoide's button: a pill with a label, an icon and a label, or an icon alone (`icon`, with an
 *  `aria-label`). Its fill says where it sits and what it does:
 *  - `soft` (default): on a page or a card, filled with the ground colour;
 *  - `raised`: on the ground, filled with the page colour;
 *  - `glass`: on a tinted screen, translucent;
 *  - `outline`: a page-coloured pill with a hairline border;
 *  - `primary`: the main action of a dialog or screen, in ink;
 *  - `accent`: the one action a screen invites, in the accent colour.
 *  `danger` colours the label for an action that removes something. Attributes and listeners
 *  (`disabled`, `aria-*`, `@click`, extra classes) go to the `<button>`. */
withDefaults(
  defineProps<{
    variant?: "soft" | "raised" | "glass" | "outline" | "primary" | "accent";
    size?: "sm" | "md" | "lg";
    icon?: boolean;
    danger?: boolean;
    type?: "button" | "submit" | "reset";
  }>(),
  { variant: "soft", size: "md", icon: false, danger: false, type: "button" },
);
</script>

<template>
  <button
    :type="type"
    class="ui-button"
    :class="[`ui-button--${variant}`, `ui-button--${size}`, { 'ui-button--icon': icon, 'ui-button--danger': danger }]"
  >
    <slot />
  </button>
</template>

<style scoped>
.ui-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-4);
  flex: none;
  padding: 0 var(--space-9);
  border-radius: var(--radius-pill);
  font-weight: 700;
  white-space: nowrap;
  transition:
    background-color var(--dur-1) var(--ease),
    box-shadow var(--dur-1) var(--ease),
    opacity var(--dur-1) var(--ease);
}
.ui-button--sm {
  height: var(--control-sm);
  padding: 0 var(--space-7);
  gap: var(--space-3);
  font-size: 13px;
}
.ui-button--md {
  height: var(--control-md);
  font-size: 14px;
}
.ui-button--lg {
  height: var(--control-lg);
  padding: 0 var(--space-11);
  gap: var(--space-5);
  font-size: 15px;
}
.ui-button--icon {
  width: var(--control-md);
  padding: 0;
}
.ui-button--icon.ui-button--sm {
  width: var(--control-sm);
}
.ui-button--icon.ui-button--lg {
  width: var(--control-lg);
}
.ui-button--soft {
  background: var(--ground);
}
.ui-button--raised {
  background: var(--page);
}
.ui-button--glass {
  background: var(--glass);
}
.ui-button--outline {
  background: var(--page);
  box-shadow: inset 0 0 0 1px var(--line);
}
.ui-button--primary {
  background: var(--ink);
  color: var(--page);
}
.ui-button--accent {
  background: var(--accent);
  color: var(--on-accent);
}
.ui-button--danger:not(.ui-button--accent, .ui-button--primary) {
  color: var(--accent);
}
.ui-button--danger.ui-button--primary {
  background: var(--accent);
  color: var(--on-accent);
}
.ui-button:not(:disabled):hover {
  box-shadow: inset 0 0 0 999px color-mix(in srgb, var(--ink) 6%, transparent);
}
.ui-button--outline:not(:disabled):hover {
  box-shadow:
    inset 0 0 0 1px var(--line),
    inset 0 0 0 999px color-mix(in srgb, var(--ink) 6%, transparent);
}
</style>
