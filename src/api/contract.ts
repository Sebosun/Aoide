// Generated from the Rust types by `AOIDE_WRITE_CONTRACT=1 cargo test -p aoide contract`.
// Do not edit: change the Rust type and regenerate.

export type BookId = string;

export type BookStatus = { "kind": "present" } | { "kind": "missing" } | { "kind": "error", "message": string };

export type ItemKind = "book" | "audiobook" | "article";

export type Collection = { id: number, name: string, createdAt: number, 
/**
 * Books on it.
 */
books: number, };

export type BookProgress = { chapter: number, segmentId: string | null, fraction: number, };

export type AudioSource = "none" | "speech" | "recording";

export type ReadingMode = "read" | "listen";

export type Shape = "read" | "readAlong" | "listen";

export type ReadState = "unread" | "inProgress" | "finished";

export type BookSummary = { id: string, title: string, authors: Array<string>, language: string | null, status: BookStatus, coverPath: string | null, progress: BookProgress | null, audioBytes: number, 
/**
 * A book (EPUB), an audiobook on its own, or an article.
 */
kind: ItemKind, 
/**
 * What plays if the user listens.
 */
audio: AudioSource, 
/**
 * Read silently or listened to.
 */
mode: ReadingMode, 
/**
 * What the reader screen is for it.
 */
shape: Shape, readState: ReadState, 
/**
 * Its length in words, once it has been opened.
 */
wordCount: number | null, 
/**
 * Whole-book preparation status, if queued, running or finished.
 */
prepare: PrepareView | null, addedAt: number, lastOpenedAt: number | null, 
/**
 * Ids of the collections the book is on.
 */
collections: Array<number>, 
/**
 * The sync stage of the book's recording, if it has one.
 */
recording: SyncStage | null, };

export type HomeView = { 
/**
 * Up to 3 started, unfinished items, most recently opened first.
 */
continueIds: Array<string>, 
/**
 * Up to 5 items never started, newest added first.
 */
upNextIds: Array<string>, 
/**
 * Up to 8 items, newest added first, leaving out the ones listed above.
 */
recentIds: Array<string>, };

export type ImportResult = { path: string, bookId: string | null, error: string | null, };

export type AppInfo = { libraryFolder: string, };

export type CoreError = { "kind": "notFound", what: string, message: string, } | { "kind": "invalid", field: string, message: string, } | { "kind": "engineUnavailable", message: string, } | { "kind": "busy", message: string, } | { "kind": "io", message: string, } | { "kind": "internal", message: string, };

export type ProblemView = { 
/**
 * The sentence to show: what failed and why.
 */
message: string, error: CoreError, };

export type UiLevel = "info" | "warn" | "error";

export type UiLog = { level: UiLevel, 
/**
 * A dotted name like Core's own (`ipc.error`, `ui.error`).
 */
event: string, msg: string, 
/**
 * The command that failed or was slow.
 */
command?: string, tookMs?: number, 
/**
 * The view on screen.
 */
view?: string, };

export type PrepareState = "waiting" | "queued" | "preparing" | "paused" | "done";

export type PrepareView = { state: PrepareState, done: number, total: number, 
/**
 * Place in the queue (0 is first); `None` once done.
 */
position: number | null, 
/**
 * The chapters being prepared, in order; `None` for the whole book.
 */
chapters: Array<number> | null, };

export type BlockKind = { "Heading": { level: number, } } | "Paragraph" | "ListItem" | "Preformatted";

export type NoteMarker = { 
/**
 * Index of the word the marker follows (0 when it precedes the first word).
 */
afterWord: number, label: string, 
/**
 * `None` when the note body was not found.
 */
text: string | null, };

export type SegmentView = { id: string, words: Array<string>, speakable: boolean, 
/**
 * A spoken footnote inserted after its paragraph (not part of the book's text flow).
 */
note: boolean, notes: Array<NoteMarker>, };

export type BlockView = { kind: string, level: number | null, segments: Array<SegmentView>, };

export type ChapterSummary = { index: number, title: string, 
/**
 * An audiobook's part: how long it plays.
 */
durationMs: number | null, };

export type ChapterView = { index: number, title: string, blocks: Array<BlockView>, 
/**
 * When the book's recording narrates it: the spoken sentences it has no audio for, which
 * playback skips. `None` while a speech engine narrates.
 */
notNarrated: Array<string> | null, };

export type ExcludedView = { title: string, reason: string, };

export type SearchHit = { chapter: number, chapterTitle: string, segmentId: string, 
/**
 * Text around the match; `match_start..match_end` (char offsets) is the match.
 */
snippet: string, matchStart: number, matchEnd: number, };

export type BookView = { id: string, title: string, authors: Array<string>, language: string | null, chapters: Array<ChapterSummary>, warnings: Array<string>, excluded: Array<ExcludedView>, languageWarning: string | null, 
/**
 * The book's voice is not offered by the active engine, so another one reads it.
 */
voiceWarning: string | null, progress: BookProgress | null, 
/**
 * Words read aloud in the whole book, for time estimates.
 */
wordCount: number, 
/**
 * Read silently or listened to.
 */
mode: ReadingMode, 
/**
 * What the reader screen is for it.
 */
shape: Shape, };

export type ChapterAudioView = { done: number, total: number, };

export type PlaybackStatus = "stopped" | "buffering" | "playing" | "paused";

export type SleepView = { 
/**
 * Milliseconds until playback pauses, when timed.
 */
remainingMs: number | null, endOfChapter: boolean, };

export type PlayerState = { bookId: string | null, status: PlaybackStatus, chapter: number, segmentId: string | null, wordIndex: number | null, speed: number, 
/**
 * Output gain, 0.0..=1.0.
 */
volume: number, chapterFraction: number, error: string | null, sleep: SleepView | null, 
/**
 * What is being heard: a speech engine or the book's recording.
 */
source: Narration, 
/**
 * The recording is playing narration that is not in the text (credits, a dedication…).
 */
offText: boolean, 
/**
 * Listening time left in the chapter at this speed, when it is known exactly (a
 * recording); otherwise the interface estimates it from the words left.
 */
chapterLeftMs: number | null, 
/**
 * The recording is being listened to while it syncs, and the sync has not reached the
 * playhead yet: playback waits for it.
 */
waitingForSync: boolean, 
/**
 * For an audiobook on its own: the playhead on the whole recording, its length, and how
 * far into the part (`chapter`) the playhead is.
 */
positionMs: number | null, durationMs: number | null, partPositionMs: number | null, };

export type EngineState = "offline" | "loading" | "ready" | "busy" | "error";

export type EngineStatusView = { profileName: string, state: EngineState, detail: string | null, 
/**
 * Something in this run has needed the engine (the user listened, prepared a book or
 * tried a voice). Until then its problems are not the user's: they are only reading.
 */
wanted: boolean, };

export type PreparingView = { bookId: string, done: number, total: number, };

export type GenerationState = { bookId: string | null, generatingSegmentId: string | null, bufferedAheadMs: number, pending: number, error: string | null, 
/**
 * Progress of the book currently being prepared, if any.
 */
preparing: PreparingView | null, };

export type ParamKind = { "type": "float", min: number, max: number, step: number, } | { "type": "int", min: number, max: number, } | { "type": "choice", options: Array<ChoiceOption>, } | { "type": "bool" };

export type ChoiceOption = { value: string, label: string, };

export type ParamValue = boolean | number | number | string;

export type ParamSpec = { key: string, label: string, kind: ParamKind, default: ParamValue, };

export type EngineDescriptor = { 
/**
 * Registry key, e.g. `"chatterbox"`.
 */
kind: string, name: string, 
/**
 * Bump when the adapter changes what audio it produces for the same input; part of cache keys.
 */
adapterVersion: number, 
/**
 * BCP-47 primary language subtags (`"en"`); `None` means the engine accepts any language.
 */
languages: Array<string> | null, 
/**
 * Whether `Synthesis::word_timings` is populated.
 */
wordTimings: boolean, 
/**
 * Longest text (in chars) one request should carry.
 */
maxChars: number, params: Array<ParamSpec>, };

export type NetworkScope = "loopback" | "privateNetwork";

export type LaunchCommand = { program: string, args: Array<string>, workingDir: string | null, };

export type EngineProfile = { id: string, name: string, kind: string, baseUrl: string, 
/**
 * Model name for engines that serve several (sent as-is); `None` uses the server default.
 */
model: string | null, network: NetworkScope, 
/**
 * Default voice for books that do not override it.
 */
voiceId: string | null, params: { [key in string]: ParamValue }, 
/**
 * Optional command that starts the engine server.
 */
launch: LaunchCommand | null, };

export type Voice = { id: string, displayName: string, };

export type EngineOptions = { descriptors: Array<EngineDescriptor>, profile: EngineProfile, };

export type BookSettings = { voiceId: string | null, params: { [key in string]: ParamValue }, 
/**
 * BCP-47 language to send instead of the book's declared language.
 */
language: string | null, 
/**
 * Speak footnotes after the paragraph that references them.
 */
readNotes: boolean, 
/**
 * Which narration plays; `None` until chosen (a speech engine).
 */
narration: Narration | null, 
/**
 * While the recording plays, the speech engine speaks the text it lacks.
 */
speakMissing: boolean, 
/**
 * Read silently or listened to; `None` until the user chooses (see
 * `reading::default_mode`).
 */
mode: ReadingMode | null, };

export type Bookmark = { id: number, bookId: BookId, chapter: number, charOffset: number, segmentId: string | null, 
/**
 * Short excerpt of the text at the position, for display.
 */
excerpt: string, note: string, createdAt: number, 
/**
 * For an audiobook, the place on its timeline; `None` for text.
 */
positionMs: number | null, };

export type DayListening = { 
/**
 * Local calendar day, `YYYY-MM-DD`.
 */
day: string, ms: number, };

export type DayActivity = { 
/**
 * Local calendar day, `YYYY-MM-DD`.
 */
day: string, readingMs: number, listeningMs: number, };

export type BookActivity = { bookId: BookId, readingMs: number, listeningMs: number, };

export type ActivityStats = { readingMs: number, listeningMs: number, 
/**
 * Oldest day first.
 */
byDay: Array<DayActivity>, 
/**
 * Most time first, then by id.
 */
byBook: Array<BookActivity>, };

export type BookDetails = { description: string | null, publisher: string | null, isbn: string | null, published: string | null, subjects: Array<string>, 
/**
 * Open Library work or edition key (e.g. "/works/OL45883W") once the user picked a match.
 */
openLibraryKey: string | null, };

export type MetadataMatch = { 
/**
 * Work key, e.g. `/works/OL45883W`.
 */
key: string, title: string, authors: Array<string>, 
/**
 * First publish year, as text.
 */
published: string | null, 
/**
 * First listed publisher.
 */
publisher: string | null, 
/**
 * The queried ISBN when the match lists it, else the first 13-digit ISBN, else the first ISBN.
 */
isbn: string | null, 
/**
 * At most 10 subjects, in the order given.
 */
subjects: Array<string>, 
/**
 * `{covers_url}/b/id/{cover_i}-M.jpg` when the result has a `cover_i`.
 */
coverUrl: string | null, };

export type LibrarySort = "title" | "author" | "progress" | "recent";

export type AlignerSettings = { baseUrl: string, network: NetworkScope, 
/**
 * Optional command that starts the aligner when it is not running.
 */
launch: LaunchCommand | null, };

export type ScanSettings = { roots: Array<string>, exclude: Array<string>, 
/**
 * Hidden folders to search anyway.
 */
includeHidden: Array<string>, 
/**
 * Look for new books when Aoide starts.
 */
onStartup: boolean, };

export type AppSettings = { 
/**
 * Look books up on Open Library when the user asks for details.
 */
metadataLookup: boolean, 
/**
 * Delete the oldest generated audio above this many bytes.
 */
cacheLimitBytes: number | null, 
/**
 * Local forced-alignment service for exact word timings; `None` uses Aoide's estimate.
 */
aligner: AlignerSettings | null, 
/**
 * Library order: "recent", "title", "author" or "progress".
 */
librarySort: LibrarySort, 
/**
 * Folders searched by "Find books".
 */
scan: ScanSettings, 
/**
 * Carry on preparing queued books when Aoide starts.
 */
resumePrepare: boolean, 
/**
 * Speech recognition for syncing recordings.
 */
recognition: RecognitionSettings, 
/**
 * Where "Find audiobooks" looks.
 */
audiobooks: AudiobookSearchSettings, 
/**
 * Pair found audiobooks without asking: each book's closest audiobooks are tried in turn
 * until one passes the quick check.
 */
verifyAudiobooks: boolean, 
/**
 * Reading speed in words per minute, for reading-time estimates (100–600).
 */
readingWpm: number, };

export type AppSettingsPatch = { metadataLookup?: boolean, cacheLimitBytes?: number | null, aligner?: AlignerSettings | null, librarySort?: LibrarySort, scan?: ScanSettings, resumePrepare?: boolean, recognition?: RecognitionSettings, audiobooks?: AudiobookSearchSettings, verifyAudiobooks?: boolean, readingWpm?: number, };

export type BookStorage = { bookId: string, bytes: number, segments: number, 
/**
 * Unix milliseconds when the newest segment was generated.
 */
newestAt: number, 
/**
 * Playing, being prepared or being exported: bulk cleanup leaves it alone.
 */
inUse: boolean, };

export type StorageReport = { 
/**
 * Generated audio in the index.
 */
audioBytes: number, coversBytes: number, 
/**
 * EPUB copies in the library folder.
 */
booksBytes: number, 
/**
 * The library database, without the recordings' sync data.
 */
databaseBytes: number, 
/**
 * Recordings' transcripts, loudness and sync maps (kept in the database).
 */
syncBytes: number, 
/**
 * Speech-recognition models.
 */
modelsBytes: number, 
/**
 * Files nothing refers to: unindexed audio and covers of removed books.
 */
unusedFiles: number, unusedBytes: number, 
/**
 * Books with generated audio, largest first.
 */
books: Array<BookStorage>, };

export type CleanResult = { files: number, bytes: number, };

export type CandidateStatus = "new" | "inLibrary" | "removed" | "unreadable";

export type Candidate = { 
/**
 * The content hash (the book id it would get), or `path:<path>` when unreadable.
 */
id: string, status: CandidateStatus, title: string, authors: Array<string>, language: string | null, isbn: string | null, coverPath: string | null, paths: Array<string>, size: number, error: string | null, 
/**
 * Best cached Open Library match, if details were fetched.
 */
suggestion: MetadataMatch | null, 
/**
 * The audiobook kept beside it (audio files, or one folder), unless the book already
 * has a recording.
 */
audiobook: Array<string>, };

export type ImportChoice = { id: string, 
/**
 * Apply the suggested Open Library match after importing.
 */
applySuggestion: boolean, 
/**
 * Pair the audiobook found beside it after importing.
 */
pairAudiobook: boolean, };

export type ScanStage = "walking" | "reading" | "done" | "cancelled" | "error";

export type ScanProgress = { stage: ScanStage, dirs: number, files: number, found: number, 
/**
 * Files read so far / to read in this scan (new or changed files only).
 */
read: number, toRead: number, error: string | null, };

export type DiscoveryImportProgress = { done: number, total: number, current: string | null, };

export type AudiobookSearchSettings = { roots: Array<string>, exclude: Array<string>, 
/**
 * Hidden folders to search anyway.
 */
includeHidden: Array<string>, 
/**
 * Look for the audiobooks of books as they are added.
 */
onAdd: boolean, };

export type AudiobookSuggestion = { 
/**
 * What to pair: audio files, or one folder.
 */
paths: Array<string>, 
/**
 * Its name has exactly the book's words.
 */
exact: boolean, 
/**
 * `paths` is one folder (rather than audio files).
 */
folder: boolean, 
/**
 * Every audio file it holds (in a folder: all of them, discs included), sorted.
 */
files: Array<string>, 
/**
 * Their size together.
 */
bytes: number, };

export type MatchStage = { "kind": "found" } | { "kind": "verifying", candidate: number, } | { "kind": "paired", candidate: number, } | { "kind": "noneVerified" };

export type AudiobookMatch = { bookId: string, 
/**
 * Closest first.
 */
candidates: Array<AudiobookSuggestion>, stage: MatchStage, };

export type AudiobookChoice = { bookId: string, 
/**
 * Index into the match's candidates.
 */
candidate: number, };

export type PairOutcome = { bookId: string, error: string | null, };

export type AppearanceMode = "auto" | "light" | "dark";

export type ReaderMode = "page" | "lines";

export type Appearance = { mode: AppearanceMode, 
/**
 * A theme file name without `.toml`, or [`DEFAULT_THEME`].
 */
theme: string, 
/**
 * Reading text size step, 0 (smallest) to 4; 2 is the designed size.
 */
textSize: number, readerMode: ReaderMode, 
/**
 * Interface zoom (Ctrl + / Ctrl −), 1.0 is 100%. The UI keeps it between 0.67 and 1.5.
 */
zoom: number, };

export type Theme = { 
/**
 * File name without `.toml`.
 */
name: string, 
/**
 * Token name (see [`TOKENS`]) to CSS colour.
 */
light: { [key in string]: string }, dark: { [key in string]: string }, problems: Array<string>, };

export type Preferences = { appearance: Appearance, themes: Array<Theme>, 
/**
 * The config folder, for display.
 */
configDir: string, 
/**
 * Problems with settings.toml (parse or section errors); theme problems are on each theme.
 */
problems: Array<string>, };

export type ExportStage = "generating" | "encoding" | "done" | "error";

export type ExportProgress = { bookId: string, stage: ExportStage, done: number, total: number, path: string | null, error: string | null, };

export type SyncStage = "checking" | "waiting" | "needsModel" | "transcribing" | "matching" | "ready" | "sampleMismatch" | "poorMatch" | "paused" | "missing" | "failed" | "standalone";

export type Narration = "speech" | "recording";

export type ModelSizeChoice = "tiny" | "base" | "small";

export type RecognitionDevice = "auto" | "gpu" | "cpu";

export type RecognitionEngine = "builtIn" | "server";

export type RecognitionServer = { baseUrl: string, 
/**
 * Sent as `model`; empty sends `whisper-1`.
 */
model: string, network: NetworkScope, };

export type RecognitionSettings = { engine: RecognitionEngine, 
/**
 * Used when `engine` is `Server`.
 */
server: RecognitionServer, modelSize: ModelSizeChoice, device: RecognitionDevice, 
/**
 * CPU threads; 0 uses half the cores.
 */
threads: number, 
/**
 * Carry on syncing recordings when Aoide starts.
 */
resumeOnStart: boolean, };

export type RecordingFileView = { path: string, name: string, durationMs: number, codec: string, };

export type RecordingProblemView = { path: string, name: string, 
/**
 * Why it is left out, for people.
 */
problem: string, drm: boolean, };

export type InspectionView = { files: Array<RecordingFileView>, problems: Array<RecordingProblemView>, totalMs: number, chapters: number, title: string | null, author: string | null, 
/**
 * The library book whose title the tags name, if any.
 */
suggestedBook: string | null, };

export type MatchSummaryView = { words: number, matchedWords: number, timedWords: number, };

export type RecordingView = { bookId: string, stage: SyncStage, error: string | null, files: Array<RecordingFileView>, totalMs: number, chapters: number, 
/**
 * How much of the recording is transcribed.
 */
transcribedMs: number, 
/**
 * Time left transcribing, once there is a pace to go by.
 */
remainingMs: number | null, 
/**
 * Place in line among recordings waiting to be transcribed (0 is next).
 */
position: number | null, summary: MatchSummaryView | null, accepted: boolean, 
/**
 * The model this book's recording is transcribed with.
 */
modelId: string, 
/**
 * It can be listened to: synced, or syncing with part of it transcribed (what is synced
 * so far plays).
 */
listenable: boolean, 
/**
 * It narrates the book: chosen as the book's narration, and listenable.
 */
narrates: boolean, };

export type ChapterMatchView = { index: number, title: string, words: number, matchedWords: number, timedWords: number, startMs: number | null, endMs: number | null, };

export type OffTextView = { startMs: number, endMs: number, excerpt: string, };

export type SyncReportView = { summary: MatchSummaryView, chapters: Array<ChapterMatchView>, 
/**
 * Narration that is not in the text (credits, a dedication left out of the EPUB…).
 */
offText: Array<OffTextView>, };

export type SyncAction = "pause" | "resume" | "retry" | "accept" | "resync";

export type ModelView = { id: string, size: ModelSizeChoice, englishOnly: boolean, bytes: number, downloaded: boolean, };

export type RecognitionOptions = { settings: RecognitionSettings, 
/**
 * This build can use a GPU.
 */
gpuBuild: boolean, models: Array<ModelView>, modelsFolder: string, };

export type DownloadStage = "downloading" | "done" | "failed";

export type ModelDownloadView = { id: string, stage: DownloadStage, doneBytes: number, totalBytes: number, error: string | null, };

/** Every event Core emits, by name, with its payload. */
export type CoreEvents = {
  "library://changed": null,
  "config://changed": Preferences,
  "player://state": PlayerState,
  "engine://status": EngineStatusView,
  "generation://state": GenerationState,
  "discovery://progress": ScanProgress,
  "discovery://details": string,
  "discovery://import": DiscoveryImportProgress,
  "export://progress": ExportProgress,
  "sync://state": RecordingView,
  "model://download": ModelDownloadView,
  "audiobooks://progress": ScanProgress,
  "audiobooks://matches": Array<AudiobookMatch>,
  "core://problem": ProblemView,
};

/** Every command Core answers, by name, with its arguments and result. */
export type Commands = {
  "log_ui": { args: { entry: UiLog }; result: void };
  "app_info": { args: Record<string, never>; result: AppInfo };
  "list_books": { args: Record<string, never>; result: BookSummary[] };
  "home": { args: Record<string, never>; result: HomeView };
  "add_audiobook": { args: { paths: string[] }; result: ImportResult };
  "import_files": { args: { paths: string[] }; result: ImportResult[] };
  "rescan_library": { args: Record<string, never>; result: void };
  "remove_book": { args: { bookId: string }; result: void };
  "show_in_folder": { args: { bookId: string }; result: void };
  "open_book": { args: { bookId: string }; result: BookView };
  "book_overview": { args: { bookId: string }; result: BookView };
  "mark_finished": { args: { bookId: string }; result: void };
  "mark_unread": { args: { bookId: string }; result: void };
  "set_reading_place": { args: { bookId: string; chapter: number; segmentId: string }; result: BookProgress | null };
  "reading_activity": { args: { bookId: string }; result: void };
  "set_mode": { args: { bookId: string; mode: ReadingMode }; result: BookView };
  "chapter": { args: { bookId: string; chapter: number }; result: ChapterView };
  "play": { args: { bookId: string; chapter: number | null; segmentId: string | null }; result: void };
  "pause": { args: Record<string, never>; result: void };
  "resume": { args: Record<string, never>; result: void };
  "stop": { args: Record<string, never>; result: void };
  "set_speed": { args: { speed: number }; result: void };
  "set_volume": { args: { volume: number }; result: void };
  "engine_status": { args: Record<string, never>; result: EngineStatusView };
  "book_settings": { args: { bookId: string }; result: BookSettings };
  "save_book_settings": { args: { bookId: string; settings: BookSettings }; result: void };
  "engine_options": { args: Record<string, never>; result: EngineOptions };
  "engine_voices": { args: Record<string, never>; result: Voice[] };
  "collections": { args: Record<string, never>; result: Collection[] };
  "create_collection": { args: { name: string }; result: number };
  "rename_collection": { args: { id: number; name: string }; result: void };
  "delete_collection": { args: { id: number }; result: void };
  "set_book_collections": { args: { bookId: string; ids: number[] }; result: void };
  "storage_report": { args: Record<string, never>; result: StorageReport };
  "clean_unused_files": { args: Record<string, never>; result: CleanResult };
  "preview_voice": { args: { profile: EngineProfile; voiceId: string | null; params: { [key in string]: ParamValue }; language: string | null }; result: ArrayBuffer };
  "save_engine_profile": { args: { profile: EngineProfile }; result: void };
  "prepare_book": { args: { bookId: string }; result: void };
  "prepare_chapter": { args: { bookId: string; chapter: number }; result: void };
  "chapter_audio": { args: { bookId: string }; result: ChapterAudioView[] };
  "set_prepare_paused": { args: { bookId: string; paused: boolean }; result: void };
  "cancel_prepare": { args: { bookId: string }; result: void };
  "regenerate_book": { args: { bookId: string }; result: void };
  "set_sleep_timer": { args: { minutes: number | null; endOfChapter: boolean }; result: void };
  "search_book": { args: { bookId: string; query: string }; result: SearchHit[] };
  "skip_chapter": { args: { delta: number }; result: void };
  "seek": { args: { bookId: string; positionMs: number }; result: void };
  "skip": { args: { bookId: string; deltaMs: number }; result: void };
  "bookmarks": { args: { bookId: string }; result: Bookmark[] };
  "add_bookmark": { args: { bookId: string; note: string }; result: Bookmark };
  "update_bookmark_note": { args: { id: number; note: string }; result: void };
  "delete_bookmark": { args: { id: number }; result: void };
  "book_listening": { args: { bookId: string }; result: DayListening[] };
  "activity_stats": { args: Record<string, never>; result: ActivityStats };
  "book_details": { args: { bookId: string }; result: BookDetails };
  "app_settings": { args: Record<string, never>; result: AppSettings };
  "update_app_settings": { args: { patch: AppSettingsPatch }; result: AppSettings };
  "preferences": { args: Record<string, never>; result: Preferences };
  "save_appearance": { args: { appearance: Appearance }; result: void };
  "lookup_metadata": { args: { bookId: string; refresh: boolean }; result: MetadataMatch[] };
  "apply_metadata": { args: { bookId: string; candidate: MetadataMatch }; result: BookDetails };
  "export_book": { args: { bookId: string }; result: string };
  "export_read_along": { args: { bookId: string }; result: string };
  "scan_for_books": { args: Record<string, never>; result: Candidate[] };
  "cancel_scan": { args: Record<string, never>; result: void };
  "discovered_books": { args: Record<string, never>; result: Candidate[] };
  "scan_for_audiobooks": { args: Record<string, never>; result: AudiobookMatch[] };
  "audiobook_matches": { args: Record<string, never>; result: AudiobookMatch[] };
  "cancel_audiobook_scan": { args: Record<string, never>; result: void };
  "look_for_audiobooks": { args: { ids: string[] }; result: number };
  "pair_audiobooks": { args: { choices: AudiobookChoice[] }; result: PairOutcome[] };
  "fetch_discovered_details": { args: { ids: string[] }; result: void };
  "import_discovered": { args: { choices: ImportChoice[] }; result: ImportResult[] };
  "inspect_recording": { args: { paths: string[] }; result: InspectionView };
  "attach_recording": { args: { bookId: string; paths: string[]; asListed: boolean | null }; result: RecordingView };
  "recording": { args: { bookId: string }; result: RecordingView | null };
  "recordings": { args: Record<string, never>; result: RecordingView[] };
  "sync_report": { args: { bookId: string }; result: SyncReportView };
  "sync_action": { args: { bookId: string; action: SyncAction }; result: void };
  "remove_recording": { args: { bookId: string }; result: void };
  "set_narration": { args: { bookId: string; narration: Narration }; result: void };
  "recognition_options": { args: Record<string, never>; result: RecognitionOptions };
  "download_model": { args: { modelId: string }; result: void };
  "play_recording_at": { args: { bookId: string; atMs: number }; result: void };
  "choose_books": { args: Record<string, never>; result: string[] | null };
  "choose_audio": { args: { folder: boolean }; result: string[] | null };
};
