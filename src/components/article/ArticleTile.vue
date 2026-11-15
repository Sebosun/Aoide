<script setup lang="ts">
/** An article where a book has its cover: its site, headline and first lines, or a pulsing
 *  outline of them while it is being fetched. The parent sizes it; `small` suits a short tile. */
defineProps<{ site: string; title: string; lead: string | null; loading?: boolean; small?: boolean }>();
</script>

<template>
  <span class="article-tile" :class="{ 'article-tile--loading': loading, 'article-tile--small': small }">
    <template v-if="loading"><i /><i /><i /></template>
    <template v-else>
      <span class="article-tile__site">{{ site }}</span>
      <span class="article-tile__headline">{{ title }}</span>
      <span v-if="lead" class="article-tile__lead">{{ lead }}</span>
    </template>
  </span>
</template>

<style scoped>
.article-tile {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 16px;
  overflow: hidden;
  border-radius: 12px;
  background: var(--page);
  color: var(--ink);
  text-align: left;
}
.article-tile__site {
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.article-tile__headline {
  font-family: var(--font-read);
  font-size: 20px;
  line-height: 1.18;
  font-weight: 500;
  display: -webkit-box;
  -webkit-line-clamp: 4;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.article-tile__lead {
  font-family: var(--font-read);
  font-size: 13.5px;
  line-height: 1.4;
  color: var(--dim);
  display: -webkit-box;
  -webkit-line-clamp: 5;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.article-tile--small {
  gap: 6px;
  padding: 12px;
  border-radius: 8px;
}
.article-tile--small .article-tile__headline {
  font-size: 17px;
  line-height: 1.2;
  -webkit-line-clamp: 3;
}
.article-tile--small .article-tile__lead {
  font-size: 12.5px;
  -webkit-line-clamp: 2;
}
.article-tile--loading i {
  display: block;
  height: 12px;
  border-radius: 6px;
  background: var(--line);
  animation: article-tile-pulse 1.4s ease-in-out infinite;
}
.article-tile--loading i:first-child {
  width: 40%;
}
.article-tile--loading i:nth-child(2) {
  height: 22px;
  width: 85%;
}
.article-tile--loading i:nth-child(3) {
  width: 70%;
}
@keyframes article-tile-pulse {
  50% {
    opacity: 0.45;
  }
}
@media (prefers-reduced-motion: reduce) {
  .article-tile--loading i {
    animation: none;
  }
}
</style>
