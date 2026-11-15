import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type * as Wire from "./contract";
import type { Commands, CoreEvents } from "./contract";
import type { AoideApi, Candidate, UiLog } from "./types";
import { report } from "../log";
import { apiError } from "./errors";

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
  "choose_books",
  "choose_audio",
]);
/** Answers slower than this go to the log. */
const SLOW_MS = 2_000;
const writeLog = (entry: UiLog) => invoke<void>("log_ui", { entry });

/** Calls one of Core's commands; its name, arguments and result come from the contract.
 *  A failure rejects with an ApiError; it (other than a cancelled scan), or a slow answer, goes
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
    if (!error.message.includes("cancelled"))
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

export const tauriApi: AoideApi = {
  logUi: (entry) => call("log_ui", { entry }),
  appInfo: () => call("app_info"),
  listBooks: async () =>
    (await call("list_books")).map(({ coverPath, ...book }) => ({
      ...book,
      coverUrl: coverPath ? convertFileSrc(coverPath) : null,
    })),
  home: () => call("home"),
  importFiles: (paths) => call("import_files", { paths }),
  addAudiobook: (paths) => call("add_audiobook", { paths }),
  rescanLibrary: () => call("rescan_library"),
  removeBook: (bookId) => call("remove_book", { bookId }),
  showInFolder: (bookId) => call("show_in_folder", { bookId }),
  collections: () => call("collections"),
  createCollection: (name) => call("create_collection", { name }),
  renameCollection: (id, name) => call("rename_collection", { id, name }),
  deleteCollection: (id) => call("delete_collection", { id }),
  setBookCollections: (bookId, ids) => call("set_book_collections", { bookId, ids }),
  openBook: (bookId) => call("open_book", { bookId }),
  bookOverview: (bookId) => call("book_overview", { bookId }),
  setMode: (bookId, mode) => call("set_mode", { bookId, mode }),
  markFinished: (bookId) => call("mark_finished", { bookId }),
  markUnread: (bookId) => call("mark_unread", { bookId }),
  setReadingPlace: (bookId, chapter, segmentId) => call("set_reading_place", { bookId, chapter, segmentId }),
  readingActivity: (bookId) => call("reading_activity", { bookId }),
  chapter: (bookId, chapter) => call("chapter", { bookId, chapter }),
  play: (bookId, chapter, segmentId) => call("play", { bookId, chapter, segmentId }),
  pause: () => call("pause"),
  resume: () => call("resume"),
  stop: () => call("stop"),
  setSpeed: (speed) => call("set_speed", { speed }),
  setVolume: (volume) => call("set_volume", { volume }),
  engineStatus: () => call("engine_status"),
  engineOptions: () => call("engine_options"),
  engineVoices: () => call("engine_voices"),
  previewVoice: (profile, voiceId, params, language) => call("preview_voice", { profile, voiceId, params, language }),
  saveEngineProfile: (profile) => call("save_engine_profile", { profile }),
  bookSettings: (bookId) => call("book_settings", { bookId }),
  saveBookSettings: (bookId, settings) => call("save_book_settings", { bookId, settings }),
  prepareBook: (bookId) => call("prepare_book", { bookId }),
  cancelPrepare: (bookId) => call("cancel_prepare", { bookId }),
  prepareChapter: (bookId, chapter) => call("prepare_chapter", { bookId, chapter }),
  chapterAudio: (bookId) => call("chapter_audio", { bookId }),
  setPreparePaused: (bookId, paused) => call("set_prepare_paused", { bookId, paused }),
  regenerateBook: (bookId) => call("regenerate_book", { bookId }),
  storageReport: () => call("storage_report"),
  cleanUnusedFiles: () => call("clean_unused_files"),
  setSleepTimer: (minutes, endOfChapter) => call("set_sleep_timer", { minutes, endOfChapter }),
  searchBook: (bookId, query) => call("search_book", { bookId, query }),
  skipChapter: (delta) => call("skip_chapter", { delta }),
  seek: (bookId, positionMs) => call("seek", { bookId, positionMs }),
  skip: (bookId, deltaMs) => call("skip", { bookId, deltaMs }),
  bookmarks: (bookId) => call("bookmarks", { bookId }),
  addBookmark: (bookId, note) => call("add_bookmark", { bookId, note }),
  updateBookmarkNote: (id, note) => call("update_bookmark_note", { id, note }),
  deleteBookmark: (id) => call("delete_bookmark", { id }),
  activityStats: () => call("activity_stats"),
  bookListening: (bookId) => call("book_listening", { bookId }),
  bookDetails: (bookId) => call("book_details", { bookId }),
  lookupMetadata: (bookId, refresh) => call("lookup_metadata", { bookId, refresh }),
  applyMetadata: (bookId, match) => call("apply_metadata", { bookId, candidate: match }),
  appSettings: () => call("app_settings"),
  updateAppSettings: (patch) => call("update_app_settings", { patch }),
  preferences: () => call("preferences"),
  saveAppearance: (appearance) => call("save_appearance", { appearance }),
  onConfigChanged: (h) => on("config://changed", h),
  exportBook: (bookId) => call("export_book", { bookId }),
  exportReadAlong: (bookId) => call("export_read_along", { bookId }),
  scanForBooks: async () => candidates(await call("scan_for_books")),
  cancelScan: () => call("cancel_scan"),
  discoveredBooks: async () => candidates(await call("discovered_books")),
  fetchDiscoveredDetails: (ids) => call("fetch_discovered_details", { ids }),
  importDiscovered: (choices) => call("import_discovered", { choices }),
  onScanProgress: (h) => on("discovery://progress", h),
  scanForAudiobooks: () => call("scan_for_audiobooks"),
  audiobookMatches: () => call("audiobook_matches"),
  unmatchedAudiobooks: () => call("unmatched_audiobooks"),
  cancelAudiobookScan: () => call("cancel_audiobook_scan"),
  lookForAudiobooks: (ids) => call("look_for_audiobooks", { ids }),
  pairAudiobooks: (choices) => call("pair_audiobooks", { choices }),
  onAudiobookScan: (h) => on("audiobooks://progress", h),
  onAudiobookMatches: (h) => on("audiobooks://matches", h),
  onDiscoveryDetails: (h) => on("discovery://details", h),
  onDiscoveryImport: (h) => on("discovery://import", h),
  onPlayerState: (h) => on("player://state", h),
  onEngineStatus: (h) => on("engine://status", h),
  onGenerationState: (h) => on("generation://state", h),
  onExportProgress: (h) => on("export://progress", h),
  onLibraryChanged: (h) => on("library://changed", () => h()),
  inspectRecording: (paths) => call("inspect_recording", { paths }),
  attachRecording: (bookId, paths, asListed = false) => call("attach_recording", { bookId, paths, asListed }),
  recording: (bookId) => call("recording", { bookId }),
  recordings: () => call("recordings"),
  syncReport: (bookId) => call("sync_report", { bookId }),
  syncAction: (bookId, action) => call("sync_action", { bookId, action }),
  removeRecording: (bookId) => call("remove_recording", { bookId }),
  setNarration: (bookId, narration) => call("set_narration", { bookId, narration }),
  recognitionOptions: () => call("recognition_options"),
  downloadModel: (modelId) => call("download_model", { modelId }),
  chooseAudio: (folder) => call("choose_audio", { folder }),
  chooseBooks: () => call("choose_books"),
  playRecordingAt: (bookId, atMs) => call("play_recording_at", { bookId, atMs }),
  onSyncState: (h) => on("sync://state", h),
  onModelDownload: (h) => on("model://download", h),
  onProblem: (h) => on("core://problem", h),
};
