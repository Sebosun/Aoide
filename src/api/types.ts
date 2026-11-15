/** The UI↔core contract. Everything that crosses the IPC seam is declared from the Rust types in
 *  `contract.ts` (generated; see src-tauri/src/core/contract.rs), commands and their argument and
 *  result types included. The Tauri adapter reshapes a few types (cover paths become webview
 *  URLs); those are defined from their wire types here. */
import type * as Wire from "./contract";

import type {
  AlignerSettings,
  AppInfo,
  AppSettings,
  AppSettingsPatch,
  Appearance,
  AppearanceMode,
  BlockKind,
  BlockView,
  BookDetails,
  BookId,
  BookListeningView,
  BookProgress,
  BookSettings,
  BookStatus,
  BookStorage,
  BookView,
  Bookmark,
  CandidateStatus,
  ChapterAudioView,
  ChapterSummary,
  ChapterView,
  ChoiceOption,
  Collection,
  DayListening,
  DiscoveryImportProgress,
  EngineDescriptor,
  EngineOptions,
  EngineProfile,
  EngineState,
  EngineStatusView,
  ExcludedView,
  ExportProgress,
  ExportStage,
  GenerationState,
  HomeView,
  ImportResult,
  AudiobookSuggestion,
  AudiobookMatch,
  AudiobookChoice,
  AudiobookSearchSettings,
  MatchStage,
  PairOutcome,
  LaunchCommand,
  LibrarySort,
  ListeningStats,
  MetadataMatch,
  NetworkScope,
  NoteMarker,
  ParamKind,
  ParamSpec,
  ParamValue,
  PlaybackStatus,
  PlayerState,
  Preferences,
  PrepareState,
  PrepareView,
  PreparingView,
  ReaderMode,
  ScanProgress,
  ScanSettings,
  ScanStage,
  SearchHit,
  SegmentView,
  SleepView,
  StorageReport,
  Theme,
  ProblemView,
  UiLog,
  Voice,
  ChapterMatchView,
  DownloadStage,
  InspectionView,
  MatchSummaryView,
  ModelDownloadView,
  ModelSizeChoice,
  ModelView,
  Narration,
  OffTextView,
  RecognitionDevice,
  RecognitionEngine,
  RecognitionServer,
  RecognitionOptions,
  RecognitionSettings,
  RecordingFileView,
  RecordingProblemView,
  RecordingView,
  SyncAction,
  SyncReportView,
  SyncStage,
} from "./contract";

// Every contract type, under its own name (the two reshaped above it are declared here instead).
export type * from "./contract";

/** A library book; the adapter turns the cover file path into a URL the webview can load. */
export type BookSummary = Omit<Wire.BookSummary, "coverPath"> & { coverUrl: string | null };
/** An EPUB found on this computer, with its cover as a webview URL. */
export type Candidate = Omit<Wire.Candidate, "coverPath"> & { coverUrl: string | null };

/** Everything the UI can ask of the core. Implemented over Tauri IPC and by a browser mock. */
export interface AoideApi {
  /** Writes a line to Aoide's log (a failure the window saw). */
  logUi(entry: UiLog): Promise<void>;
  appInfo(): Promise<AppInfo>;
  listBooks(): Promise<BookSummary[]>;
  /** Home's lists, as ids of library items. */
  home(): Promise<HomeView>;
  /** Copies dropped EPUB files into the library folder and imports them. */
  importFiles(paths: string[]): Promise<ImportResult[]>;
  rescanLibrary(): Promise<void>;
  removeBook(bookId: string): Promise<void>;
  /** Opens the file manager on the book's folder with its EPUB selected. */
  showInFolder(bookId: string): Promise<void>;
  /** By name. */
  collections(): Promise<Collection[]>;
  /** Fails when the name is empty or already used (ignoring case). Returns the new id. */
  createCollection(name: string): Promise<number>;
  renameCollection(id: number, name: string): Promise<void>;
  /** The books stay in the library. */
  deleteCollection(id: number): Promise<void>;
  /** Puts the book on exactly these collections. */
  setBookCollections(bookId: string, ids: number[]): Promise<void>;
  openBook(bookId: string): Promise<BookView>;
  /** Like openBook, without marking the book as recently opened. */
  bookOverview(bookId: string): Promise<BookView>;
  chapter(bookId: string, chapter: number): Promise<ChapterView>;
  /** Starts playback at a segment (or the saved position when segmentId is null). */
  play(bookId: string, chapter: number | null, segmentId: string | null): Promise<void>;
  pause(): Promise<void>;
  resume(): Promise<void>;
  stop(): Promise<void>;
  setSpeed(speed: number): Promise<void>;
  /** Output gain, 0–1; saved with the speed. */
  setVolume(volume: number): Promise<void>;
  engineStatus(): Promise<EngineStatusView>;
  /** Registered adapters and the active global profile. */
  engineOptions(): Promise<{ descriptors: EngineDescriptor[]; profile: EngineProfile }>;
  /** Voices of the active engine (fails when the engine is offline). */
  engineVoices(): Promise<Voice[]>;
  /** Speaks a sample sentence with `profile` (saved or not) and returns a WAV file. `params`
   *  override the profile's; a null voice means the profile's voice, else the engine's first. */
  previewVoice(
    profile: EngineProfile,
    voiceId: string | null,
    params: Record<string, ParamValue>,
    language: string | null,
  ): Promise<ArrayBuffer>;
  /** Validates and activates a profile; regenerates upcoming audio with it. */
  saveEngineProfile(profile: EngineProfile): Promise<void>;
  bookSettings(bookId: string): Promise<BookSettings>;
  saveBookSettings(bookId: string, settings: BookSettings): Promise<void>;
  /** Queue whole-book generation (progress arrives via BookSummary.prepare + library://changed). */
  prepareBook(bookId: string): Promise<void>;
  cancelPrepare(bookId: string): Promise<void>;
  /** Queues one chapter (from 0); it joins the book's entry, or is already covered by a whole book. */
  prepareChapter(bookId: string, chapter: number): Promise<void>;
  /** How much of each chapter's audio is made, in chapter order. */
  chapterAudio(bookId: string): Promise<ChapterAudioView[]>;
  /** Pause or resume a queued book's preparation (remembered across restarts). */
  setPreparePaused(bookId: string, paused: boolean): Promise<void>;
  /** Delete the book's generated audio so it is regenerated. */
  regenerateBook(bookId: string): Promise<void>;
  storageReport(): Promise<StorageReport>;
  /** Deletes unused files (see StorageReport) and returns what was freed. */
  cleanUnusedFiles(): Promise<{ files: number; bytes: number }>;
  /** minutes: pause after that long; endOfChapter: pause at the chapter end; neither: cancel. */
  setSleepTimer(minutes: number | null, endOfChapter: boolean): Promise<void>;
  /** Case-insensitive text search (min 2 chars), at most 200 hits in reading order. */
  searchBook(bookId: string, query: string): Promise<SearchHit[]>;
  /** Jump to the start of the next (+1) or previous (-1) chapter of the playing book. */
  skipChapter(delta: number): Promise<void>;
  /** Bookmarks of a book, in reading order. */
  bookmarks(bookId: string): Promise<Bookmark[]>;
  /** Bookmarks the current playhead (or the saved position of bookId when not playing it). */
  addBookmark(bookId: string, note: string): Promise<Bookmark>;
  updateBookmarkNote(id: number, note: string): Promise<void>;
  deleteBookmark(id: number): Promise<void>;
  listeningStats(): Promise<ListeningStats>;
  /** One book's listened time per local day, oldest first. */
  bookListening(bookId: string): Promise<DayListening[]>;
  bookDetails(bookId: string): Promise<BookDetails>;
  /** Searches Open Library for the book (fails when lookups are turned off). */
  /** Cached for 30 days; refresh asks Open Library again. */
  lookupMetadata(bookId: string, refresh: boolean): Promise<MetadataMatch[]>;
  /** Stores the picked match's details (and cover when the book has none). */
  applyMetadata(bookId: string, match: MetadataMatch): Promise<BookDetails>;
  appSettings(): Promise<AppSettings>;
  /** Merges a change into the app settings; resolves with the settings now saved. */
  updateAppSettings(patch: AppSettingsPatch): Promise<AppSettings>;
  /** Appearance, themes and config problems from ~/.config/aoide. */
  preferences(): Promise<Preferences>;
  saveAppearance(appearance: Appearance): Promise<void>;
  /** Fires when files in the config folder change (hand edits included). */
  onConfigChanged(handler: (preferences: Preferences) => void): Promise<() => void>;
  /** Writes an M4B audiobook (generating missing audio first); resolves with the output path. */
  exportBook(bookId: string): Promise<string>;
  /** Writes the book with its synced recording as an EPUB 3 with Media Overlays; returns its path. */
  exportReadAlong(bookId: string): Promise<string>;
  /** Scans the configured folders; resolves with everything found. */
  scanForBooks(): Promise<Candidate[]>;
  cancelScan(): Promise<void>;
  /** Results of the last scan, compared with the library. */
  discoveredBooks(): Promise<Candidate[]>;
  /** Open Library lookups for these candidates; "discovery://details" fires per id. */
  fetchDiscoveredDetails(ids: string[]): Promise<void>;
  /** `pairAudiobook` pairs the audiobook found beside a book once it is imported. */
  importDiscovered(
    choices: { id: string; applySuggestion: boolean; pairAudiobook: boolean }[],
  ): Promise<ImportResult[]>;
  onScanProgress(handler: (progress: ScanProgress) => void): Promise<() => void>;
  /** Searches the audiobook folders for every book without a recording; with verifying on,
   *  Aoide starts pairing them itself. "audiobooks://progress" reports the search. */
  scanForAudiobooks(): Promise<AudiobookMatch[]>;
  /** The matches the last searches found, and where each stands. */
  audiobookMatches(): Promise<AudiobookMatch[]>;
  cancelAudiobookScan(): Promise<void>;
  /** After books were added: how many have audiobooks waiting for the user (0 when looking on
   *  adding is off or Aoide verifies them). */
  lookForAudiobooks(ids: string[]): Promise<number>;
  /** Pairs each chosen candidate; each succeeds or fails on its own. */
  pairAudiobooks(choices: AudiobookChoice[]): Promise<PairOutcome[]>;
  onAudiobookScan(handler: (progress: ScanProgress) => void): Promise<() => void>;
  onAudiobookMatches(handler: (matches: AudiobookMatch[]) => void): Promise<() => void>;
  onDiscoveryDetails(handler: (id: string) => void): Promise<() => void>;
  onDiscoveryImport(handler: (progress: DiscoveryImportProgress) => void): Promise<() => void>;
  onPlayerState(handler: (state: PlayerState) => void): Promise<() => void>;
  onEngineStatus(handler: (status: EngineStatusView) => void): Promise<() => void>;
  onGenerationState(handler: (state: GenerationState) => void): Promise<() => void>;
  onExportProgress(handler: (progress: ExportProgress) => void): Promise<() => void>;
  /** Fires when books are added/changed by the folder watcher or imports. */
  onLibraryChanged(handler: () => void): Promise<() => void>;
  /** What dropped or chosen audio files hold, before pairing them with a book. */
  inspectRecording(paths: string[]): Promise<InspectionView>;
  /** Pairs audio files with a book (replacing its recording) and starts syncing them. */
  /** `asListed`: the files stay in the order given instead of being sorted. */
  attachRecording(bookId: string, paths: string[], asListed?: boolean): Promise<RecordingView>;
  recording(bookId: string): Promise<RecordingView | null>;
  /** Every book's recording, oldest first. */
  recordings(): Promise<RecordingView[]>;
  /** How well the recording matched the text (fails until it is matched). */
  syncReport(bookId: string): Promise<SyncReportView>;
  syncAction(bookId: string, action: SyncAction): Promise<void>;
  /** Forgets the recording and its sync; the audio files are left alone. */
  removeRecording(bookId: string): Promise<void>;
  /** Plays the book with its recording (once synced) or a speech engine. */
  setNarration(bookId: string, narration: Narration): Promise<void>;
  recognitionOptions(): Promise<RecognitionOptions>;
  /** Downloads a speech-recognition model (progress on "model://download"). */
  downloadModel(modelId: string): Promise<void>;
  /** The system's file chooser for EPUB files to add; null when cancelled. */
  chooseBooks(): Promise<string[] | null>;
  /** The system's file chooser for audiobook files (or a folder); null when cancelled. */
  chooseAudio(folder: boolean): Promise<string[] | null>;
  /** Plays the book's recording from a time on its timeline (needs the recording narration). */
  playRecordingAt(bookId: string, atMs: number): Promise<void>;
  onSyncState(handler: (recording: RecordingView) => void): Promise<() => void>;
  onModelDownload(handler: (download: ModelDownloadView) => void): Promise<() => void>;
  /** A failure in the background the user should know about. */
  onProblem(handler: (problem: ProblemView) => void): Promise<() => void>;
}
