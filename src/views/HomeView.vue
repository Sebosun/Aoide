<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { api } from "../api";
import type { BookSummary, HomeView } from "../api/types";
import CoverArt from "../components/CoverArt.vue";
import { Plus } from "@lucide/vue";
import WeekCard from "../components/library/WeekCard.vue";
import HomeContinueCard from "../components/home/HomeContinueCard.vue";
import { useLibraryStore } from "../stores/library";
import { useNavigationStore } from "../stores/navigation";

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
/** "today", "yesterday", a weekday within the week, else the date. */
function added(at: number) {
  const day = (ms: number) => Math.floor((ms - new Date(ms).getTimezoneOffset() * 60_000) / 86_400_000);
  const ago = day(Date.now()) - day(at);
  if (ago <= 0) return "today";
  if (ago === 1) return "yesterday";
  if (ago < 7) return new Date(at).toLocaleDateString(undefined, { weekday: "long" });
  return new Date(at).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}
</script>

<template>
  <div class="home">
    <p v-if="error" class="home__error" role="alert">{{ error }}</p>
    <section v-if="empty" class="home__welcome" aria-labelledby="home-welcome">
      <h1 id="home-welcome" class="home__title">Nothing to read yet</h1>
      <p class="home__lead">Add books to start reading, or listen to them with a voice.</p>
      <button type="button" class="home__add" aria-label="Add" @click="navigation.openAdd()">
        <Plus :size="16" aria-hidden="true" /> Add books
      </button>
    </section>
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
          <ul class="home__rows">
            <li v-for="book in upNext" :key="book.id">
              <button
                type="button"
                class="home__row"
                :aria-label="`About ${book.title}`"
                @click="navigation.openItem(book.id)"
              >
                <b class="home__row-title">{{ book.title }}</b>
                <span class="home__row-meta"
                  >{{ book.kind === "audiobook" ? "Audiobook" : "Book"
                  }}<template v-if="book.authors.length"> · {{ book.authors.join(", ") }}</template> · added
                  {{ added(book.addedAt) }}</span
                >
              </button>
            </li>
          </ul>
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
              :aria-label="`${book.title}, ${book.kind === 'audiobook' ? 'audiobook' : 'book'}`"
              :title="book.title"
              @click="navigation.openItem(book.id)"
            >
              <CoverArt
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
  gap: 32px;
  padding: 8px 36px 0;
}
.home__section {
  display: flex;
  flex-direction: column;
  gap: 12px;
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
  gap: 16px;
}
.home__pair {
  display: flex;
  flex-wrap: wrap;
  gap: 32px;
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
  padding: 4px 18px;
  list-style: none;
  border-radius: 18px;
  background: var(--page);
}
.home__rows li + li {
  border-top: 1px solid var(--wash);
}
.home__row {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 14px 0;
  text-align: left;
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
.home__covers {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-wrap: wrap;
  gap: 18px;
  align-items: flex-end;
}
.home__cover {
  display: block;
  width: 96px;
  border-radius: 8px;
  transition: transform var(--dur-1) var(--ease);
}
.home__cover:hover {
  transform: translateY(-2px);
}
.home__welcome {
  display: grid;
  justify-items: start;
  gap: 12px;
  max-width: 520px;
  padding: 28px;
  border-radius: 22px;
  background: var(--page);
}
.home__lead {
  margin: 0 0 6px;
  color: var(--dim);
}
.home__add {
  height: 44px;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 0 20px;
  border-radius: 22px;
  background: var(--ink);
  color: var(--ground);
  font-weight: 600;
}
.home__error {
  margin: 0;
  color: var(--accent);
  font-weight: 600;
}
@media (max-width: 719px) {
  .home {
    padding: 0 16px;
  }
}
</style>
