import { defineStore } from "pinia";
import { ref } from "vue";
import { api } from "@/api";
import type { LinkChoice, LinkImportView } from "@/api/contract";
import { useLibraryStore } from "@/stores/library";

const IDLE: LinkImportView = {
  stage: "idle",
  source: null,
  found: 0,
  unread: 0,
  already: 0,
  duplicates: 0,
  notWeb: 0,
  of: 0,
  ready: 0,
  partial: 0,
  failed: 0,
};

/** The import of links from another app, as the core reports it ("links://import"). The core
 *  decides every stage and count (core/link_import.rs); this shows them and asks. */
export const useLinkImportStore = defineStore("linkImport", () => {
  const view = ref<LinkImportView>({ ...IDLE });
  const error = ref("");
  let startup: Promise<void> | undefined;
  function start() {
    return (startup ??= (async () => {
      await api.onLinkImport((v) => (view.value = v));
      view.value = await api.linkImport();
    })());
  }
  async function run(step: () => Promise<LinkImportView | void>) {
    error.value = "";
    try {
      const next = await step();
      if (next) view.value = next;
    } catch (e) {
      error.value = String(e).replace(/^Error: /, "");
    }
  }
  /** Asks for an export and reads it; the dialog then offers the choice. */
  async function choose() {
    const path = await api.chooseLinksFile().catch(() => null);
    if (path) await run(() => api.readLinks({ path }));
    return Boolean(path);
  }
  const begin = (choice: LinkChoice) =>
    run(async () => {
      const next = await api.startLinkImport({ choice });
      await useLibraryStore().refresh();
      return next;
    });
  const pause = (paused: boolean) => run(() => api.pauseLinkImport({ paused }));
  const cancel = () => run(() => api.cancelLinkImport());
  const retryArchived = () => run(() => api.retryLinkImportArchived());
  const dismiss = () =>
    run(async () => {
      await api.dismissLinkImport();
      view.value = { ...IDLE };
    });
  return { view, error, start, choose, begin, pause, cancel, retryArchived, dismiss };
});
