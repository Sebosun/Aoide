<script setup lang="ts">
/** An article where a book has its cover: its site, headline and first lines, or a pulsing
 *  outline of them while it is being fetched. With the article's `image`, the picture fills the
 *  tile and the site and headline lie over its foot. The parent sizes it; `small` suits a short
 *  tile. */
withDefaults(
  defineProps<{
    site: string;
    title: string;
    lead: string | null;
    image?: string | null;
    loading?: boolean;
    small?: boolean;
  }>(),
  { image: null },
);
</script>

<template>
  <span
    class="article-tile"
    :class="{
      'article-tile--loading': loading,
      'article-tile--small': small,
      'article-tile--image': image && !loading,
    }"
  >
    <template v-if="loading"><i /><i /><i /></template>
    <template v-else-if="image">
      <img class="article-tile__image" :src="image" alt="" loading="lazy" decoding="async" draggable="false" />
      <span class="article-tile__over">
        <span class="article-tile__site">{{ site }}</span>
        <span class="article-tile__headline">{{ title }}</span>
      </span>
    </template>
    <template v-else>
      <span class="article-tile__site">{{ site }}</span>
      <span class="article-tile__headline">{{ title }}</span>
      <span v-if="lead" class="article-tile__lead">{{ lead }}</span>
    </template>
  </span>
</template>

<style scoped>
.article-tile {
  width: 100%;
  min-width: 0;
  overflow-wrap: anywhere;
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  padding: var(--space-8);
  overflow: hidden;
  border-radius: var(--radius-md);
  background: var(--page);
  color: var(--ink);
  text-align: left;
}
.article-tile__site {
  flex: none;
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
  flex: none;
  font-family: var(--font-read);
  font-size: 20px;
  line-height: 1.18;
  font-weight: 500;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
/* The lead takes what room is left and fades where the tile ends. */
.article-tile__lead {
  min-height: 0;
  -webkit-mask-image: linear-gradient(var(--ink) 70%, transparent);
  mask-image: linear-gradient(var(--ink) 70%, transparent);
  font-family: var(--font-read);
  font-size: 13.5px;
  line-height: 1.4;
  color: var(--dim);
  display: -webkit-box;
  -webkit-line-clamp: 5;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
/* The picture fills the tile; the site and headline sit at its foot on a fade into the page's
   colour, so they read in either theme. */
.article-tile--image {
  position: relative;
  justify-content: flex-end;
  padding: 0;
  background: var(--line);
}
.article-tile__image {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.article-tile__over {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-16) var(--space-7) var(--space-7);
  background: linear-gradient(
    transparent,
    color-mix(in srgb, var(--page) 86%, transparent) 40%,
    color-mix(in srgb, var(--page) 96%, transparent)
  );
}
.article-tile--small.article-tile--image .article-tile__over {
  padding: var(--space-12) var(--space-5) var(--space-5);
}
.article-tile--small {
  gap: var(--space-3);
  padding: var(--space-6);
  border-radius: var(--radius-sm);
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
  border-radius: var(--radius-sm);
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
