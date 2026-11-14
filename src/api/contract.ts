// Generated from the Rust types by `AOIDE_WRITE_CONTRACT=1 cargo test -p aoide contract`.
// Do not edit: change the Rust type and regenerate.

export type BookId = string;

export type BookStatus = { "kind": "present" } | { "kind": "missing" } | { "kind": "error", "message": string };

export type Collection = { id: number, name: string, createdAt: number, 
/**
 * Books on it.
 */
books: number, };

export type BookProgress = { chapter: number, segmentId: string | null, fraction: number, };

export type BookSummary = { id: string, title: string, authors: Array<string>, language: string | null, status: BookStatus, coverPath: string | null, progress: BookProgress | null, audioBytes: number, 
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

export type ImportResult = { path: string, bookId: string | null, error: string | null, };

export type AppInfo = { libraryFolder: string, };

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

export type ChapterSummary = { index: number, title: string, };

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
wordCount: number, };

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
waitingForSync: boolean, };

export type EngineState = "offline" | "loading" | "ready" | "busy" | "error";

export type EngineStatusView = { profileName: string, state: EngineState, detail: string | null, };

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
narration: Narration | null, };

export type Bookmark = { id: number, bookId: BookId, chapter: number, charOffset: number, segmentId: string | null, 
/**
 * Short excerpt of the text at the position, for display.
 */
excerpt: string, note: string, createdAt: number, };

export type DayListening = { 
/**
 * Local calendar day, `YYYY-MM-DD`.
 */
day: string, ms: number, };

export type BookListeningView = { bookId: string, title: string, ms: number, };

export type ListeningStats = { totalMs: number, byBook: Array<BookListeningView>, byDay: Array<DayListening>, };

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
recognition: RecognitionSettings, };

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

export type CandidateStatus = "new" | "inLibrary" | "removed" | "unreadable";

export type Candidate = { 
/**
 * The content hash (the book id it would get), or `path:<path>` when unreadable.
 */
id: string, status: CandidateStatus, title: string, authors: Array<string>, language: string | null, isbn: string | null, coverPath: string | null, paths: Array<string>, size: number, error: string | null, 
/**
 * Best cached Open Library match, if details were fetched.
 */
suggestion: MetadataMatch | null, };

export type ScanStage = "walking" | "reading" | "done" | "cancelled" | "error";

export type ScanProgress = { stage: ScanStage, dirs: number, files: number, found: number, 
/**
 * Files read so far / to read in this scan (new or changed files only).
 */
read: number, toRead: number, error: string | null, };

export type DiscoveryImportProgress = { done: number, total: number, current: string | null, };

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

export type SyncStage = "checking" | "waiting" | "needsModel" | "transcribing" | "matching" | "ready" | "sampleMismatch" | "poorMatch" | "paused" | "missing" | "failed";

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
modelId: string, };

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
};
