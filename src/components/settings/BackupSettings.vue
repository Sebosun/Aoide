<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { ArchiveRestore, DatabaseBackup, FolderOpen } from "@lucide/vue";
import { api } from "@/api";
import type { BackupProgressView, BackupSettings } from "@/api/contract";
import UiButton from "@/components/ui/UiButton.vue";
import SettingsSection from "@/components/settings/SettingsSection.vue";
import SettingsField from "@/components/settings/SettingsField.vue";
import SettingsNote from "@/components/settings/SettingsNote.vue";
import { formatBytes } from "@/bytes";
import { useAppSettings } from "@/composables/useAppSettings";
import { useUiStore } from "@/stores/ui";

/** Backing the library up to one file, restoring one (Aoide restarts to put it in place), and
 *  automatic backups in a folder. What goes in and what a restore replaces is the core's
 *  (core/backup.rs); this asks and shows. */
const ui = useUiStore();
const { settings, save } = useAppSettings();
const withBooks = ref(false),
  busy = ref(false),
  message = ref(""),
  error = ref(""),
  progress = ref<BackupProgressView | null>(null);
let off: (() => void) | undefined,
  mounted = true;
onMounted(() => {
  void api
    .onBackupProgress((p) => {
      if (!p.automatic) progress.value = p;
    })
    .then((stop) => (mounted ? (off = stop) : stop()));
});
onBeforeUnmount(() => {
  mounted = false;
  off?.();
});
const percent = computed(() => {
  const p = progress.value;
  return p && p.of ? Math.round((p.done / p.of) * 100) : 0;
});

async function backUp() {
  const path = await api.chooseBackupTarget().catch(() => null);
  if (!path) return;
  busy.value = true;
  message.value = error.value = "";
  try {
    const written = await api.backUp({ path, options: { books: withBooks.value } });
    message.value = `Backup written (${formatBytes(written.bytes)}).`;
  } catch (e) {
    error.value = String(e).replace(/^Error: /, "");
  } finally {
    busy.value = false;
    progress.value = null;
  }
}

async function restore() {
  const path = await api.chooseBackupFile().catch(() => null);
  if (!path) return;
  message.value = error.value = "";
  try {
    const info = await api.inspectBackup({ path });
    if (info.refused) {
      error.value = info.refused;
      return;
    }
    const made = new Date(info.createdAt).toLocaleString(undefined, { dateStyle: "long", timeStyle: "short" });
    const ok = await ui.confirm({
      title: "Restore this backup?",
      body:
        `The backup from ${made} (Aoide ${info.aoideVersion}) replaces your library, covers, saved articles and ` +
        `settings. Your current library is saved first. ` +
        (info.booksIncluded
          ? "Its books are added to the library folder. "
          : "It holds no books: they show as missing until their files are in the library folder. ") +
        "Aoide restarts to finish.",
      confirmLabel: "Restore and restart",
      danger: true,
    });
    if (!ok) return;
    busy.value = true;
    await api.restoreBackup({ path });
  } catch (e) {
    error.value = String(e).replace(/^Error: /, "");
  } finally {
    busy.value = false;
  }
}

/** Automatic backups, changed one field at a time. */
const automatic = computed(() => settings.value?.backup ?? null);
function saveAutomatic(change: Partial<BackupSettings>) {
  if (automatic.value) void save({ backup: { ...automatic.value, ...change } });
}
async function chooseFolder() {
  const folder = await api.chooseFolder({ title: "Choose a folder for automatic backups" }).catch(() => null);
  if (folder) saveAutomatic({ folder });
}
const whole = (text: string, min: number, max: number) => {
  const n = Math.round(Number(text));
  return Number.isFinite(n) ? Math.min(max, Math.max(min, n)) : min;
};
</script>

<template>
  <div class="backup">
    <SettingsSection title="Backup" :icon="DatabaseBackup">
      <SettingsNote
        >A backup holds your library: progress, bookmarks, highlights and notes, collections, settings and saved
        articles. It is not encrypted, so keep it somewhere private. Sign-ins to sites are never in it, and generated
        audio is made again.</SettingsNote
      >
      <label class="backup__check"
        ><input v-model="withBooks" type="checkbox" aria-label="Include the books" /> Include the books (they can be
        large)</label
      >
      <div class="backup__actions">
        <UiButton size="sm" aria-label="Back up the library" :disabled="busy" @click="backUp">
          <DatabaseBackup :size="14" aria-hidden="true" /> Back up…
        </UiButton>
        <UiButton size="sm" variant="outline" aria-label="Restore a backup" :disabled="busy" @click="restore">
          <ArchiveRestore :size="14" aria-hidden="true" /> Restore…
        </UiButton>
      </div>
      <div
        v-if="progress && !progress.finished"
        class="backup__track"
        role="progressbar"
        aria-label="Writing the backup"
        :aria-valuenow="percent"
        aria-valuemin="0"
        aria-valuemax="100"
      >
        <i :style="{ width: `${percent}%` }" />
      </div>
      <p v-if="message" class="backup__message" role="status">{{ message }}</p>
      <p v-if="error" class="backup__error" role="alert">{{ error }}</p>
    </SettingsSection>

    <SettingsSection v-if="automatic" title="Automatic backups">
      <SettingsField label="Folder" as="div">
        <span class="backup__folder">{{ automatic.folder ?? "Off" }}</span>
        <UiButton size="sm" aria-label="Choose a folder for automatic backups" @click="chooseFolder">
          <FolderOpen :size="14" aria-hidden="true" /> Choose…
        </UiButton>
        <UiButton
          v-if="automatic.folder"
          size="sm"
          variant="outline"
          aria-label="Turn automatic backups off"
          @click="saveAutomatic({ folder: null })"
          >Turn off</UiButton
        >
      </SettingsField>
      <template v-if="automatic.folder">
        <SettingsField label="Every"
          ><span class="backup__unit"
            ><input
              type="number"
              min="1"
              max="365"
              :value="automatic.everyDays"
              aria-label="Days between automatic backups"
              @change="saveAutomatic({ everyDays: whole(($event.target as HTMLInputElement).value, 1, 365) })"
            />
            days</span
          ></SettingsField
        >
        <SettingsField label="Keep"
          ><span class="backup__unit"
            ><input
              type="number"
              min="1"
              max="100"
              :value="automatic.keep"
              aria-label="Automatic backups kept"
              @change="saveAutomatic({ keep: whole(($event.target as HTMLInputElement).value, 1, 100) })"
            />
            backups</span
          ></SettingsField
        >
        <label class="backup__check"
          ><input
            type="checkbox"
            :checked="automatic.books"
            aria-label="Include the books in automatic backups"
            @change="saveAutomatic({ books: ($event.target as HTMLInputElement).checked })"
          />
          Include the books</label
        >
      </template>
      <SettingsNote
        >Point it at a synced folder to carry your library to another computer: restore the newest backup there. The
        oldest are removed beyond the number kept; nothing else in the folder is touched.</SettingsNote
      >
    </SettingsSection>
  </div>
</template>

<style scoped>
.backup {
  display: grid;
  gap: var(--space-11);
  padding-top: var(--space-11);
  border-top: 1px solid var(--line);
}
.backup__check {
  display: flex;
  align-items: center;
  gap: var(--space-5);
  font-size: 13px;
}
.backup__actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-4);
}
.backup__track {
  position: relative;
  height: var(--space-2);
  border-radius: var(--radius-pill);
  background: var(--line);
  overflow: hidden;
}
.backup__track i {
  position: absolute;
  inset: 0 auto 0 0;
  background: var(--accent);
}
.backup__message {
  font-size: 12.5px;
  color: var(--dim);
}
.backup__error {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--accent);
}
.backup__folder {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
}
.backup__unit {
  display: inline-flex;
  align-items: center;
  gap: var(--space-4);
  color: var(--dim);
}
.backup__unit input {
  width: 6em;
}
</style>
