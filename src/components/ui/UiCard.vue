<script setup lang="ts">
/** A page-coloured panel on the ground: a card of facts, a list of rows, an empty state. `as`
 *  is the element it renders (a `section` by default; `ul`, `li`, `div`, …); `pad` its inner
 *  space: `list` keeps rows edge to edge vertically, `none` leaves it to the content. */
withDefaults(defineProps<{ as?: string; pad?: "none" | "list" | "sm" | "md" | "lg" }>(), {
  as: "section",
  pad: "md",
});
</script>

<template>
  <component :is="as" class="ui-card" :class="`ui-card--${pad}`"><slot /></component>
</template>

<style scoped>
.ui-card {
  border-radius: var(--radius-lg);
  background: var(--page);
}
.ui-card--list {
  padding: var(--space-1) var(--space-9);
}
.ui-card--sm {
  padding: var(--space-6);
}
.ui-card--md {
  padding: var(--space-10);
}
.ui-card--lg {
  padding: var(--space-14);
}
</style>
