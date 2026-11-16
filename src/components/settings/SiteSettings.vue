<script setup lang="ts">
import UiButton from "@/components/ui/UiButton.vue";
import { computed, onMounted, onUnmounted, ref } from "vue";
import { Download, Globe, Trash2, X } from "@lucide/vue";
import { api } from "@/api";
import type { BrowserDownloadView, BrowserStatusView, BrowserUse, SiteData, SiteFetch, SiteView } from "@/api/types";
import SettingsSection from "@/components/settings/SettingsSection.vue";
import SettingsField from "@/components/settings/SettingsField.vue";
import SettingsNote from "@/components/settings/SettingsNote.vue";
import UiSelect from "@/components/ui/UiSelect.vue";
import { useAppSettings } from "@/composables/useAppSettings";
import { useUiStore } from "@/stores/ui";
import { useSignInStore } from "@/stores/signin";
import { formatBytes } from "@/bytes";
import { age } from "@/days";

/** Whether a browser fetches articles plain HTTP cannot read, which browser, and the sites
 *  that needed the user: their session, whether their data is kept between fetches, how they
 *  are fetched. With the browser off, the rest is shown but cannot be used. */
const { settings, error, save } = useAppSettings();
const ui = useUiStore(),
  signIn = useSignInStore();
const status = ref<BrowserStatusView | null>(null),
  sites = ref<SiteView[]>([]),
  download = ref<BrowserDownloadView | null>(null),
  busy = ref(false);
let unlisten: (() => void) | undefined,
  disposed = false;

async function load() {
  try {
    [status.value, sites.value] = await Promise.all([api.browserStatus(), api.sites()]);
    download.value = status.value.download;
  } catch (e) {
    error.value = String(e);
  }
}
onMounted(async () => {
  await load();
  const off = await api.onBrowserDownload((d) => {
    download.value = d;
    if (["done", "failed", "cancelled"].includes(d.stage)) void load();
  });
  if (disposed) off();
  else unlisten = off;
});
onUnmounted(() => {
  disposed = true;
  unlisten?.();
});

const browser = computed(() => settings.value?.browser ?? null);
const off = computed(() => browser.value?.use === "off");
const USES: { value: BrowserUse; label: string; hint: string }[] = [
  { value: "off", label: "Off", hint: "Articles are fetched as plain pages only" },
  {
    value: "fallback",
    label: "When plain fetching fails",
    hint: "A page that comes back cut short or behind a check is opened in a browser on this computer",
  },
  { value: "first", label: "Always", hint: "Every article is opened in a browser on this computer: slower" },
];
async function setUse(use: BrowserUse) {
  if (browser.value && (await save({ browser: { ...browser.value, use } }))) await load();
}
async function choosePath() {
  const path = await api.chooseBrowser();
  if (path && browser.value && (await save({ browser: { ...browser.value, path } }))) await load();
}
async function forgetPath() {
  if (browser.value && (await save({ browser: { ...browser.value, path: null } }))) await load();
}

const found = computed(() => (status.value?.binary.kind === "found" ? status.value.binary : null));
const downloading = computed(
  () => download.value !== null && ["downloading", "checking", "unpacking"].includes(download.value.stage),
);
const downloadText = computed(() => {
  const d = download.value;
  if (!d) return "";
  if (d.stage === "downloading")
    return `Downloading Chrome ${d.version}: ${formatBytes(d.doneBytes)} of ${formatBytes(d.totalBytes)}`;
  if (d.stage === "checking") return "Checking the download…";
  if (d.stage === "unpacking") return "Unpacking…";
  if (d.stage === "cancelled") return "Download stopped. Downloading again continues it.";
  if (d.stage === "failed") return d.error ?? "The download failed.";
  return "";
});
async function run(task: () => Promise<unknown>, done?: string) {
  busy.value = true;
  error.value = "";
  try {
    await task();
    if (done) ui.toast(done);
  } catch (e) {
    error.value = String(e).replace(/^Error: /, "");
  } finally {
    busy.value = false;
    await load();
  }
}
const startDownload = () => {
  // Resolves once the download ends; progress arrives as events meanwhile.
  void run(() => api.downloadBrowser());
};

const sessionText = (session: SiteView["session"]) => {
  switch (session.kind) {
    case "signedIn":
      return `Signed in ${age(session.at)}`;
    case "checkPassed":
      return `Check passed ${age(session.at)}`;
    case "askedFor":
      return {
        signIn: "Asked for a sign-in",
        paywall: "Asked for a subscription",
        check: "Showed a check",
        cutShort: "Gave only part of an article",
        empty: "Gave nothing to read",
      }[session.reason];
    default:
      return "No sign-in";
  }
};
const signedIn = (site: SiteView) => site.session.kind === "signedIn" || site.session.kind === "checkPassed";
const DATA: { value: SiteData; label: string }[] = [
  { value: "keep", label: "Keep" },
  { value: "clear_before_fetch", label: "Clear before each fetch" },
];
type Route = SiteFetch | "auto";
const ROUTES: { value: Route; label: string }[] = [
  { value: "auto", label: "Automatic" },
  { value: "http", label: "Plain" },
  { value: "browser", label: "Browser" },
];
const setRule = (site: SiteView, change: { fetch?: Route; siteData?: SiteData }) => {
  const fetch = change.fetch ?? site.fetch ?? "auto";
  void run(() =>
    api.setSiteRule({
      host: site.host,
      fetch: fetch === "auto" ? null : fetch,
      siteData: change.siteData ?? site.siteData,
    }),
  );
};
const signOut = (site: SiteView) => run(() => api.signOutSite({ host: site.host }), `Signed out of ${site.host}.`);
async function forget(site: SiteView) {
  const ok = await ui.confirm({
    title: `Forget ${site.host}?`,
    body: "Its sign-in and site data are removed, and its rule is dropped from site-rules.toml.",
    confirmLabel: "Forget",
    danger: true,
  });
  if (ok) await run(() => api.forgetSite({ host: site.host }), `Forgot ${site.host}.`);
}
async function removeAll() {
  const ok = await ui.confirm({
    title: "Remove all site data?",
    body: "This signs you out of every site and repeats any checks the next time they are fetched.",
    confirmLabel: "Remove all",
    danger: true,
  });
  if (ok) await run(() => api.clearBrowserData(), "Removed all site data.");
}
async function removeBrowser() {
  const ok = await ui.confirm({
    title: "Remove the downloaded browser?",
    body: "Downloading it again takes about 200 MB.",
    confirmLabel: "Remove",
    danger: true,
  });
  if (ok) await run(() => api.removeBrowser(), "Removed the downloaded browser.");
}
const startSignIn = (site: SiteView) => site.waitingArticle && void signIn.start(site.waitingArticle);
</script>

<template>
  <div class="site-settings">
    <SettingsSection title="Browser for articles" :icon="Globe">
      <SettingsField v-if="browser" label="Use a browser" as="div"
        ><UiSelect
          :model-value="browser.use"
          label="Use a browser for articles"
          :options="USES"
          @update:model-value="setUse"
      /></SettingsField>
      <SettingsNote
        >Some sites build their pages with scripts, ask you to sign in, or check that a person is reading. A browser
        reads those as you would. It runs on this computer with its own profile, and only sites on the internet can be
        opened in it.</SettingsNote
      >
      <div
        class="site-settings__browser"
        :class="{ 'site-settings__browser--off': off }"
        aria-label="Browser for fetching"
      >
        <div class="site-settings__which">
          <strong>Browser for fetching</strong>
          <span v-if="found" class="site-settings__dim"
            >{{ found.downloaded ? "Chrome, downloaded by Aoide" : found.path
            }}<template v-if="found.version"> · {{ found.version }}</template></span
          >
          <span v-else-if="status" class="site-settings__dim">No Chrome or Chromium was found.</span>
        </div>
        <div class="site-settings__buttons">
          <UiButton
            variant="outline"
            size="sm"
            class="site-settings__button"
            :disabled="off || busy"
            @click="choosePath"
          >
            Change…
          </UiButton>
          <UiButton
            variant="outline"
            size="sm"
            v-if="browser?.path"
            class="site-settings__button"
            :disabled="off || busy"
            @click="forgetPath"
          >
            Find on its own
          </UiButton>
          <UiButton
            variant="primary"
            size="sm"
            v-if="!downloading && status?.offeredVersion && !status.downloaded && !found"
            class="site-settings__button site-settings__button--primary"
            :disabled="off || busy"
            @click="startDownload"
          >
            <Download :size="15" aria-hidden="true" />Download Chrome
          </UiButton>
          <UiButton
            variant="outline"
            size="sm"
            v-if="downloading"
            class="site-settings__button"
            @click="api.cancelBrowserDownload()"
          >
            <X :size="15" aria-hidden="true" />Cancel download
          </UiButton>
          <UiButton
            variant="outline"
            size="sm"
            v-if="status?.downloaded && !downloading"
            class="site-settings__button"
            :disabled="busy"
            @click="removeBrowser"
          >
            <Trash2 :size="15" aria-hidden="true" />Remove downloaded browser
          </UiButton>
        </div>
        <p v-if="downloadText" class="site-settings__progress" role="status">{{ downloadText }}</p>
        <progress
          v-if="download?.stage === 'downloading'"
          class="site-settings__bar"
          :value="download.doneBytes"
          :max="download.totalBytes"
          aria-label="Download progress"
        />
        <SettingsNote v-if="status?.offeredVersion && !found"
          >Download Chrome fetches Chrome for Testing {{ status.offeredVersion }} from Google (about 200 MB) and checks
          it before use. Its use is under Google's Chrome terms.</SettingsNote
        >
      </div>
    </SettingsSection>

    <SettingsSection title="Sites">
      <p v-if="!sites.length" class="site-settings__dim">
        Sites that ask for a sign-in or a check, and sites with a rule, are listed here.
      </p>
      <table v-else class="site-settings__table" :class="{ 'site-settings__table--off': off }" aria-label="Sites">
        <thead>
          <tr>
            <th scope="col">Site</th>
            <th scope="col">Session</th>
            <th scope="col">Site data between fetches</th>
            <th scope="col">Fetch</th>
            <th scope="col"><span class="site-settings__hidden">Action</span></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="site in sites" :key="site.host">
            <th scope="row" class="site-settings__host">
              <!-- Long names break only before a dot. -->
              <template v-for="(part, i) in site.host.split('.')" :key="i"
                ><wbr v-if="i" />{{ i ? "." : "" }}{{ part }}</template
              >
            </th>
            <td
              class="site-settings__session"
              :class="{
                'site-settings__session--in': signedIn(site),
                'site-settings__session--asked': site.session.kind === 'askedFor',
              }"
            >
              {{ sessionText(site.session) }}
            </td>
            <td>
              <UiSelect
                :model-value="site.siteData"
                :label="`Site data for ${site.host}`"
                :options="DATA.map((d) => ({ ...d, disabled: off }))"
                @update:model-value="(siteData: SiteData) => setRule(site, { siteData })"
              />
            </td>
            <td>
              <UiSelect
                :model-value="site.fetch ?? 'auto'"
                :label="`How ${site.host} is fetched`"
                :options="ROUTES.map((r) => ({ ...r, disabled: off }))"
                @update:model-value="(fetch: Route) => setRule(site, { fetch })"
              />
            </td>
            <td class="site-settings__action">
              <UiButton
                variant="outline"
                size="sm"
                v-if="signedIn(site)"
                class="site-settings__button"
                :disabled="off || busy"
                :aria-label="`Sign out of ${site.host}`"
                @click="signOut(site)"
              >
                Sign out
              </UiButton>
              <UiButton
                variant="primary"
                size="sm"
                v-else-if="site.waitingArticle"
                class="site-settings__button site-settings__button--primary"
                :disabled="off || busy || signIn.open"
                :aria-label="`Sign in to ${site.host}`"
                @click="startSignIn(site)"
              >
                Sign in…
              </UiButton>
              <UiButton
                variant="outline"
                size="sm"
                v-else
                class="site-settings__button"
                :disabled="off || busy"
                :aria-label="`Forget ${site.host}`"
                @click="forget(site)"
              >
                Forget
              </UiButton>
            </td>
          </tr>
        </tbody>
      </table>
      <div class="site-settings__remove">
        <SettingsNote>Removing all site data signs you out everywhere and repeats any checks.</SettingsNote>
        <UiButton
          variant="outline"
          size="sm"
          danger
          class="site-settings__button site-settings__button--danger"
          :disabled="busy"
          @click="removeAll"
        >
          Remove all site data
        </UiButton>
      </div>
      <SettingsNote>Other rules (pages to wait for, addresses to block) are kept in site-rules.toml.</SettingsNote>
    </SettingsSection>
    <p v-if="error || signIn.error" class="site-settings__error" role="alert">{{ error || signIn.error }}</p>
  </div>
</template>

<style scoped>
.site-settings {
  display: grid;
  gap: var(--space-11);
}
.site-settings__browser {
  display: grid;
  gap: var(--space-5);
  padding: var(--space-7) var(--space-8);
  border-radius: var(--radius-lg);
  background: var(--ground);
}
.site-settings__browser--off,
.site-settings__table--off {
  opacity: 0.6;
}
.site-settings__which {
  display: grid;
  gap: var(--space-1);
  font-size: 13px;
  overflow-wrap: anywhere;
}
.site-settings__dim {
  color: var(--dim);
  font-size: 13px;
}
.site-settings__buttons {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-4);
}
.site-settings__progress {
  margin: 0;
  font-size: 13px;
}
.site-settings__bar {
  width: 100%;
  height: 6px;
  accent-color: var(--accent);
}
.site-settings__table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}
.site-settings__table th,
.site-settings__table td {
  padding: var(--space-4) var(--space-3);
  text-align: left;
  vertical-align: middle;
  border-top: 1px solid var(--line);
}
.site-settings__table thead th {
  border-top: 0;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--dim);
}
.site-settings__host {
  font-weight: 700;
}
.site-settings__session {
  color: var(--dim);
}
.site-settings__session--in {
  color: var(--ok);
  font-weight: 600;
}
.site-settings__session--asked {
  color: var(--warn);
  font-weight: 600;
}
.site-settings__action {
  text-align: right;
}
.site-settings__hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0 0 0 0);
}
.site-settings__remove {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-6);
}
.site-settings__error {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--accent);
}
</style>
