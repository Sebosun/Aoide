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

export type TagCount = { name: string, books: number, };

export type BookProgress = { chapter: number, segmentId: string | null, fraction: number, 
/**
 * An audiobook's place on its timeline (`chapter` is its part).
 */
positionMs: number | null, };

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
 * An audiobook's length (it has no words).
 */
durationMs: number | null, 
/**
 * Whole-book preparation status, if queued, running or finished.
 */
prepare: PrepareView | null, addedAt: number, lastOpenedAt: number | null, 
/**
 * Ids of the collections the book is on.
 */
collections: Array<number>, 
/**
 * Its tags, by name: from its subjects or genre, the user, or a classifier.
 */
tags: Array<string>, 
/**
 * The sync stage of the book's recording, if it has one.
 */
recording: SyncStage | null, 
/**
 * An article's site and fetch.
 */
article: ArticleSummary | null, };

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

export type CoreError = { "kind": "notFound", what: string, message: string, } | { "kind": "invalid", field: string, message: string, } | { "kind": "engineUnavailable", message: string, } | { "kind": "busy", message: string, } | { "kind": "io", message: string, } | { "kind": "cancelled", message: string, } | { "kind": "internal", message: string, };

export type ProblemView = { 
/**
 * The sentence to show: what failed and why.
 */
message: string, error: CoreError, };

export type SpeechProblem = { 
/**
 * What went wrong, said for people.
 */
message: string, 
/**
 * The whole error.
 */
detail: string, 
/**
 * When it first happened in this run, in ms since the epoch.
 */
firstMs: number, 
/**
 * When it last happened.
 */
lastMs: number, 
/**
 * How many times in a row it happened.
 */
times: number, };

export type UiLevel = "debug" | "info" | "warn" | "error";

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
view?: string, 
/**
 * Frames drawn while scrolling or opening (`ui.frames`).
 */
frames?: number, 
/**
 * Frames that took longer than one display refresh to come.
 */
dropped?: number, 
/**
 * The longest wait for a frame.
 */
worstMs?: number, };

export type PrepareState = "waiting" | "queued" | "preparing" | "paused" | "done";

export type PrepareAction = "prepare" | "start" | "pause" | "resume" | "cancel";

export type PrepareView = { state: PrepareState, done: number, total: number, 
/**
 * Place in the queue (0 is first); `None` once done.
 */
position: number | null, 
/**
 * The chapters being prepared, in order; `None` for the whole book.
 */
chapters: Array<number> | null, 
/**
 * What can be done about it now (see [`actions`]).
 */
actions: Array<PrepareAction>, };

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
 * The speech engine says it, so it can be said again (see `Core::say_again`): not a
 * silent entry, nor anything of a book its recording narrates.
 */
canSayAgain: boolean, 
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

export type BookView = { id: string, 
/**
 * A book, an article or an audiobook: what the item is, decided once here.
 */
kind: ItemKind, title: string, authors: Array<string>, language: string | null, chapters: Array<ChapterSummary>, warnings: Array<string>, excluded: Array<ExcludedView>, languageWarning: string | null, 
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
 * Listening time left in the book at this speed, from the playhead to the end; `None`
 * while part of the rest waits for the recording's sync.
 */
bookLeftMs: number | null, 
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

export type ServerState = "stopped" | "starting" | "running" | "stopping" | "waiting" | "failed" | "external";

export type FailureKind = "spawn" | "exited" | "portTaken" | "startTimedOut" | "tooManyRestarts";

export type StoppedKind = "never" | "idle" | "exited" | "switched" | "retry" | "asked" | "quit";

export type ServerView = { state: ServerState, 
/**
 * Why it is stopped, while it is.
 */
stopped: StoppedKind | null, 
/**
 * Why it failed, while it has.
 */
failure: FailureKind | null, 
/**
 * How the failed server ended, when it ended.
 */
exitCode: number | null, signal: number | null, 
/**
 * Why it could not be started at all.
 */
error: string | null, 
/**
 * Minutes without use before it is stopped; `None` keeps it running.
 */
idleMinutes: number | null, 
/**
 * Seconds without an answer or output before a start is given up.
 */
startTimeoutSeconds: number, 
/**
 * Whether Try again applies.
 */
canRetry: boolean, 
/**
 * Whether Stop now applies.
 */
canStop: boolean, 
/**
 * The name of the server's log file (in Aoide's logs folder).
 */
logFile: string, 
/**
 * Its last lines, while it has failed.
 */
lastLines: Array<string>, };

export type EngineStatusView = { profileName: string, state: EngineState, detail: string | null, 
/**
 * Something in this run has needed the engine (the user listened, prepared a book or
 * tried a voice). Until then its problems are not the user's: they are only reading.
 */
wanted: boolean, 
/**
 * The server Aoide starts for the profile's launch command; `None` without one.
 */
server: ServerView | null, };

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

export type LaunchCommand = { program: string, args: Array<string>, workingDir: string | null, 
/**
 * Seconds without an answer and without output after which a start is given up.
 */
startTimeoutSeconds: number, 
/**
 * Minutes without use after which the server is stopped (it starts again when wanted);
 * 0 keeps it running.
 */
idleMinutes: number, };

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

export type TagFilter = { include: Array<string>, exclude: Array<string>, };

export type PauseSettings = { 
/**
 * After each paragraph.
 */
paragraphMs: number, 
/**
 * At a scene break (`* * *`, a rule).
 */
breakMs: number, 
/**
 * After a heading, and at a chapter's end.
 */
headingMs: number, };

export type TaggingSettings = { 
/**
 * Off until the user turns it on.
 */
enabled: boolean, 
/**
 * The model server's origin (Ollama listens on 127.0.0.1:11434).
 */
baseUrl: string, 
/**
 * The model to ask, as the server names it.
 */
model: string, network: NetworkScope, };

export type TaggingRun = { 
/**
 * Items that were given tags.
 */
tagged: number, 
/**
 * Items the model gave nothing for, or that failed.
 */
missed: number, };

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
 * The tags the library shows only, and those it leaves out.
 */
libraryTags: TagFilter, 
/**
 * Tags from a local language model, when turned on.
 */
tagging: TaggingSettings, 
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
readingWpm: number, 
/**
 * How often feeds are checked and how long their entries are kept.
 */
feeds: FeedSettings, 
/**
 * Whether and how a browser fetches articles plain HTTP cannot read.
 */
browser: BrowserSettings, 
/**
 * The silences narration leaves between paragraphs, at scene breaks and after headings.
 */
pauses: PauseSettings, 
/**
 * After a break, play on a little earlier: from the sentence's start, or a sentence or two
 * back, the longer the break.
 */
stepBackOnResume: boolean, };

export type AppSettingsPatch = { metadataLookup?: boolean, cacheLimitBytes?: number | null, aligner?: AlignerSettings | null, librarySort?: LibrarySort, libraryTags?: TagFilter, tagging?: TaggingSettings, scan?: ScanSettings, resumePrepare?: boolean, recognition?: RecognitionSettings, audiobooks?: AudiobookSearchSettings, verifyAudiobooks?: boolean, readingWpm?: number, feeds?: FeedSettings, browser?: BrowserSettings, pauses?: PauseSettings, stepBackOnResume?: boolean, };

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
 * Saved articles' pages and covers.
 */
articlesBytes: number, 
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
 * The optional browser: a downloaded one, and its profile (sign-ins, site data).
 */
browserBytes: number, 
/**
 * Files nothing refers to: unindexed audio, and covers and article pages of removed items.
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
bytes: number, 
/**
 * It may not be an audiobook: nothing about it says so (listed apart).
 */
doubtful: boolean, 
/**
 * Its length, from a few of its files' headers.
 */
durationMs: number | null, 
/**
 * The artist its files name: usually the author, or the narrator.
 */
author: string | null, };

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

export type FetchReason = "signIn" | "paywall" | "check" | "cutShort" | "empty";

export type FetchStage = { "kind": "queued" } | { "kind": "fetching" } | { "kind": "ready" } | { "kind": "needsYou", "reason": FetchReason } | { "kind": "failed" };

export type ArticleSource = "http" | "archive" | "feed" | "browser";

export type DiscoveredFeed = { url: string, title: string | null, subscribed: boolean, };

export type ArticleView = { id: string, 
/**
 * As the user gave it.
 */
url: string, canonicalUrl: string, siteName: string | null, byline: string | null, publishedAt: number | null, stage: FetchStage, 
/**
 * Why the last fetch failed, when it did.
 */
error: string | null, 
/**
 * How the stored copy was got.
 */
via: ArticleSource | null, fetchedAt: number | null, capturedWords: number, declaredWords: number | null, 
/**
 * Feeds the page links to.
 */
feeds: Array<DiscoveredFeed>, 
/**
 * Kept in the library; false while it was only opened from a feed.
 */
inLibrary: boolean, };

export type ArticleSummary = { 
/**
 * The site's name, else its address's host.
 */
siteName: string | null, 
/**
 * Its first lines.
 */
lead: string | null, stage: FetchStage, error: string | null, };

export type FeedStage = { "kind": "idle" } | { "kind": "checking" } | { "kind": "failing", sinceMs: number, };

export type FeedSettings = { 
/**
 * Minutes between checks of a healthy feed (15–1440).
 */
checkMinutes: number, 
/**
 * Days a read entry that was not saved is kept.
 */
keepReadDays: number, 
/**
 * Days an unread entry is kept.
 */
keepUnreadDays: number, };

export type FeedView = { id: number, title: string | null, url: string, siteUrl: string | null, unread: number, stage: FeedStage, 
/**
 * Why the last check failed, while it fails.
 */
error: string | null, lastOkAt: number | null, };

export type SubscribeResult = { "kind": "subscribed", "value": FeedView } | { "kind": "choose", "value": Array<DiscoveredFeed> } | { "kind": "notAFeed" };

export type AddedLink = { "kind": "subscribed", "value": FeedView } | { "kind": "saved", "value": string };

export type OpmlImport = { 
/**
 * Feeds subscribed to.
 */
added: number, 
/**
 * Feeds already subscribed, listed twice, or not at a web address.
 */
skipped: number, };

export type EntryFilter = "all" | "unread" | "saved";

export type EntryView = { id: number, feedId: number, feedTitle: string | null, title: string | null, 
/**
 * Plain text.
 */
summary: string | null, url: string | null, 
/**
 * When it was published, else when a check first found it; the list is newest first by it.
 */
at: number, read: boolean, 
/**
 * Of the text the feed carried, when it carried any.
 */
words: number | null, 
/**
 * The feed carried the whole text: it opens without fetching the page.
 */
hasFullText: boolean, 
/**
 * The item it was opened as.
 */
bookId: string | null, inLibrary: boolean, 
/**
 * What the item it was opened as needs from the user, when its fetch fell short.
 */
needsYou: FetchReason | null, };

export type BrowserUse = "off" | "fallback" | "first";

export type BrowserSettings = { use: BrowserUse, 
/**
 * The Chrome or Chromium to run; found on its own when unset.
 */
path: string | null, 
/**
 * Seconds without work after which the browser is shut down.
 */
idleSeconds: number, };

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
 * The file's `title`, or its name.
 */
title: string, 
/**
 * Comes with Aoide (no file of this name in the themes folder replaces it).
 */
builtin: boolean, 
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

export type RecordingAction = "listen" | "download" | "report" | "export" | "pause" | "resume" | "retry" | "accept" | "resync" | "remove";

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
narrates: boolean, 
/**
 * What can be done about it now (see [`super::syncing::actions`]).
 */
actions: Array<RecordingAction>, };

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

export type BrowserDownloadStage = "downloading" | "checking" | "unpacking" | "done" | "cancelled" | "failed";

export type BrowserDownloadView = { stage: BrowserDownloadStage, 
/**
 * The version being downloaded.
 */
version: string, doneBytes: number, totalBytes: number, error: string | null, };

export type BrowserBinaryView = { "kind": "found", path: string, 
/**
 * Its version (`None` while the browser is off: finding out runs it).
 */
version: string | null, 
/**
 * Whether it is the one Aoide downloaded.
 */
downloaded: boolean, } | { "kind": "missing" };

export type BrowserStatusView = { use: BrowserUse, binary: BrowserBinaryView, 
/**
 * The download under way or last made in this run.
 */
download: BrowserDownloadView | null, 
/**
 * The version "Download Chrome" would get; `None` where none is offered.
 */
offeredVersion: string | null, 
/**
 * Whether a browser was downloaded (it can be removed).
 */
downloaded: boolean, };

export type SiteFetch = "http" | "browser";

export type SiteData = "keep" | "clear_before_fetch";

export type SiteSession = { "kind": "signedIn", at: number, } | { "kind": "checkPassed", at: number, } | { "kind": "askedFor", reason: FetchReason, at: number, } | { "kind": "none" };

export type SiteView = { host: string, session: SiteSession, 
/**
 * `None`: follows the global choice.
 */
fetch: SiteFetch | null, siteData: SiteData, 
/**
 * An article from this site waiting for the user, to sign in with.
 */
waitingArticle: string | null, };

export type SignInStage = { "kind": "opening" } | { "kind": "waiting" } | { "kind": "fetching" } | { "kind": "done" } | { "kind": "cancelled" } | { "kind": "failed", error: string, };

export type SignInView = { articleId: string, 
/**
 * The site signed in to (`www.` dropped).
 */
site: string, 
/**
 * What the article needed when the flow started.
 */
reason: FetchReason | null, stage: SignInStage, 
/**
 * Keep the site's cookies after the fetch ("Stay signed in to this site").
 */
staySignedIn: boolean, 
/**
 * The article's fetch stage once fetched.
 */
result: FetchStage | null, };

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
  "article://state": ArticleView,
  "feeds://changed": null,
  "signin://state": SignInView,
  "browser://download": BrowserDownloadView,
};

/** Every command Core answers, by name, with its arguments and result. */
export type Commands = {
  /** Writes a line to Aoide's log (a failure the window saw). */
  "log_ui": { args: { entry: UiLog }; result: void };
  "app_info": { args: Record<string, never>; result: AppInfo };
  "app_settings": { args: Record<string, never>; result: AppSettings };
  /** Merges a change into the app settings; resolves with the settings now saved. */
  "update_app_settings": { args: { patch: AppSettingsPatch }; result: AppSettings };
  /** Appearance, themes and config problems from ~/.config/aoide. */
  "preferences": { args: Record<string, never>; result: Preferences };
  "save_appearance": { args: { appearance: Appearance }; result: void };
  /**
   * Saves a link as an article (or finds it saved) and queues its fetch; returns its id. An invalid
   * link is refused. "article://state" reports the fetch.
   */
  "save_article": { args: { url: string }; result: string };
  /** Fetches an article again, from the page or an archived copy. */
  "refetch_article": { args: { bookId: string; source: ArticleSource }; result: void };
  /** An article's source and where its fetch stands. */
  "article": { args: { bookId: string }; result: ArticleView };
  /** What browser would run for articles, and its download. */
  "browser_status": { args: Record<string, never>; result: BrowserStatusView };
  /**
   * Downloads the pinned Chrome for Testing (progress on "browser://download"); refused while the
   * browser is off. Resolves once done, cancelled or failed.
   */
  "download_browser": { args: Record<string, never>; result: void };
  /** Stops a browser download; downloading again continues it. */
  "cancel_browser_download": { args: Record<string, never>; result: void };
  /** Deletes the downloaded browser. */
  "remove_browser": { args: Record<string, never>; result: void };
  /** Sites that needed the user or have a rule, by host. */
  "sites": { args: Record<string, never>; result: SiteView[] };
  /**
   * How a site is fetched (null: as the global choice) and whether its data is kept; written to
   * site-rules.toml.
   */
  "set_site_rule": { args: { host: string; fetch?: SiteFetch | null; siteData: SiteData }; result: void };
  /** Clears the site's cookies and storage in the browser (refused while it is off). */
  "sign_out_site": { args: { host: string }; result: void };
  /** Signs out of the site and drops its row and rule. */
  "forget_site": { args: { host: string }; result: void };
  /** Removes the browser's profile: every sign-in and all site data. */
  "clear_browser_data": { args: Record<string, never>; result: void };
  /**
   * Opens a browser window on the article's page to sign in or pass a check; the article is fetched
   * again once the window shows it whole or `finishSignIn` is called. One at a time; refused while
   * the browser is off. Its stages arrive on "signin://state".
   */
  "start_sign_in": { args: { bookId: string }; result: SignInView };
  /** The sign-in open or last finished, if any. */
  "sign_in": { args: Record<string, never>; result: SignInView | null };
  /** "I'm signed in, fetch again". */
  "finish_sign_in": { args: Record<string, never>; result: void };
  "cancel_sign_in": { args: Record<string, never>; result: void };
  "show_sign_in_window": { args: Record<string, never>; result: void };
  /** Whether the site's cookies are kept after the fetch. */
  "stay_signed_in": { args: { stay: boolean }; result: void };
  /** The system's file chooser for the browser's program; null when cancelled. */
  "choose_browser": { args: Record<string, never>; result: string | null };
  /** Scans the configured folders; resolves with everything found. */
  "scan_for_books": { args: Record<string, never>; result: Candidate[] };
  "cancel_scan": { args: Record<string, never>; result: void };
  /** Results of the last scan, compared with the library. */
  "discovered_books": { args: Record<string, never>; result: Candidate[] };
  /**
   * Searches the audiobook folders for every book without a recording; with verifying on, Aoide
   * starts pairing them itself. "audiobooks://progress" reports the search.
   */
  "scan_for_audiobooks": { args: Record<string, never>; result: AudiobookMatch[] };
  /** The matches the last searches found, and where each stands. */
  "audiobook_matches": { args: Record<string, never>; result: AudiobookMatch[] };
  /**
   * Gives an audiobook on its own the book it reads (already imported, with no recording): the book
   * takes its recording, which syncs, its bookmarks, collections and listening time, and its place
   * once the sync reaches it. The audiobook item is removed.
   */
  "merge_audiobook": { args: { audiobookId: string; bookId: string }; result: void };
  /** What the last whole search found that comes close to no book, to add on its own. */
  "unmatched_audiobooks": { args: Record<string, never>; result: AudiobookSuggestion[] };
  "cancel_audiobook_scan": { args: Record<string, never>; result: void };
  /**
   * After books were added: how many have audiobooks waiting for the user (0 when looking on adding
   * is off or Aoide verifies them).
   */
  "look_for_audiobooks": { args: { ids: string[] }; result: number };
  /** Pairs each chosen candidate; each succeeds or fails on its own. */
  "pair_audiobooks": { args: { choices: AudiobookChoice[] }; result: PairOutcome[] };
  /** Open Library lookups for these candidates; "discovery://details" fires per id. */
  "fetch_discovered_details": { args: { ids: string[] }; result: void };
  /** `pairAudiobook` pairs the audiobook found beside a book once it is imported. */
  "import_discovered": { args: { choices: ImportChoice[] }; result: ImportResult[] };
  /** The system's file chooser for books (EPUBs and PDFs) to add; null when cancelled. */
  "choose_books": { args: Record<string, never>; result: string[] | null };
  /** The system's folder chooser, titled `title` (what the folder is for); null when cancelled. */
  "choose_folder": { args: { title: string }; result: string | null };
  /** The system's file chooser for audiobook files (or a folder); null when cancelled. */
  "choose_audio": { args: { folder: boolean }; result: string[] | null };
  /** Subscriptions, by title, with their unread counts and health. */
  "feeds": { args: Record<string, never>; result: FeedView[] };
  /** A link given to Add: a feed is subscribed, anything else is saved as an article. */
  "add_link": { args: { url: string }; result: AddedLink };
  /**
   * Subscribes to a feed address, or to the one feed a page links to; a page with several feeds
   * offers them to choose from. Subscribing again finds the same feed.
   */
  "subscribe": { args: { url: string }; result: SubscribeResult };
  /** Removes a feed and its entries that were not saved. */
  "unsubscribe": { args: { feedId: number }; result: void };
  /** Entries newest first, from one feed or all; the next page starts `before` an entry id. */
  "feed_entries": { args: { feedId?: number | null; filter: EntryFilter; before?: number | null; limit?: number | null }; result: EntryView[] };
  /** The item to read an entry as (made on the feed shelf when needed); marks it read. */
  "open_feed_entry": { args: { entryId: number }; result: string };
  /** Keeps an entry's item in the library; returns its id. */
  "save_feed_entry": { args: { entryId: number }; result: string };
  "mark_entry_read": { args: { entryId: number; read: boolean }; result: void };
  /** Marks one feed's entries read, or every feed's. */
  "mark_all_read": { args: { feedId?: number | null }; result: void };
  /** Keeps an item opened from a feed in the library (the reader's "Save to library"). */
  "save_to_library": { args: { bookId: string }; result: void };
  /** Opens a feed's site (its own address when it names none) in the system's browser. */
  "open_feed_site": { args: { feedId: number }; result: void };
  /** Checks every feed now ("feeds://changed" reports what changed). */
  "check_feeds": { args: Record<string, never>; result: void };
  /** The system's file chooser for a subscription list (OPML) to import; null when cancelled. */
  "choose_opml": { args: Record<string, never>; result: string | null };
  /** The system's save dialog for exporting the subscription list; null when cancelled. */
  "choose_opml_target": { args: Record<string, never>; result: string | null };
  /** Subscribes to the feeds an OPML file lists that are not subscribed yet. */
  "import_opml": { args: { path: string }; result: OpmlImport };
  /** Writes every subscription to an OPML file; resolves with how many. */
  "export_opml": { args: { path: string }; result: number };
  "list_books": { args: Record<string, never>; result: BookSummary[] };
  /** Home's lists, as ids of library items. */
  "home": { args: Record<string, never>; result: HomeView };
  /**
   * Adds an audiobook with no EPUB (an M4B, audio files or a folder of them); the same files again
   * are the same item. Refusals (DRM, unreadable) come back as the error.
   */
  "add_audiobook": { args: { paths: string[] }; result: ImportResult };
  /** Copies dropped EPUB files into the library folder and imports them. */
  "import_files": { args: { paths: string[] }; result: ImportResult[] };
  "rescan_library": { args: Record<string, never>; result: void };
  "remove_book": { args: { bookId: string }; result: void };
  /** Opens the file manager on the book's folder with its EPUB selected. */
  "show_in_folder": { args: { bookId: string }; result: void };
  /** Opens an article's page, as it was kept, in the system's browser. */
  "open_original": { args: { bookId: string }; result: void };
  "open_book": { args: { bookId: string }; result: BookView };
  /** Like openBook, without marking the book as recently opened. */
  "book_overview": { args: { bookId: string }; result: BookView };
  "chapter": { args: { bookId: string; chapter: number }; result: ChapterView };
  /** By name. */
  "collections": { args: Record<string, never>; result: Collection[] };
  /** Fails when the name is empty or already used (ignoring case). Returns the new id. */
  "create_collection": { args: { name: string }; result: number };
  "rename_collection": { args: { id: number; name: string }; result: void };
  /** The books stay in the library. */
  "delete_collection": { args: { id: number }; result: void };
  /** Every tag on some item, with how many items have it, by name. */
  "tags": { args: Record<string, never>; result: TagCount[] };
  /**
   * The user's tags for an item: exactly these are shown; the others stay removed (reading its
   * subjects again does not bring them back).
   */
  "set_book_tags": { args: { bookId: string; tags: string[] }; result: void };
  /** Asks the tagging model (Settings › Library) for an item's tags; keeps and returns them. */
  "tag_with_model": { args: { bookId: string }; result: string[] };
  /** Asks the tagging model for the tags of every item that has none, one after another. */
  "tag_untagged_with_model": { args: Record<string, never>; result: TaggingRun };
  /** Puts the book on exactly these collections. */
  "set_book_collections": { args: { bookId: string; ids: number[] }; result: void };
  "storage_report": { args: Record<string, never>; result: StorageReport };
  /** Deletes unused files (see StorageReport) and returns what was freed. */
  "clean_unused_files": { args: Record<string, never>; result: CleanResult };
  /** Case-insensitive text search (min 2 chars), at most 200 hits in reading order. */
  "search_book": { args: { bookId: string; query: string }; result: SearchHit[] };
  "book_details": { args: { bookId: string }; result: BookDetails };
  /**
   * Searches Open Library for the book (fails when lookups are turned off). Cached for 30 days;
   * refresh asks Open Library again.
   */
  "lookup_metadata": { args: { bookId: string; refresh: boolean }; result: MetadataMatch[] };
  /** Stores the picked match's details (and cover when the book has none). */
  "apply_metadata": { args: { bookId: string; candidate: MetadataMatch }; result: BookDetails };
  /** Writes an M4B audiobook (generating missing audio first); resolves with the output path. */
  "export_book": { args: { bookId: string }; result: string };
  /** Writes the book with its synced recording as an EPUB 3 with Media Overlays; returns its path. */
  "export_read_along": { args: { bookId: string }; result: string };
  /** Starts playback at a segment (or the saved position when segmentId is null). */
  "play": { args: { bookId: string; chapter?: number | null; segmentId?: string | null }; result: void };
  "pause": { args: Record<string, never>; result: void };
  "resume": { args: Record<string, never>; result: void };
  "stop": { args: Record<string, never>; result: void };
  "set_speed": { args: { speed: number }; result: void };
  /** Output gain, 0–1; saved with the speed. */
  "set_volume": { args: { volume: number }; result: void };
  /** minutes: pause after that long; endOfChapter: pause at the chapter end; neither: cancel. */
  "set_sleep_timer": { args: { minutes?: number | null; endOfChapter: boolean }; result: void };
  /** Jump to the start of the next (+1) or previous (-1) chapter of the playing book. */
  "skip_chapter": { args: { delta: number }; result: void };
  /** Moves to positionMs on an audiobook's timeline (its saved place when it is not playing). */
  "seek": { args: { bookId: string; positionMs: number }; result: void };
  /** Moves deltaMs back or forward on an audiobook's timeline (the ±30 s buttons). */
  "skip": { args: { bookId: string; deltaMs: number }; result: void };
  "mark_finished": { args: { bookId: string }; result: void };
  /** Forgets that the book was finished and where the user was in it. */
  "mark_unread": { args: { bookId: string }; result: void };
  /**
   * Where the user is reading silently; returns the book's progress now, or null when ignored
   * because that book is playing.
   */
  "set_reading_place": { args: { bookId: string; chapter: number; segmentId: string }; result: BookProgress | null };
  /** The user is reading the book silently now; send every 15 s at most. */
  "reading_activity": { args: { bookId: string }; result: void };
  /**
   * Reads the book silently or listens to it. Reading the playing book pauses it; choosing to listen
   * does not start playback (call play next).
   */
  "set_mode": { args: { bookId: string; mode: ReadingMode }; result: BookView };
  /** Bookmarks of a book, in reading order. */
  "bookmarks": { args: { bookId: string }; result: Bookmark[] };
  /** Bookmarks the current playhead (or the saved position of bookId when not playing it). */
  "add_bookmark": { args: { bookId: string; note: string }; result: Bookmark };
  "update_bookmark_note": { args: { id: number; note: string }; result: void };
  "delete_bookmark": { args: { id: number }; result: void };
  /** One book's listened time per local day, oldest first. */
  "book_listening": { args: { bookId: string }; result: DayListening[] };
  /** Reading and listening time, per day and per book. */
  "activity_stats": { args: Record<string, never>; result: ActivityStats };
  /** What dropped or chosen audio files hold, before pairing them with a book. */
  "inspect_recording": { args: { paths: string[] }; result: InspectionView };
  /**
   * Pairs audio files with a book (replacing its recording) and starts syncing them.
   * `asListed`: the files stay in the order given instead of being sorted.
   */
  "attach_recording": { args: { bookId: string; paths: string[]; asListed?: boolean | null }; result: RecordingView };
  "recording": { args: { bookId: string }; result: RecordingView | null };
  /** Every book's recording, oldest first. */
  "recordings": { args: Record<string, never>; result: RecordingView[] };
  /** How well the recording matched the text (fails until it is matched). */
  "sync_report": { args: { bookId: string }; result: SyncReportView };
  "sync_action": { args: { bookId: string; action: SyncAction }; result: void };
  /** Forgets the recording and its sync; the audio files are left alone. */
  "remove_recording": { args: { bookId: string }; result: void };
  /** Plays the book with its recording (once synced) or a speech engine. */
  "set_narration": { args: { bookId: string; narration: Narration }; result: void };
  "recognition_options": { args: Record<string, never>; result: RecognitionOptions };
  /** Downloads a speech-recognition model (progress on "model://download"). */
  "download_model": { args: { modelId: string }; result: void };
  /** Plays the book's recording from a time on its timeline (needs the recording narration). */
  "play_recording_at": { args: { bookId: string; atMs: number }; result: void };
  "engine_status": { args: Record<string, never>; result: EngineStatusView };
  "book_settings": { args: { bookId: string }; result: BookSettings };
  "save_book_settings": { args: { bookId: string; settings: BookSettings }; result: void };
  /** What went wrong with the speech engine lately, newest first (Settings › Speech engine). */
  "speech_problems": { args: Record<string, never>; result: SpeechProblem[] };
  /** Forgets the speech engine's problems listed so far. */
  "clear_speech_problems": { args: Record<string, never>; result: void };
  /** Try again: starts a speech server that failed once more. */
  "retry_speech_server": { args: Record<string, never>; result: void };
  /** Stop now: stops the speech server Aoide started; it starts again when speech is needed. */
  "stop_speech_server": { args: Record<string, never>; result: void };
  /** Shows the speech server's log (`speech-server.log`) in the file manager. */
  "show_speech_server_log": { args: Record<string, never>; result: void };
  /** Registered adapters and the active global profile. */
  "engine_options": { args: Record<string, never>; result: EngineOptions };
  /** Voices of the active engine (fails when the engine is offline). */
  "engine_voices": { args: Record<string, never>; result: Voice[] };
  /**
   * Speaks a sample sentence with `profile` (saved or not) and returns a WAV file. `params` override
   * the profile's; a null voice means the profile's voice, else the engine's first.
   * The WAV goes back as raw bytes (an ArrayBuffer in the UI), not as a JSON number array.
   */
  "preview_voice": { args: { profile: EngineProfile; voiceId?: string | null; params: { [key in string]: ParamValue }; language?: string | null }; result: ArrayBuffer };
  /**
   * Says a few short paragraphs in the voice books are read in, with `pauses` (as on screen,
   * saved or not) between them, and returns a WAV file as raw bytes.
   */
  "preview_pauses": { args: { pauses: PauseSettings }; result: ArrayBuffer };
  /** Validates and activates a profile; regenerates upcoming audio with it. */
  "save_engine_profile": { args: { profile: EngineProfile }; result: void };
  /** Queue whole-book generation (progress arrives via BookSummary.prepare + library://changed). */
  "prepare_book": { args: { bookId: string }; result: void };
  /** Queues one chapter (from 0); it joins the book's entry, or is already covered by a whole book. */
  "prepare_chapter": { args: { bookId: string; chapter: number }; result: void };
  /** How much of each chapter's audio is made, in chapter order. */
  "chapter_audio": { args: { bookId: string }; result: ChapterAudioView[] };
  /** Pause or resume a queued book's preparation (remembered across restarts). */
  "set_prepare_paused": { args: { bookId: string; paused: boolean }; result: void };
  "cancel_prepare": { args: { bookId: string }; result: void };
  /**
   * Throw away one sentence's audio and make it again as a new take, from the playhead put on
   * it (a book not being listened to is listened to from there).
   */
  "say_again": { args: { bookId: string; segmentId: string }; result: void };
  /** Delete the book's generated audio so it is regenerated. */
  "regenerate_book": { args: { bookId: string }; result: void };
};

/** Every command as the UI calls it: a method taking the command's arguments by name. */
export interface CommandApi {
  /** Writes a line to Aoide's log (a failure the window saw). */
  logUi(args: { entry: UiLog }): Promise<void>;
  appInfo(): Promise<AppInfo>;
  appSettings(): Promise<AppSettings>;
  /** Merges a change into the app settings; resolves with the settings now saved. */
  updateAppSettings(args: { patch: AppSettingsPatch }): Promise<AppSettings>;
  /** Appearance, themes and config problems from ~/.config/aoide. */
  preferences(): Promise<Preferences>;
  saveAppearance(args: { appearance: Appearance }): Promise<void>;
  /**
   * Saves a link as an article (or finds it saved) and queues its fetch; returns its id. An invalid
   * link is refused. "article://state" reports the fetch.
   */
  saveArticle(args: { url: string }): Promise<string>;
  /** Fetches an article again, from the page or an archived copy. */
  refetchArticle(args: { bookId: string; source: ArticleSource }): Promise<void>;
  /** An article's source and where its fetch stands. */
  article(args: { bookId: string }): Promise<ArticleView>;
  /** What browser would run for articles, and its download. */
  browserStatus(): Promise<BrowserStatusView>;
  /**
   * Downloads the pinned Chrome for Testing (progress on "browser://download"); refused while the
   * browser is off. Resolves once done, cancelled or failed.
   */
  downloadBrowser(): Promise<void>;
  /** Stops a browser download; downloading again continues it. */
  cancelBrowserDownload(): Promise<void>;
  /** Deletes the downloaded browser. */
  removeBrowser(): Promise<void>;
  /** Sites that needed the user or have a rule, by host. */
  sites(): Promise<SiteView[]>;
  /**
   * How a site is fetched (null: as the global choice) and whether its data is kept; written to
   * site-rules.toml.
   */
  setSiteRule(args: { host: string; fetch?: SiteFetch | null; siteData: SiteData }): Promise<void>;
  /** Clears the site's cookies and storage in the browser (refused while it is off). */
  signOutSite(args: { host: string }): Promise<void>;
  /** Signs out of the site and drops its row and rule. */
  forgetSite(args: { host: string }): Promise<void>;
  /** Removes the browser's profile: every sign-in and all site data. */
  clearBrowserData(): Promise<void>;
  /**
   * Opens a browser window on the article's page to sign in or pass a check; the article is fetched
   * again once the window shows it whole or `finishSignIn` is called. One at a time; refused while
   * the browser is off. Its stages arrive on "signin://state".
   */
  startSignIn(args: { bookId: string }): Promise<SignInView>;
  /** The sign-in open or last finished, if any. */
  signIn(): Promise<SignInView | null>;
  /** "I'm signed in, fetch again". */
  finishSignIn(): Promise<void>;
  cancelSignIn(): Promise<void>;
  showSignInWindow(): Promise<void>;
  /** Whether the site's cookies are kept after the fetch. */
  staySignedIn(args: { stay: boolean }): Promise<void>;
  /** The system's file chooser for the browser's program; null when cancelled. */
  chooseBrowser(): Promise<string | null>;
  /** Scans the configured folders; resolves with everything found. */
  scanForBooks(): Promise<Candidate[]>;
  cancelScan(): Promise<void>;
  /** Results of the last scan, compared with the library. */
  discoveredBooks(): Promise<Candidate[]>;
  /**
   * Searches the audiobook folders for every book without a recording; with verifying on, Aoide
   * starts pairing them itself. "audiobooks://progress" reports the search.
   */
  scanForAudiobooks(): Promise<AudiobookMatch[]>;
  /** The matches the last searches found, and where each stands. */
  audiobookMatches(): Promise<AudiobookMatch[]>;
  /**
   * Gives an audiobook on its own the book it reads (already imported, with no recording): the book
   * takes its recording, which syncs, its bookmarks, collections and listening time, and its place
   * once the sync reaches it. The audiobook item is removed.
   */
  mergeAudiobook(args: { audiobookId: string; bookId: string }): Promise<void>;
  /** What the last whole search found that comes close to no book, to add on its own. */
  unmatchedAudiobooks(): Promise<AudiobookSuggestion[]>;
  cancelAudiobookScan(): Promise<void>;
  /**
   * After books were added: how many have audiobooks waiting for the user (0 when looking on adding
   * is off or Aoide verifies them).
   */
  lookForAudiobooks(args: { ids: string[] }): Promise<number>;
  /** Pairs each chosen candidate; each succeeds or fails on its own. */
  pairAudiobooks(args: { choices: AudiobookChoice[] }): Promise<PairOutcome[]>;
  /** Open Library lookups for these candidates; "discovery://details" fires per id. */
  fetchDiscoveredDetails(args: { ids: string[] }): Promise<void>;
  /** `pairAudiobook` pairs the audiobook found beside a book once it is imported. */
  importDiscovered(args: { choices: ImportChoice[] }): Promise<ImportResult[]>;
  /** The system's file chooser for books (EPUBs and PDFs) to add; null when cancelled. */
  chooseBooks(): Promise<string[] | null>;
  /** The system's folder chooser, titled `title` (what the folder is for); null when cancelled. */
  chooseFolder(args: { title: string }): Promise<string | null>;
  /** The system's file chooser for audiobook files (or a folder); null when cancelled. */
  chooseAudio(args: { folder: boolean }): Promise<string[] | null>;
  /** Subscriptions, by title, with their unread counts and health. */
  feeds(): Promise<FeedView[]>;
  /** A link given to Add: a feed is subscribed, anything else is saved as an article. */
  addLink(args: { url: string }): Promise<AddedLink>;
  /**
   * Subscribes to a feed address, or to the one feed a page links to; a page with several feeds
   * offers them to choose from. Subscribing again finds the same feed.
   */
  subscribe(args: { url: string }): Promise<SubscribeResult>;
  /** Removes a feed and its entries that were not saved. */
  unsubscribe(args: { feedId: number }): Promise<void>;
  /** Entries newest first, from one feed or all; the next page starts `before` an entry id. */
  feedEntries(args: { feedId?: number | null; filter: EntryFilter; before?: number | null; limit?: number | null }): Promise<EntryView[]>;
  /** The item to read an entry as (made on the feed shelf when needed); marks it read. */
  openFeedEntry(args: { entryId: number }): Promise<string>;
  /** Keeps an entry's item in the library; returns its id. */
  saveFeedEntry(args: { entryId: number }): Promise<string>;
  markEntryRead(args: { entryId: number; read: boolean }): Promise<void>;
  /** Marks one feed's entries read, or every feed's. */
  markAllRead(args?: { feedId?: number | null }): Promise<void>;
  /** Keeps an item opened from a feed in the library (the reader's "Save to library"). */
  saveToLibrary(args: { bookId: string }): Promise<void>;
  /** Opens a feed's site (its own address when it names none) in the system's browser. */
  openFeedSite(args: { feedId: number }): Promise<void>;
  /** Checks every feed now ("feeds://changed" reports what changed). */
  checkFeeds(): Promise<void>;
  /** The system's file chooser for a subscription list (OPML) to import; null when cancelled. */
  chooseOpml(): Promise<string | null>;
  /** The system's save dialog for exporting the subscription list; null when cancelled. */
  chooseOpmlTarget(): Promise<string | null>;
  /** Subscribes to the feeds an OPML file lists that are not subscribed yet. */
  importOpml(args: { path: string }): Promise<OpmlImport>;
  /** Writes every subscription to an OPML file; resolves with how many. */
  exportOpml(args: { path: string }): Promise<number>;
  listBooks(): Promise<BookSummary[]>;
  /** Home's lists, as ids of library items. */
  home(): Promise<HomeView>;
  /**
   * Adds an audiobook with no EPUB (an M4B, audio files or a folder of them); the same files again
   * are the same item. Refusals (DRM, unreadable) come back as the error.
   */
  addAudiobook(args: { paths: string[] }): Promise<ImportResult>;
  /** Copies dropped EPUB files into the library folder and imports them. */
  importFiles(args: { paths: string[] }): Promise<ImportResult[]>;
  rescanLibrary(): Promise<void>;
  removeBook(args: { bookId: string }): Promise<void>;
  /** Opens the file manager on the book's folder with its EPUB selected. */
  showInFolder(args: { bookId: string }): Promise<void>;
  /** Opens an article's page, as it was kept, in the system's browser. */
  openOriginal(args: { bookId: string }): Promise<void>;
  openBook(args: { bookId: string }): Promise<BookView>;
  /** Like openBook, without marking the book as recently opened. */
  bookOverview(args: { bookId: string }): Promise<BookView>;
  chapter(args: { bookId: string; chapter: number }): Promise<ChapterView>;
  /** By name. */
  collections(): Promise<Collection[]>;
  /** Fails when the name is empty or already used (ignoring case). Returns the new id. */
  createCollection(args: { name: string }): Promise<number>;
  renameCollection(args: { id: number; name: string }): Promise<void>;
  /** The books stay in the library. */
  deleteCollection(args: { id: number }): Promise<void>;
  /** Every tag on some item, with how many items have it, by name. */
  tags(): Promise<TagCount[]>;
  /**
   * The user's tags for an item: exactly these are shown; the others stay removed (reading its
   * subjects again does not bring them back).
   */
  setBookTags(args: { bookId: string; tags: string[] }): Promise<void>;
  /** Asks the tagging model (Settings › Library) for an item's tags; keeps and returns them. */
  tagWithModel(args: { bookId: string }): Promise<string[]>;
  /** Asks the tagging model for the tags of every item that has none, one after another. */
  tagUntaggedWithModel(): Promise<TaggingRun>;
  /** Puts the book on exactly these collections. */
  setBookCollections(args: { bookId: string; ids: number[] }): Promise<void>;
  storageReport(): Promise<StorageReport>;
  /** Deletes unused files (see StorageReport) and returns what was freed. */
  cleanUnusedFiles(): Promise<CleanResult>;
  /** Case-insensitive text search (min 2 chars), at most 200 hits in reading order. */
  searchBook(args: { bookId: string; query: string }): Promise<SearchHit[]>;
  bookDetails(args: { bookId: string }): Promise<BookDetails>;
  /**
   * Searches Open Library for the book (fails when lookups are turned off). Cached for 30 days;
   * refresh asks Open Library again.
   */
  lookupMetadata(args: { bookId: string; refresh: boolean }): Promise<MetadataMatch[]>;
  /** Stores the picked match's details (and cover when the book has none). */
  applyMetadata(args: { bookId: string; candidate: MetadataMatch }): Promise<BookDetails>;
  /** Writes an M4B audiobook (generating missing audio first); resolves with the output path. */
  exportBook(args: { bookId: string }): Promise<string>;
  /** Writes the book with its synced recording as an EPUB 3 with Media Overlays; returns its path. */
  exportReadAlong(args: { bookId: string }): Promise<string>;
  /** Starts playback at a segment (or the saved position when segmentId is null). */
  play(args: { bookId: string; chapter?: number | null; segmentId?: string | null }): Promise<void>;
  pause(): Promise<void>;
  resume(): Promise<void>;
  stop(): Promise<void>;
  setSpeed(args: { speed: number }): Promise<void>;
  /** Output gain, 0–1; saved with the speed. */
  setVolume(args: { volume: number }): Promise<void>;
  /** minutes: pause after that long; endOfChapter: pause at the chapter end; neither: cancel. */
  setSleepTimer(args: { minutes?: number | null; endOfChapter: boolean }): Promise<void>;
  /** Jump to the start of the next (+1) or previous (-1) chapter of the playing book. */
  skipChapter(args: { delta: number }): Promise<void>;
  /** Moves to positionMs on an audiobook's timeline (its saved place when it is not playing). */
  seek(args: { bookId: string; positionMs: number }): Promise<void>;
  /** Moves deltaMs back or forward on an audiobook's timeline (the ±30 s buttons). */
  skip(args: { bookId: string; deltaMs: number }): Promise<void>;
  markFinished(args: { bookId: string }): Promise<void>;
  /** Forgets that the book was finished and where the user was in it. */
  markUnread(args: { bookId: string }): Promise<void>;
  /**
   * Where the user is reading silently; returns the book's progress now, or null when ignored
   * because that book is playing.
   */
  setReadingPlace(args: { bookId: string; chapter: number; segmentId: string }): Promise<BookProgress | null>;
  /** The user is reading the book silently now; send every 15 s at most. */
  readingActivity(args: { bookId: string }): Promise<void>;
  /**
   * Reads the book silently or listens to it. Reading the playing book pauses it; choosing to listen
   * does not start playback (call play next).
   */
  setMode(args: { bookId: string; mode: ReadingMode }): Promise<BookView>;
  /** Bookmarks of a book, in reading order. */
  bookmarks(args: { bookId: string }): Promise<Bookmark[]>;
  /** Bookmarks the current playhead (or the saved position of bookId when not playing it). */
  addBookmark(args: { bookId: string; note: string }): Promise<Bookmark>;
  updateBookmarkNote(args: { id: number; note: string }): Promise<void>;
  deleteBookmark(args: { id: number }): Promise<void>;
  /** One book's listened time per local day, oldest first. */
  bookListening(args: { bookId: string }): Promise<DayListening[]>;
  /** Reading and listening time, per day and per book. */
  activityStats(): Promise<ActivityStats>;
  /** What dropped or chosen audio files hold, before pairing them with a book. */
  inspectRecording(args: { paths: string[] }): Promise<InspectionView>;
  /**
   * Pairs audio files with a book (replacing its recording) and starts syncing them.
   * `asListed`: the files stay in the order given instead of being sorted.
   */
  attachRecording(args: { bookId: string; paths: string[]; asListed?: boolean | null }): Promise<RecordingView>;
  recording(args: { bookId: string }): Promise<RecordingView | null>;
  /** Every book's recording, oldest first. */
  recordings(): Promise<RecordingView[]>;
  /** How well the recording matched the text (fails until it is matched). */
  syncReport(args: { bookId: string }): Promise<SyncReportView>;
  syncAction(args: { bookId: string; action: SyncAction }): Promise<void>;
  /** Forgets the recording and its sync; the audio files are left alone. */
  removeRecording(args: { bookId: string }): Promise<void>;
  /** Plays the book with its recording (once synced) or a speech engine. */
  setNarration(args: { bookId: string; narration: Narration }): Promise<void>;
  recognitionOptions(): Promise<RecognitionOptions>;
  /** Downloads a speech-recognition model (progress on "model://download"). */
  downloadModel(args: { modelId: string }): Promise<void>;
  /** Plays the book's recording from a time on its timeline (needs the recording narration). */
  playRecordingAt(args: { bookId: string; atMs: number }): Promise<void>;
  engineStatus(): Promise<EngineStatusView>;
  bookSettings(args: { bookId: string }): Promise<BookSettings>;
  saveBookSettings(args: { bookId: string; settings: BookSettings }): Promise<void>;
  /** What went wrong with the speech engine lately, newest first (Settings › Speech engine). */
  speechProblems(): Promise<SpeechProblem[]>;
  /** Forgets the speech engine's problems listed so far. */
  clearSpeechProblems(): Promise<void>;
  /** Try again: starts a speech server that failed once more. */
  retrySpeechServer(): Promise<void>;
  /** Stop now: stops the speech server Aoide started; it starts again when speech is needed. */
  stopSpeechServer(): Promise<void>;
  /** Shows the speech server's log (`speech-server.log`) in the file manager. */
  showSpeechServerLog(): Promise<void>;
  /** Registered adapters and the active global profile. */
  engineOptions(): Promise<EngineOptions>;
  /** Voices of the active engine (fails when the engine is offline). */
  engineVoices(): Promise<Voice[]>;
  /**
   * Speaks a sample sentence with `profile` (saved or not) and returns a WAV file. `params` override
   * the profile's; a null voice means the profile's voice, else the engine's first.
   * The WAV goes back as raw bytes (an ArrayBuffer in the UI), not as a JSON number array.
   */
  previewVoice(args: { profile: EngineProfile; voiceId?: string | null; params: { [key in string]: ParamValue }; language?: string | null }): Promise<ArrayBuffer>;
  /**
   * Says a few short paragraphs in the voice books are read in, with `pauses` (as on screen,
   * saved or not) between them, and returns a WAV file as raw bytes.
   */
  previewPauses(args: { pauses: PauseSettings }): Promise<ArrayBuffer>;
  /** Validates and activates a profile; regenerates upcoming audio with it. */
  saveEngineProfile(args: { profile: EngineProfile }): Promise<void>;
  /** Queue whole-book generation (progress arrives via BookSummary.prepare + library://changed). */
  prepareBook(args: { bookId: string }): Promise<void>;
  /** Queues one chapter (from 0); it joins the book's entry, or is already covered by a whole book. */
  prepareChapter(args: { bookId: string; chapter: number }): Promise<void>;
  /** How much of each chapter's audio is made, in chapter order. */
  chapterAudio(args: { bookId: string }): Promise<ChapterAudioView[]>;
  /** Pause or resume a queued book's preparation (remembered across restarts). */
  setPreparePaused(args: { bookId: string; paused: boolean }): Promise<void>;
  cancelPrepare(args: { bookId: string }): Promise<void>;
  /**
   * Throw away one sentence's audio and make it again as a new take, from the playhead put on
   * it (a book not being listened to is listened to from there).
   */
  sayAgain(args: { bookId: string; segmentId: string }): Promise<void>;
  /** Delete the book's generated audio so it is regenerated. */
  regenerateBook(args: { bookId: string }): Promise<void>;
}

/** Each `CommandApi` method's command. */
export const COMMANDS = {
  logUi: "log_ui",
  appInfo: "app_info",
  appSettings: "app_settings",
  updateAppSettings: "update_app_settings",
  preferences: "preferences",
  saveAppearance: "save_appearance",
  saveArticle: "save_article",
  refetchArticle: "refetch_article",
  article: "article",
  browserStatus: "browser_status",
  downloadBrowser: "download_browser",
  cancelBrowserDownload: "cancel_browser_download",
  removeBrowser: "remove_browser",
  sites: "sites",
  setSiteRule: "set_site_rule",
  signOutSite: "sign_out_site",
  forgetSite: "forget_site",
  clearBrowserData: "clear_browser_data",
  startSignIn: "start_sign_in",
  signIn: "sign_in",
  finishSignIn: "finish_sign_in",
  cancelSignIn: "cancel_sign_in",
  showSignInWindow: "show_sign_in_window",
  staySignedIn: "stay_signed_in",
  chooseBrowser: "choose_browser",
  scanForBooks: "scan_for_books",
  cancelScan: "cancel_scan",
  discoveredBooks: "discovered_books",
  scanForAudiobooks: "scan_for_audiobooks",
  audiobookMatches: "audiobook_matches",
  mergeAudiobook: "merge_audiobook",
  unmatchedAudiobooks: "unmatched_audiobooks",
  cancelAudiobookScan: "cancel_audiobook_scan",
  lookForAudiobooks: "look_for_audiobooks",
  pairAudiobooks: "pair_audiobooks",
  fetchDiscoveredDetails: "fetch_discovered_details",
  importDiscovered: "import_discovered",
  chooseBooks: "choose_books",
  chooseFolder: "choose_folder",
  chooseAudio: "choose_audio",
  feeds: "feeds",
  addLink: "add_link",
  subscribe: "subscribe",
  unsubscribe: "unsubscribe",
  feedEntries: "feed_entries",
  openFeedEntry: "open_feed_entry",
  saveFeedEntry: "save_feed_entry",
  markEntryRead: "mark_entry_read",
  markAllRead: "mark_all_read",
  saveToLibrary: "save_to_library",
  openFeedSite: "open_feed_site",
  checkFeeds: "check_feeds",
  chooseOpml: "choose_opml",
  chooseOpmlTarget: "choose_opml_target",
  importOpml: "import_opml",
  exportOpml: "export_opml",
  listBooks: "list_books",
  home: "home",
  addAudiobook: "add_audiobook",
  importFiles: "import_files",
  rescanLibrary: "rescan_library",
  removeBook: "remove_book",
  showInFolder: "show_in_folder",
  openOriginal: "open_original",
  openBook: "open_book",
  bookOverview: "book_overview",
  chapter: "chapter",
  collections: "collections",
  createCollection: "create_collection",
  renameCollection: "rename_collection",
  deleteCollection: "delete_collection",
  tags: "tags",
  setBookTags: "set_book_tags",
  tagWithModel: "tag_with_model",
  tagUntaggedWithModel: "tag_untagged_with_model",
  setBookCollections: "set_book_collections",
  storageReport: "storage_report",
  cleanUnusedFiles: "clean_unused_files",
  searchBook: "search_book",
  bookDetails: "book_details",
  lookupMetadata: "lookup_metadata",
  applyMetadata: "apply_metadata",
  exportBook: "export_book",
  exportReadAlong: "export_read_along",
  play: "play",
  pause: "pause",
  resume: "resume",
  stop: "stop",
  setSpeed: "set_speed",
  setVolume: "set_volume",
  setSleepTimer: "set_sleep_timer",
  skipChapter: "skip_chapter",
  seek: "seek",
  skip: "skip",
  markFinished: "mark_finished",
  markUnread: "mark_unread",
  setReadingPlace: "set_reading_place",
  readingActivity: "reading_activity",
  setMode: "set_mode",
  bookmarks: "bookmarks",
  addBookmark: "add_bookmark",
  updateBookmarkNote: "update_bookmark_note",
  deleteBookmark: "delete_bookmark",
  bookListening: "book_listening",
  activityStats: "activity_stats",
  inspectRecording: "inspect_recording",
  attachRecording: "attach_recording",
  recording: "recording",
  recordings: "recordings",
  syncReport: "sync_report",
  syncAction: "sync_action",
  removeRecording: "remove_recording",
  setNarration: "set_narration",
  recognitionOptions: "recognition_options",
  downloadModel: "download_model",
  playRecordingAt: "play_recording_at",
  engineStatus: "engine_status",
  bookSettings: "book_settings",
  saveBookSettings: "save_book_settings",
  speechProblems: "speech_problems",
  clearSpeechProblems: "clear_speech_problems",
  retrySpeechServer: "retry_speech_server",
  stopSpeechServer: "stop_speech_server",
  showSpeechServerLog: "show_speech_server_log",
  engineOptions: "engine_options",
  engineVoices: "engine_voices",
  previewVoice: "preview_voice",
  previewPauses: "preview_pauses",
  saveEngineProfile: "save_engine_profile",
  prepareBook: "prepare_book",
  prepareChapter: "prepare_chapter",
  chapterAudio: "chapter_audio",
  setPreparePaused: "set_prepare_paused",
  cancelPrepare: "cancel_prepare",
  sayAgain: "say_again",
  regenerateBook: "regenerate_book",
} as const satisfies Record<keyof CommandApi, keyof Commands>;
