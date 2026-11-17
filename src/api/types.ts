/** The UI↔core contract. Everything that crosses the IPC seam is declared from the Rust types in
 *  `contract.ts` (generated; see src-tauri/src/core/contract.rs), commands and their argument and
 *  result types included. The Tauri adapter reshapes a few types (cover paths become webview
 *  URLs); those are defined from their wire types here. */
import type * as Wire from "@/api/contract";

import type {
  CommandApi,
  AlignerSettings,
  AppInfo,
  AppSettings,
  AppSettingsPatch,
  Appearance,
  ArticleSource,
  BrowserDownloadView,
  BrowserStatusView,
  SiteData,
  SiteFetch,
  SiteView,
  SignInView,
  SavedLink,
  InboxBook,
  InboxView,
  FinishedItem,
  BackupProgressView,
  LinkImportView,
  ArticleView,
  AddedLink,
  EntryFilter,
  OpmlImport,
  EntryView,
  FeedView,
  SubscribeResult,
  AppearanceMode,
  BlockKind,
  BlockView,
  BookDetails,
  BookId,
  BookProgress,
  BookSettings,
  BookStatus,
  BookStorage,
  BookView,
  Bookmark,
  CandidateStatus,
  ChapterAudioView,
  ChapterSummary,
  ChoiceOption,
  Collection,
  DayListening,
  DiscoveryImportProgress,
  EngineDescriptor,
  EngineOptions,
  EngineProfile,
  EngineState,
  EngineStatusView,
  ServerView,
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
  ReaderFont,
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
  ReadingMode,
  ActivityStats,
  SyncAction,
  SyncReportView,
  SyncStage,
} from "@/api/contract";

// Every contract type, under its own name (the two reshaped above it are declared here instead).
export type * from "@/api/contract";

/** A library book; the adapter turns the cover file path into a URL the webview can load. */
export type BookSummary = Omit<Wire.BookSummary, "coverPath"> & { coverUrl: string | null };
/** A picture between a chapter's blocks, its file as a URL the webview can load. */
export type InsertView = Omit<Wire.InsertView, "path"> & { url: string };
/** A chapter as the page draws it, its pictures given as URLs. */
export type ChapterView = Omit<Wire.ChapterView, "inserts"> & { inserts: InsertView[] };
/** An EPUB found on this computer, with its cover as a webview URL. */
export type Candidate = Omit<Wire.Candidate, "coverPath"> & { coverUrl: string | null };

/** Everything the UI can ask of the core. Implemented over Tauri IPC and by a browser mock. */
export interface AoideApi extends Omit<CommandApi, "listBooks" | "scanForBooks" | "discoveredBooks" | "chapter"> {
  // Every command comes from the contract (CommandApi); these give covers and pictures as URLs
  // the window can show.
  listBooks(): Promise<BookSummary[]>;
  chapter(args: { bookId: string; chapter: number }): Promise<ChapterView>;
  /** Fires when files in the config folder change (hand edits included). */
  onConfigChanged(handler: (preferences: Preferences) => void): Promise<() => void>;
  /** Scans the configured folders; resolves with everything found. */
  scanForBooks(): Promise<Candidate[]>;
  /** Results of the last scan, compared with the library. */
  discoveredBooks(): Promise<Candidate[]>;
  onScanProgress(handler: (progress: ScanProgress) => void): Promise<() => void>;
  onBrowserDownload(handler: (download: BrowserDownloadView) => void): Promise<() => void>;
  onSignInState(handler: (signIn: SignInView) => void): Promise<() => void>;
  /** Fires when feeds or their entries change (a check, reading, saving, cleanup). */
  onFeedsChanged(handler: () => void): Promise<() => void>;
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
  onSyncState(handler: (recording: RecordingView) => void): Promise<() => void>;
  onArticleState(handler: (article: ArticleView) => void): Promise<() => void>;
  onModelDownload(handler: (download: ModelDownloadView) => void): Promise<() => void>;
  /** A failure in the background the user should know about. */
  onProblem(handler: (problem: ProblemView) => void): Promise<() => void>;
  /** Asks for an item's page (a desktop notification about it was clicked). */
  onOpenItem(handler: (id: string) => void): Promise<() => void>;
  /** A page sent from the browser (`aoide://save?url=…`) was saved. */
  onLinkSaved(handler: (saved: SavedLink) => void): Promise<() => void>;
  /** Books that appeared in the inbox folder were added to the library. */
  onInboxAdded(handler: (books: InboxBook[]) => void): Promise<() => void>;
  /** The inbox folder was chosen, went or came back. */
  onInboxState(handler: (inbox: InboxView) => void): Promise<() => void>;
  /** An article was just finished: the window offers to archive it. */
  onFinished(handler: (item: FinishedItem) => void): Promise<() => void>;
  /** Items were added to the queue, moved in it or taken off it (one that starts leaves it). */
  onQueueChanged(handler: () => void): Promise<() => void>;
  /** The window now fills the screen, or no longer does (F11, Esc or the window manager). */
  onFullscreen(handler: (on: boolean) => void): Promise<() => void>;
  /** The window was closed while working: ask whether to keep working (`chooseClose`). */
  onCloseAsked(handler: () => void): Promise<() => void>;
  /** An import of links moved on. */
  onLinkImport(handler: (view: LinkImportView) => void): Promise<() => void>;
  /** A backup is being written, or was. */
  onBackupProgress(handler: (progress: BackupProgressView) => void): Promise<() => void>;
}
