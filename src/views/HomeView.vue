<script setup lang="ts">
import UiCard from "@/components/ui/UiCard.vue";
import UiButton from "@/components/ui/UiButton.vue";
import { computed, onMounted, ref, watch } from "vue";
import { api } from "@/api";
import type { BookSummary, HomeView } from "@/api/types";
import CoverArt from "@/components/CoverArt.vue";
import ArticleTile from "@/components/article/ArticleTile.vue";
import { formatDuration, readingMs } from "@/estimate";
import { Plus } from "@lucide/vue";
import WeekCard from "@/components/library/WeekCard.vue";
import HomeContinueCard from "@/components/home/HomeContinueCard.vue";
import { useLibraryStore } from "@/stores/library";
import { useNavigationStore } from "@/stores/navigation";
import { dayName } from "@/days";
import { NEEDS } from "@/articles";

/** Home: what to continue, what is next, this week, and what was added recently. The core
 *  chooses the items (`home`); this screen looks them up in the library it already has. */
const library = useLibraryStore(),
  navigation = useNavigationStore();
const home = ref<HomeView | null>(null),
  error = ref("");
async function load() {
  try {
    home.value = await api.home();
    error.value = "";
  } catch (e) {
    error.value = String(e);
  }
}
onMounted(load);
watch(() => library.books, load);
const pick = (ids: string[] | undefined) =>
  (ids ?? []).flatMap((id) => library.books.filter((b) => b.id === id)) as BookSummary[];
const continuing = computed(() => pick(home.value?.continueIds));
const upNext = computed(() => pick(home.value?.upNextIds));
const recent = computed(() => pick(home.value?.recentIds));
const empty = computed(
  () => home.value !== null && !continuing.value.length && !upNext.value.length && !recent.value.length,
);
const added = (at: number) => dayName(at);
</script>

<template>
  <div class="home">
    <p v-if="error" class="home__error" role="alert">{{ error }}</p>
    <UiCard pad="lg" v-if="empty" class="home__welcome" aria-labelledby="home-welcome">
      <h1 id="home-welcome" class="home__title">Nothing to read yet</h1>
      <p class="home__lead">Add books to start reading, or listen to them with a voice.</p>
      <UiButton variant="primary" size="lg" class="home__add" aria-label="Add" @click="navigation.openAdd()">
        <Plus :size="16" aria-hidden="true" /> Add books
      </UiButton>
    </UiCard>
    <template v-else>
      <section v-if="continuing.length" class="home__section" aria-labelledby="home-continue">
        <h1 id="home-continue" class="home__title">Continue</h1>
        <div class="home__continue">
          <HomeContinueCard v-for="(book, i) in continuing" :key="book.id" :book="book" :hero="i === 0" />
        </div>
      </section>
      <div class="home__pair">
        <section v-if="upNext.length" class="home__section home__section--next" aria-labelledby="home-next">
          <div class="home__heading">
            <h2 id="home-next" class="home__subtitle">Up next</h2>
          </div>
          <UiCard as="ul" pad="list" class="home__rows">
            <li v-for="book in upNext" :key="book.id">
              <button
                type="button"
                class="home__row"
                :aria-label="`About ${book.title}`"
                @click="navigation.openItem(book.id)"
              >
                <span class="home__row-text">
                  <b class="home__row-title" :class="{ 'home__row-title--article': book.kind === 'article' }">{{
                    book.title
                  }}</b>
                  <span v-if="book.kind === 'article'" class="home__row-meta"
                    >{{ book.article?.siteName ?? book.title }} · saved {{ added(book.addedAt)
                    }}<b v-if="book.article?.stage.kind === 'needsYou'" class="home__row-needs">
                      · Needs you · {{ NEEDS[book.article.stage.reason] }}</b
                    ></span
                  >
                  <span v-else class="home__row-meta"
                    >{{ book.kind === "audiobook" ? "Audiobook" : "Book"
                    }}<template v-if="book.authors.length"> · {{ book.authors.join(", ") }}</template> · added
                    {{ added(book.addedAt) }}</span
                  >
                </span>
                <span v-if="book.kind === 'article' && book.wordCount" class="home__row-time">{{
                  formatDuration(readingMs(book.wordCount, library.readingWpm))
                }}</span>
              </button>
            </li>
          </UiCard>
        </section>
        <section class="home__section home__section--week" aria-labelledby="home-week">
          <h2 id="home-week" class="home__subtitle">This week</h2>
          <WeekCard />
        </section>
      </div>
      <section v-if="recent.length" class="home__section" aria-labelledby="home-recent">
        <h2 id="home-recent" class="home__subtitle">Recently added</h2>
        <ul class="home__covers">
          <li v-for="book in recent" :key="book.id">
            <button
              type="button"
              class="home__cover"
              :class="{ 'home__cover--article': book.kind === 'article' }"
              :aria-label="`${book.title}, ${book.kind}`"
              :title="book.title"
              @click="navigation.openItem(book.id)"
            >
              <ArticleTile
                v-if="book.kind === 'article'"
                small
                class="home__tile"
                :site="book.article?.siteName ?? book.title"
                :title="book.title"
                :lead="book.article?.lead ?? null"
              />
              <CoverArt
                v-else
                :title="book.title"
                :url="book.coverUrl"
                :shape="book.kind === 'audiobook' ? 'square' : 'portrait'"
              />
            </button>
          </li>
        </ul>
      </section>
    </template>
  </div>
</template>

<style scoped>
.home {
  display: flex;
  flex-direction: column;
  gap: var(--space-16);
  padding: var(--space-4) var(--space-16) 0;
}
.home__section {
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
  min-width: 0;
}
.home__title {
  margin: 0;
  font-size: 24px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.home__subtitle {
  margin: 0;
  font-size: 18px;
  font-weight: 700;
}
.home__continue {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-8);
}
.home__pair {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-16);
  align-items: flex-start;
}
.home__section--next {
  flex: 3 1 420px;
}
.home__section--week {
  flex: 2 1 280px;
}
.home__heading {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
}
.home__rows {
  margin: 0;
  list-style: none;
}
.home__rows li + li {
  border-top: 1px solid var(--wash);
}
.home__row {
  width: 100%;
  display: flex;
  align-items: baseline;
  gap: var(--space-7);
  padding: var(--space-7) 0;
  text-align: left;
}
.home__row-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}
.home__row-title--article {
  font-family: var(--font-read);
  font-size: 18px;
  font-weight: 400;
  line-height: 1.25;
}
.home__row-time {
  font-size: 13px;
  color: var(--dim);
  white-space: nowrap;
}
.home__row-title {
  font-size: 16px;
  font-weight: 600;
  line-height: 1.3;
}
.home__row:hover .home__row-title {
  color: var(--accent);
}
.home__row-meta {
  font-size: 13px;
  color: var(--dim);
}
.home__row-needs {
  color: var(--warn);
  font-weight: 600;
}
.home__covers {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-9);
  align-items: flex-end;
}
.home__cover {
  display: block;
  width: 96px;
  border-radius: var(--radius-sm);
  transition: transform var(--dur-1) var(--ease);
}
.home__cover:hover {
  transform: translateY(-2px);
}
.home__cover--article {
  width: 200px;
}
.home__tile {
  height: 144px;
}
.home__welcome {
  display: grid;
  justify-items: start;
  gap: var(--space-6);
  max-width: 520px;
}
.home__lead {
  margin: 0 0 var(--space-3);
  color: var(--dim);
}
.home__error {
  margin: 0;
  color: var(--accent);
  font-weight: 600;
}
@media (max-width: 719px) {
  .home {
    padding: 0 var(--space-8);
  }
}
</style>
