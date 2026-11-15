<script setup lang="ts">
import { computed, ref } from "vue";
import { Download, Rss, Upload } from "@lucide/vue";
import type { FeedSettings } from "../../api/types";
import SettingsSection from "./SettingsSection.vue";
import SettingsField from "./SettingsField.vue";
import SettingsNote from "./SettingsNote.vue";
import UiSelect from "../ui/UiSelect.vue";
import { useAppSettings } from "../../composables/useAppSettings";
import { useFeedsStore } from "../../stores/feeds";

/** How often feeds are checked, how long their entries stay, and subscription lists in and
 *  out. Every change saves at once. */
const { settings, error, save } = useAppSettings();
const feeds = useFeedsStore();
const INTERVALS = [
  { value: 15, label: "Every 15 minutes" },
  { value: 30, label: "Every 30 minutes" },
  { value: 60, label: "Every hour" },
  { value: 180, label: "Every 3 hours" },
  { value: 360, label: "Every 6 hours" },
  { value: 720, label: "Every 12 hours" },
  { value: 1440, label: "Once a day" },
];
const current = computed(() => settings.value?.feeds ?? null);
/** Intervals offered, with the saved one even when it is not a listed step. */
const intervals = computed(() =>
  current.value && !INTERVALS.some((i) => i.value === current.value!.checkMinutes)
    ? [...INTERVALS, { value: current.value.checkMinutes, label: `Every ${current.value.checkMinutes} minutes` }].sort(
        (a, b) => a.value - b.value,
      )
    : INTERVALS,
);
const change = (patch: Partial<FeedSettings>) => {
  if (current.value) void save({ feeds: { ...current.value, ...patch } });
};
function days(field: "keepReadDays" | "keepUnreadDays", event: Event) {
  const input = event.target as HTMLInputElement;
  const value = Math.round(Number(input.value));
  if (!Number.isFinite(value) || value < 1) {
    input.value = String(current.value?.[field] ?? "");
    return;
  }
  change({ [field]: value });
}

const result = ref(""),
  busy = ref(false);
const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
async function run(task: () => Promise<string | null>) {
  busy.value = true;
  error.value = "";
  try {
    const said = await task();
    if (said !== null) result.value = said;
  } catch (e) {
    error.value = String(e).replace(/^Error: /, "");
  } finally {
    busy.value = false;
  }
}
const importList = () =>
  run(async () => {
    const done = await feeds.importList();
    if (!done) return null;
    const added = done.added ? `Added ${plural(done.added, "feed")}.` : "No new feeds.";
    return done.skipped ? `${added} ${done.skipped} skipped: already subscribed, or not at a web address.` : added;
  });
const exportList = () =>
  run(async () => {
    const count = await feeds.exportList();
    return count === null ? null : `Exported ${plural(count, "feed")}.`;
  });
</script>

<template>
  <div class="feed-settings">
    <SettingsSection title="Checking feeds" :icon="Rss">
      <SettingsField v-if="current" label="Check for new entries" as="div"
        ><UiSelect
          :model-value="current.checkMinutes"
          label="How often feeds are checked"
          :options="intervals"
          @update:model-value="(minutes: number) => change({ checkMinutes: minutes })"
      /></SettingsField>
      <SettingsNote
        >Aoide asks each site only whether its feed changed. A feed that stops answering is asked less often until it
        answers again.</SettingsNote
      >
    </SettingsSection>
    <SettingsSection title="Keeping entries">
      <template v-if="current">
        <SettingsField label="Read entries" sub="days"
          ><input
            :value="current.keepReadDays"
            type="number"
            min="1"
            max="3650"
            aria-label="Days to keep read entries"
            @change="days('keepReadDays', $event)"
        /></SettingsField>
        <SettingsField label="Unread entries" sub="days"
          ><input
            :value="current.keepUnreadDays"
            type="number"
            min="1"
            max="3650"
            aria-label="Days to keep unread entries"
            @change="days('keepUnreadDays', $event)"
        /></SettingsField>
      </template>
      <SettingsNote>Entries you save to the library are always kept, with their text.</SettingsNote>
    </SettingsSection>
    <SettingsSection title="Subscription lists">
      <div class="feed-settings__buttons">
        <button type="button" class="feed-settings__button" :disabled="busy" @click="importList">
          <Download :size="15" aria-hidden="true" />Import…
        </button>
        <button type="button" class="feed-settings__button" :disabled="busy" @click="exportList">
          <Upload :size="15" aria-hidden="true" />Export…
        </button>
      </div>
      <p v-if="result" class="feed-settings__result" role="status">{{ result }}</p>
      <SettingsNote>OPML files, as other feed readers import and export them.</SettingsNote>
    </SettingsSection>
    <p v-if="error" class="feed-settings__error" role="alert">{{ error }}</p>
  </div>
</template>

<style scoped>
.feed-settings {
  display: grid;
  gap: 22px;
}
.feed-settings__buttons {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.feed-settings__button {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 36px;
  padding: 0 16px;
  border-radius: 999px;
  background: var(--ground);
  font-weight: 700;
  font-size: 13px;
}
.feed-settings__result {
  margin: 0;
  font-size: 13px;
}
.feed-settings__error {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--accent);
}
</style>
