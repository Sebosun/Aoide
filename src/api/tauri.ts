import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type * as Wire from "@/api/contract";
import { COMMANDS, type CommandApi, type Commands, type CoreEvents } from "@/api/contract";
import type { AoideApi, Candidate, UiLog } from "@/api/types";
import { report } from "@/log";
import { apiError } from "@/api/errors";

const candidates = (list: Wire.Candidate[]): Candidate[] =>
  list.map(({ coverPath, ...c }) => ({ ...c, coverUrl: coverPath ? convertFileSrc(coverPath) : null }));
/** Jobs that log their own time in Core, or wait for the user (a file chooser). */
const LONG = new Set<keyof Commands>([
  "import_files",
  "rescan_library",
  "export_book",
  "export_read_along",
  "scan_for_books",
  "scan_for_audiobooks",
  "import_discovered",
  "download_model",
  "download_browser",
  "choose_books",
  "choose_audio",
  "choose_notes_target",
  "choose_notes_folder",
  "export_all_notes",
]);
/** Answers slower than this go to the log. */
const SLOW_MS = 2_000;
const writeLog = (entry: UiLog) => invoke<void>("log_ui", { entry });

/** Calls one of Core's commands; its name, arguments and result come from the contract.
 *  A failure rejects with an ApiError; it (other than a scan the user cancelled), or a slow answer, goes
 *  to Aoide's log too. */
async function call<K extends keyof Commands>(
  name: K,
  ...args: Commands[K]["args"] extends Record<string, never> ? [] : [Commands[K]["args"]]
): Promise<Commands[K]["result"]> {
  if (name === "log_ui") return invoke(name, ...args);
  const start = Date.now();
  try {
    const result = await invoke<Commands[K]["result"]>(name, ...args);
    const tookMs = Date.now() - start;
    if (tookMs > SLOW_MS && !LONG.has(name))
      report({ level: "info", event: "ipc.slow", msg: "Core answered slowly", command: name, tookMs }, writeLog);
    return result;
  } catch (rejected) {
    const error = apiError(rejected);
    if (error.kind !== "cancelled")
      report(
        { level: "warn", event: "ipc.error", msg: error.message, command: name, tookMs: Date.now() - start },
        writeLog,
      );
    throw error;
  }
}
/** Subscribes to one of Core's events; the name and payload type come from the contract. */
const on = <K extends keyof CoreEvents>(name: K, handler: (payload: CoreEvents[K]) => void) =>
  listen<CoreEvents[K]>(name, (event) => handler(event.payload));

/** Every command, as a method that sends its arguments (named as in the contract) as they are. */
const commands = Object.fromEntries(
  Object.entries(COMMANDS).map(([method, name]) => [
    method,
    (args?: Record<string, unknown>) =>
      (call as (name: keyof Commands, ...args: [Record<string, unknown>?]) => Promise<unknown>)(
        name,
        ...(args ? [args] : []),
      ),
  ]),
) as unknown as CommandApi;

export const tauriApi: AoideApi = {
  ...commands,
  listBooks: async () =>
    (await commands.listBooks()).map(({ coverPath, ...book }) => ({
      ...book,
      coverUrl: coverPath ? convertFileSrc(coverPath) : null,
    })),
  scanForBooks: async () => candidates(await commands.scanForBooks()),
  discoveredBooks: async () => candidates(await commands.discoveredBooks()),
  onConfigChanged: (h) => on("config://changed", h),
  onScanProgress: (h) => on("discovery://progress", h),
  onBrowserDownload: (h) => on("browser://download", h),
  onSignInState: (h) => on("signin://state", h),
  onFeedsChanged: (h) => on("feeds://changed", () => h()),
  onAudiobookScan: (h) => on("audiobooks://progress", h),
  onAudiobookMatches: (h) => on("audiobooks://matches", h),
  onDiscoveryDetails: (h) => on("discovery://details", h),
  onDiscoveryImport: (h) => on("discovery://import", h),
  onPlayerState: (h) => on("player://state", h),
  onEngineStatus: (h) => on("engine://status", h),
  onGenerationState: (h) => on("generation://state", h),
  onExportProgress: (h) => on("export://progress", h),
  onLibraryChanged: (h) => on("library://changed", () => h()),
  onSyncState: (h) => on("sync://state", h),
  onArticleState: (h) => on("article://state", h),
  onModelDownload: (h) => on("model://download", h),
  onProblem: (h) => on("core://problem", h),
  onOpenItem: (h) => on("item://open", h),
  onLinkSaved: (h) => on("article://saved", h),
  onInboxAdded: (h) => on("inbox://added", h),
  onInboxState: (h) => on("inbox://state", h),
  onFinished: (h) => on("item://finished", h),
  onQueueChanged: (h) => on("queue://changed", () => h()),
  onFullscreen: (h) => on("window://fullscreen", h),
  onCloseAsked: (h) => on("window://close-asked", () => h()),
  onBackupProgress: (h) => on("backup://progress", h),
  onLinkImport: (h) => on("links://import", h),
};
