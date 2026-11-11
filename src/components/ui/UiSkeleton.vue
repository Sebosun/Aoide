<script setup lang="ts">
/** A grey block that stands in for content that is still loading. The parent sizes it with
 *  its own class. It stays invisible for a moment, so content that arrives quickly never
 *  flashes a placeholder first; announce the loading itself with a `role="status"` element. */
</script>

<template>
  <span class="ui-skeleton" aria-hidden="true" />
</template>

<style scoped>
.ui-skeleton { display: block; min-height: 10px; border-radius: 8px; background: var(--line); animation: ui-skeleton-in var(--dur-2) 150ms both, ui-skeleton-pulse 1.2s ease-in-out 350ms infinite alternate; }
@keyframes ui-skeleton-in { from { opacity: 0; } }
@keyframes ui-skeleton-pulse { to { opacity: .55; } }
</style>
