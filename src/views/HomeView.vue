<script setup lang="ts">
/** Home: what to continue, what is next, this week, and what was added recently. */
</script>

<template>
  <section class="home" aria-labelledby="home-title">
    <h1 id="home-title" class="home__title">Home</h1>
  </section>
</template>

<style scoped>
.home {
  display: flex;
  flex-direction: column;
  gap: 32px;
  padding: 8px 36px 0;
}
.home__title {
  margin: 0;
  font-size: 24px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
</style>
