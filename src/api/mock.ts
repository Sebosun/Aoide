/** The browser preview's stand-in core. It serves what the real core made for the preview
 *  (`src/tests/fixtures/`, written by src-tauri/src/core/fixtures.rs) and keeps the user's edits
 *  in memory. Where it plays out a rule of the core's, `src/tests/mock.test.ts` replays the
 *  scenarios the core ran (`scenarios.json`) and the core's stage tables (`stages.json`) on it,
 *  so it answers as the core did. */
import type {
  ArticleView,
  SpeechProblem,
  AddedLink,
  EntryView,
  FeedView,
  SubscribeResult,
  ProblemView,
  AudiobookMatch,
  AudiobookSuggestion,
  RecordingView,
  SyncStage,
  InspectionView,
  ModelDownloadView,
  BrowserDownloadView,
  BrowserStatusView,
  SiteView,
  SignInView,
  SavedLink,
  InboxBook,
  FinishedItem,
  InboxView,
  SyncReportView,
  SyncAction,
  RecordingAction,
  PrepareAction,
  PrepareState,
  PrepareView,
  Preferences,
  Theme,
  Candidate,
  ScanProgress,
  DiscoveryImportProgress,
  AoideApi,
  BookSummary,
  BookView,
  ChapterView,
  PlayerState,
  EngineStatusView,
  GenerationState,
  EngineDescriptor,
  EngineProfile,
  BookSettings,
  Bookmark,
  HighlightView,
  Pronunciation,
  AppSettings,
  BookDetails,
  MetadataMatch,
  ExportProgress,
  SearchHit,
  HomeView,
} from "@/api/types";
import type * as Wire from "@/api/contract";
import type {
  BackupProgressView,
  DetailField,
  LibraryHits,
  LinkImportView,
  PassageHitView,
  QueueView,
  WhyNot,
} from "@/api/contract";
import fixtureLibrary from "@/tests/fixtures/library.json";
import fixtureViews from "@/tests/fixtures/book-views.json";
import fixturePlayer from "@/tests/fixtures/player.json";
import fixtureGenerationPlaying from "@/tests/fixtures/generation-playing.json";
import fixtureRecording from "@/tests/fixtures/recording.json";
import fixtureAudiobook from "@/tests/fixtures/audiobook.json";
import fixtureArticle from "@/tests/fixtures/article.json";
import fixtureEmail from "@/tests/fixtures/email.json";
import fixtureFeeds from "@/tests/fixtures/feeds.json";
import fixtureStages from "@/tests/fixtures/stages.json";
import fixtureChapters from "@/tests/fixtures/chapters.json";
import fixtureSpeechServer from "@/tests/fixtures/speech-server.json";
import fixtureWeb from "@/tests/fixtures/web.json";
import fixtureBrowser from "@/tests/fixtures/browser.json";
import fixtureFound from "@/tests/fixtures/found.json";
import fixtureThemes from "@/tests/fixtures/themes.json";
import { ApiError } from "@/api/errors";

/** A refusal, as the core answers one: an error of a kind (its `String` errors are `internal`). */
const refused = (message: string) => new ApiError({ kind: "internal", message });

/** A 10 ms silent 16-bit mono WAV, for previews. */
/** A picture for the mock's illustrated book (the core serves files from the book). */
const MOCK_PICTURE =
  "data:image/svg+xml," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="640" height="360" viewBox="0 0 640 360"><rect width="640" height="360" fill="#2b3a55"/><rect y="250" width="640" height="110" fill="#1d2738"/><rect x="300" y="110" width="40" height="150" fill="#e8e1d3"/><polygon points="292,110 348,110 320,80" fill="#c45a3c"/><circle cx="320" cy="96" r="9" fill="#ffd36b"/></svg>`,
  );

function silentWav(): ArrayBuffer {
  const samples = 240,
    view = new DataView(new ArrayBuffer(44 + samples * 2));
  const text = (at: number, value: string) => [...value].forEach((c, i) => view.setUint8(at + i, c.charCodeAt(0)));
  text(0, "RIFF");
  view.setUint32(4, 36 + samples * 2, true);
  text(8, "WAVE");
  text(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, 24_000, true);
  view.setUint32(28, 48_000, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  text(36, "data");
  view.setUint32(40, samples * 2, true);
  return view.buffer;
}
/** The views the core made for the preview's books (see src-tauri/src/core/fixtures.rs), with
 *  covers as the window sees them. */
const coreBooks = (): BookSummary[] =>
  (structuredClone(fixtureLibrary) as unknown as Wire.BookSummary[]).map(({ coverPath, ...book }) => ({
    ...book,
    coverUrl: coverPath,
  }));
const coreView = (id: string): BookView | undefined =>
  structuredClone(
    id === fixtureAudiobook.view.id
      ? (fixtureAudiobook.view as unknown as BookView)
      : id === fixtureArticle.view.id
        ? (fixtureArticle.view as unknown as BookView)
        : id === fixtureEmail.view.id
          ? (fixtureEmail.view as unknown as BookView)
          : (fixtureViews as unknown as Record<string, BookView>)[id],
  );
/** A saved link as the core reads it (core/articles.rs `parse_link`). */
const parseLink = (link: string) => {
  const text = link.trim();
  const invalid = () => refused("That is not a web address.");
  if (!text || /\s/.test(text)) throw invalid();
  let url: URL;
  try {
    url = new URL(/^[a-z][a-z0-9+.-]*:/i.test(text) ? text : `https://${text}`);
  } catch {
    throw invalid();
  }
  if (!/^https?:$/.test(url.protocol) || !url.hostname) throw invalid();
  return url;
};
/** What the core says of a text-only operation on an audiobook. */
const NO_TEXT = "Not available for an audiobook: it has no text.";

/** The same API with every request (not the event subscriptions) taking `ms` longer. */
export function slowed<T extends AoideApi>(api: T, ms: number): T {
  if (ms <= 0) return api;
  return new Proxy(api, {
    get(target, key, receiver) {
      const value = Reflect.get(target, key, receiver);
      if (typeof value !== "function" || String(key).startsWith("on")) return value;
      return (...args: unknown[]) =>
        new Promise((resolve) => setTimeout(resolve, ms)).then(() =>
          (value as (...a: unknown[]) => unknown).apply(target, args),
        );
    },
  });
}

/** Original, synthetic preview books. No personal books or network requests. `search` holds
 *  the preview's parameters (the page's own by default). */
export function createMockApi(search = typeof location === "undefined" ? "" : location.search) {
  /** The speech server Aoide starts, as the core showed it (`server=starting|running|stopped|failed`). */
  const serverViews = fixtureSpeechServer as Record<string, EngineStatusView>;
  const serverParam = new URLSearchParams(search).get("server");
  const engine: EngineStatusView =
    serverParam && serverViews[serverParam]
      ? structuredClone(serverViews[serverParam])
      : {
          profileName: "Local Chatterbox",
          state: "ready",
          detail: "Browser preview · simulated audio",
          wanted: false,
          server: null,
        };
  /** Shows the core's view of the server in `stage`, as Try again or Stop now lead to. */
  const showServer = (stage: "starting" | "stopped") => {
    if (!engine.server) return;
    Object.assign(engine, structuredClone(serverViews[stage]));
    engineHandlers.forEach((h) => h({ ...engine }));
  };
  const books = coreBooks();
  /** Home as the core made it for these books; `setHome` replaces it (an empty library). */
  /** Home for another library, set by a test; else Home is worked out from the books. */
  let homeOverride: HomeView | null = null;
  /** A series' name as the core matches it (core/facets.rs `series_key`). */
  const seriesKey = (name: string) => name.trim().toLowerCase();
  /** The book after the most recently finished item in its series, among `among` (core/facets.rs
   *  `next_in_series`). */
  const nextInSeries = (items: BookSummary[], among: BookSummary[]) => {
    const last = items
      .filter((b) => b.finishedAt !== null)
      .reduce<BookSummary | null>((m, b) => (!m || b.finishedAt! > m.finishedAt! ? b : m), null);
    const after = last?.series?.position;
    if (!last?.series || after == null) return null;
    const key = seriesKey(last.series.name);
    return (
      among
        .filter((b) => b.series && seriesKey(b.series.name) === key && (b.series.position ?? -Infinity) > after)
        .sort((a, b) => a.series!.position! - b.series!.position!)[0] ?? null
    );
  };
  /** Home's lists from the library's items, as the core makes them (core/facets.rs `home`):
   *  started and unfinished by last opened, never started and every other item by newest added;
   *  archived items in none. */
  const homeOf = (items: BookSummary[]): HomeView => {
    const usable = items.filter((b) => b.status.kind !== "error" && !b.archived),
      started = (b: BookSummary) => b.progress !== null,
      ids = (list: BookSummary[], n: number) => list.slice(0, n).map((b) => b.id);
    const continueIds = ids(
      usable
        .filter((b) => started(b) && b.progress!.fraction < 1 && b.readState !== "finished")
        .sort((a, b) => (b.lastOpenedAt ?? b.addedAt) - (a.lastOpenedAt ?? a.addedAt)),
      3,
    );
    const newest = [...usable].sort((a, b) => b.addedAt - a.addedAt);
    const unstarted = newest.filter((b) => !started(b));
    // The next book of the series just finished goes first, if it is not started.
    const next = nextInSeries(items, usable);
    if (next && !started(next)) (unstarted.splice(unstarted.indexOf(next), 1), unstarted.unshift(next));
    const upNextIds = ids(unstarted, 5);
    const recentIds = ids(
      newest.filter((b) => !continueIds.includes(b.id) && !upNextIds.includes(b.id)),
      8,
    );
    return { continueIds, upNextIds, recentIds };
  };
  const shelves: { id: number; name: string; createdAt: number }[] = [];
  const shelfName = (name: string, except?: number) => {
    const trimmed = name.trim();
    if (!trimmed) throw refused("a collection needs a name");
    if (shelves.some((s) => s.id !== except && s.name.toLowerCase() === trimmed.toLowerCase()))
      throw refused(`there is already a collection called “${trimmed}”`);
    return trimmed;
  };
  /** Each preview book's chapters, as the core reads them (`chapters.json`). */
  const chapters = structuredClone(fixtureChapters) as unknown as ChapterView[][];
  const descriptors: EngineDescriptor[] = [
    {
      kind: "chatterbox",
      name: "Chatterbox TTS Server",
      adapterVersion: 1,
      languages: ["en", "fr", "de"],
      wordTimings: false,
      maxChars: 300,
      cheapVoiceSwitch: true,
      params: [
        {
          key: "temperature",
          label: "Temperature",
          kind: { type: "float", min: 0.05, max: 2, step: 0.05 },
          default: 0.8,
        },
        {
          key: "exaggeration",
          label: "Exaggeration",
          kind: { type: "float", min: 0.25, max: 2, step: 0.05 },
          default: 0.5,
        },
        { key: "seed", label: "Seed (0 = random)", kind: { type: "int", min: 0, max: 4294967295 }, default: 0 },
      ],
    },
    {
      kind: "openai-speech",
      name: "OpenAI-compatible speech server",
      adapterVersion: 1,
      languages: null,
      wordTimings: false,
      maxChars: 1000,
      cheapVoiceSwitch: false,
      params: [{ key: "speed", label: "Speed", kind: { type: "float", min: 0.25, max: 4, step: 0.05 }, default: 1 }],
    },
  ];
  let profile: EngineProfile = {
    id: "default",
    name: "Local Chatterbox",
    kind: "chatterbox",
    baseUrl: "http://127.0.0.1:8004",
    model: null,
    network: "loopback",
    voiceId: "Emily.wav",
    params: {},
    // With a server Aoide starts, the command that starts it.
    launch: engine.server
      ? {
          program: "/home/reader/chatterbox/.venv/bin/python",
          args: ["server.py"],
          workingDir: "/home/reader/chatterbox",
          startTimeoutSeconds: engine.server.startTimeoutSeconds,
          idleMinutes: engine.server.idleMinutes ?? 0,
        }
      : null,
  };
  const settings = new Map<string, BookSettings>();
  /** A book's settings; nobody has changed them for most books. */
  const settingsOf = (id: string): BookSettings =>
    settings.get(id) ?? {
      voiceId: null,
      params: {},
      language: null,
      readNotes: false,
      narration: null,
      speakMissing: false,
      mode: null,
      leftOut: [],
      playbackSpeed: null,
      shortenSilences: null,
      dialogueVoiceId: null,
    };
  const bookmarks: Bookmark[] = [];
  let nextBookmark = 1;
  const highlights: (HighlightView & { bookId: string })[] = [];
  let nextHighlight = 1;
  const pronunciations: Pronunciation[] = [];
  let nextPronunciation = 1;
  /** Words separated by single spaces, if any has a letter or digit (as the core keeps them). */
  const listWords = (text: string) => {
    const words = text.split(/\s+/).filter(Boolean).join(" ");
    return /[\p{L}\p{N}]/u.test(words) ? words : null;
  };
  /** The sentences of book `id` the speech engine says that hold `written` as a whole word (a
   *  plural or possessive too), as the core's pronunciation rule matches it. */
  const sentencesSaying = (id: string, written: string, matchCase: boolean) => {
    const escaped = written.replace(/[.*+?^${}()|[\]\\]/g, "\\$&").replace(/ /g, "\\s+");
    const word = new RegExp(`(^|[^\\p{L}\\p{N}])${escaped}s?(?=$|[^\\p{L}\\p{N}])`, matchCase ? "u" : "iu");
    const list = chapters[Number(id.split("-")[1])] ?? [];
    return list.flatMap((chapter, c) =>
      chapter.blocks
        .flatMap((b) => b.segments)
        .filter((s) => s.canSayAgain && word.test(s.words.join(" ")))
        .map((s) => ({ chapter: c, segmentId: s.id })),
    );
  };
  const listened = new Map<string, number>([
    ["book-0", 5_400_000],
    ["book-1", 1_200_000],
  ]);
  let appSettings: AppSettings = {
    metadataLookup: true,
    cacheLimitBytes: null,
    aligner: null,
    librarySort: "recent",
    libraryTags: { include: [], exclude: [] },
    tagging: { enabled: false, baseUrl: "http://127.0.0.1:11434", model: "", network: "loopback" },
    scan: { roots: [], exclude: [], includeHidden: [], onStartup: false },
    resumePrepare: true,
    recognition: {
      engine: "builtIn",
      server: { baseUrl: "http://127.0.0.1:8000", model: "", network: "loopback" },
      modelSize: "base",
      device: "auto",
      threads: 0,
      resumeOnStart: true,
    },
    audiobooks: { roots: [], exclude: [], includeHidden: [], onAdd: true },
    verifyAudiobooks: false,
    readingWpm: 230,
    feeds: { checkMinutes: 60, keepReadDays: 30, keepUnreadDays: 90 },
    browser: {
      use: ["missing", "ready"].includes(new URLSearchParams(search).get("browser") ?? "") ? "fallback" : "off",
      path: null,
      idleSeconds: 60,
    },
    pauses: { paragraphMs: 250, breakMs: 1200, headingMs: 700 },
    stepBackOnResume: true,
    notices: true,
    inbox: null,
    archiveFinishedArticles: false,
    close: "ask",
    backup: { folder: null, everyDays: 7, keep: 5, books: false },
    searchText: true,
    shortenSilences: false,
  };
  /** The inbox as the core shows it: watched once chosen; `?inbox=unavailable` chooses one that is gone. */
  if (new URLSearchParams(search).get("inbox") === "unavailable") appSettings.inbox = "/media/reader/Stick/Books";
  const inboxGone = () =>
    new URLSearchParams(search).get("inbox") === "unavailable" && appSettings.inbox === "/media/reader/Stick/Books";
  const inboxView = (): InboxView => ({
    folder: appSettings.inbox,
    state: appSettings.inbox === null ? "off" : inboxGone() ? "unavailable" : "watching",
  });
  const inboxHandlers = new Set<(inbox: InboxView) => void>();
  /** The browser for articles, as the core finds it (core/browsing.rs). `?browser=off|missing|ready`:
   *  off (the default) finds Chromium but does not run it; missing finds none; ready finds one. */
  const systemChromium = new URLSearchParams(search).get("browser") !== "missing";
  let browserDownloaded = false;
  let browserDownload: BrowserDownloadView | null = null;
  let cancelBrowser = false;
  const browserDownloadHandlers = new Set<(d: BrowserDownloadView) => void>();
  /** The pinned Chrome for Testing, as the core offers it (`browser.json`). */
  const BROWSER_VERSION = fixtureBrowser.version;
  const BROWSER_BYTES = fixtureBrowser.bytes;
  const browserStatus = (): BrowserStatusView => {
    const on = appSettings.browser.use !== "off";
    const binary: BrowserStatusView["binary"] = appSettings.browser.path
      ? { kind: "found", path: appSettings.browser.path, version: on ? "Chromium 1.0" : null, downloaded: false }
      : systemChromium
        ? { kind: "found", path: "/usr/bin/chromium", version: on ? "Chromium 154.0.8037.97" : null, downloaded: false }
        : browserDownloaded
          ? {
              kind: "found",
              path: "~/.local/share/aoide/browser/chrome/chrome-linux64/chrome",
              version: BROWSER_VERSION,
              downloaded: true,
            }
          : { kind: "missing" };
    return {
      use: appSettings.browser.use,
      binary,
      download: browserDownload && { ...browserDownload },
      offeredVersion: BROWSER_VERSION,
      downloaded: browserDownloaded,
    };
  };
  /** Sites, as the core lists them (core/sites.rs): rows joined with rules, by host. */
  const sites = new Map<string, SiteView>([
    [
      "harbour.example",
      {
        host: "harbour.example",
        session: { kind: "signedIn", at: Date.parse("2026-11-02T09:00:00Z") },
        fetch: null,
        siteData: "keep",
        waitingArticle: null,
      },
    ],
    [
      "lantern.example",
      {
        host: "lantern.example",
        session: { kind: "askedFor", reason: "paywall", at: Date.parse("2026-11-14T18:30:00Z") },
        fetch: "browser",
        siteData: "clear_before_fetch",
        waitingArticle: null,
      },
    ],
  ]);
  const siteHost = (host: string) =>
    host
      .trim()
      .toLowerCase()
      .replace(/^www\./, "");
  const browserOff = () => {
    if (appSettings.browser.use === "off") throw refused("The browser is off. Turn it on in Settings › Sites.");
  };
  const tellBrowserDownload = (change: Partial<BrowserDownloadView>) => {
    browserDownload = { ...browserDownload!, ...change };
    browserDownloadHandlers.forEach((h) => h({ ...browserDownload! }));
  };
  /** Minutes read silently, per book. */
  const readMs = new Map<string, number>([["book-1", 1_800_000]]);
  /** What "Find books" finds in the preview's folders, as the core found it (`found.json`). */
  const found = structuredClone(fixtureFound.books) as unknown as Candidate[];
  /** What a scan found; like the app, a book that has a recording is not offered another. */
  const listed = () => structuredClone(found).map((c) => (recordings.has(c.id) ? { ...c, audiobook: [] } : c));
  const query = new URLSearchParams(search);
  const mode = query.get("theme");
  /** `?sleep=again|again-chapter`: a 15-minute (or end-of-chapter) sleep timer has just paused
   *  playback; the first play starts it again. */
  let sleptJustNow = ["again", "again-chapter"].includes(query.get("sleep") ?? "") ? query.get("sleep") : null;
  /** The speech engine's problems (core/speech_problems.rs), newest first; one to show with
   *  `?speech-problems`. */
  let speechProblems: SpeechProblem[] = query.has("speech-problems")
    ? [
        {
          message: "The speech engine is unavailable",
          detail: "engine is unavailable: error sending request for url (http://127.0.0.1:8883/v1/audio/speech)",
          firstMs: Date.now() - 600_000,
          lastMs: Date.now() - 60_000,
          times: 12,
        },
      ]
    : [];
  let preferences: Preferences = {
    appearance: {
      mode: mode === "light" || mode === "dark" ? mode : "auto",
      theme: "default",
      textSize: 2,
      readerMode: query.get("mode") === "lines" ? "lines" : "page",
      zoom: Number(query.get("zoom")) || 1,
      timeLeft: query.get("timeleft") === "book" ? "book" : "chapter",
      readerFont: (["sans", "legible", "system"] as const).find((font) => font === query.get("font")) ?? "serif",
      lineSpacing: Number(query.get("spacing") ?? 2),
      textWidth: Number(query.get("width") ?? 2),
      justify: query.has("justify"),
      hyphenate: query.has("hyphenate"),
    },
    themes: [
      ...(structuredClone(fixtureThemes) as Theme[]),
      {
        name: "dusk",
        title: "dusk",
        builtin: false,
        light: { accent: "#B4472B" },
        dark: { accent: "#FFB27A", ground: "#120E1C" },
        problems: [],
      },
    ],
    configDir: "~/.config/aoide",
    problems: [],
  };
  /** Recordings paired with books, like the app (core/recording.rs): one per book, each with an
   *  explicit sync stage. `?recording=syncing|ready|review|missing|model|mismatch|failed` gives
   *  book-0 a recording in that stage. */
  const recordings = new Map<string, RecordingView>();
  /** A recording before the core's decisions about it are added. */
  type Undecided = Omit<RecordingView, "listenable" | "narrates" | "actions">;
  /** The core's sync stage machine: where each event leads from each stage, and what it offers. */
  const syncStages = fixtureStages.sync as unknown as Record<
    SyncStage,
    {
      next: Record<SyncAction | "promote", SyncStage>;
      actions: RecordingAction[];
      listenableActions: RecordingAction[];
      /** Whether it can be listened to with none of it transcribed, and with some. */
      listenable: [boolean, boolean];
    }
  >;
  const syncHandlers = new Set<(r: RecordingView) => void>();
  const downloadHandlers = new Set<(d: ModelDownloadView) => void>();
  const problemHandlers = new Set<(p: ProblemView) => void>();
  const downloaded = new Set<string>(["base.en"]);
  const hour = 3_600_000;
  /** What the core decides about a recording (core/recording.rs): what is synced so far can be
   *  listened to, it narrates the book unless speech was chosen, and what can be done about it (the core's
   *  own table, from the stages fixture). */
  const decide = (r: Undecided): RecordingView => {
    const stage = syncStages[r.stage],
      listenable = stage.listenable[r.transcribedMs > 0 ? 1 : 0];
    return {
      ...r,
      listenable,
      // Unless speech was chosen: whoever paired an audiobook wants to hear it.
      narrates: listenable && settingsOf(r.bookId).narration !== "speech",
      actions: listenable ? stage.listenableActions : stage.actions,
    };
  };
  /** The recording the core synced (its fixture), as it stands in `stage`. */
  const recordingIn = (bookId: string, stage: SyncStage): Undecided => {
    const synced = structuredClone(fixtureRecording) as RecordingView,
      matched = stage === "ready" || stage === "poorMatch";
    return {
      ...synced,
      bookId,
      stage,
      error: stage === "failed" ? "the recording could not be read" : null,
      transcribedMs: matched ? synced.totalMs : stage === "transcribing" ? Math.round(synced.totalMs * 0.37) : 0,
      remainingMs: stage === "transcribing" ? 14 * 60_000 : null,
      position: stage === "transcribing" || stage === "waiting" ? 0 : null,
      summary:
        stage === "poorMatch"
          ? { ...synced.summary!, matchedWords: Math.round(synced.summary!.words * 0.59) }
          : matched
            ? synced.summary
            : null,
    };
  };
  const emitSync = (undecided: Undecided) => {
    const recording = decide(undecided);
    recordings.set(recording.bookId, recording);
    const book = books.find((b) => b.id === recording.bookId);
    if (book) book.recording = recording.stage;
    syncHandlers.forEach((h) => h(structuredClone(recording)));
  };
  let syncTimer: ReturnType<typeof setInterval> | undefined;
  /** Walks a new recording through checking, transcribing and matching, a little at a time. */
  const simulateSync = (bookId: string) => {
    clearInterval(syncTimer);
    syncTimer = setInterval(() => {
      const r = recordings.get(bookId);
      if (!r) return clearInterval(syncTimer);
      if (r.stage === "checking") emitSync({ ...r, stage: "transcribing", position: 0 });
      else if (r.stage === "waiting") emitSync({ ...r, stage: syncStages.waiting.next.promote, position: 0 });
      else if (r.stage === "transcribing") {
        const transcribedMs = Math.min(r.totalMs, r.transcribedMs + 45 * 60_000);
        const remainingMs = Math.round((r.totalMs - transcribedMs) / 27);
        emitSync(
          transcribedMs >= r.totalMs
            ? { ...r, transcribedMs, stage: "matching", remainingMs: null, position: null }
            : { ...r, transcribedMs, remainingMs },
        );
      } else if (r.stage === "matching") {
        clearInterval(syncTimer);
        if (!settingsOf(bookId).narration) settings.set(bookId, { ...settingsOf(bookId), narration: "recording" });
        emitSync({ ...recordingIn(bookId, "ready"), files: r.files, totalMs: r.totalMs });
        changed();
      }
    }, 400);
  };
  const previewStage = (
    {
      syncing: "transcribing",
      ready: "ready",
      review: "poorMatch",
      missing: "missing",
      model: "needsModel",
      mismatch: "sampleMismatch",
      failed: "failed",
    } as Record<string, SyncStage>
  )[query.get("recording") ?? ""];
  if (previewStage) {
    if (previewStage === "ready") {
      settings.set("book-0", { ...settingsOf("book-0"), narration: "recording" });
      books[0].audio = "recording";
    }
    recordings.set("book-0", decide(recordingIn("book-0", previewStage)));
    books[0].recording = previewStage;
  }
  const configHandlers = new Set<(p: Preferences) => void>();
  const emitConfig = (
    next: Partial<Omit<Preferences, "appearance">> & { appearance?: Partial<Preferences["appearance"]> },
  ) => {
    preferences = { ...preferences, ...next, appearance: { ...preferences.appearance, ...next.appearance } };
    configHandlers.forEach((h) => h(structuredClone(preferences)));
  };
  let scanned = false;
  const scanHandlers = new Set<(p: ScanProgress) => void>();
  const detailHandlers = new Set<(id: string) => void>();
  const importHandlers = new Set<(p: DiscoveryImportProgress) => void>();
  /** Find audiobooks: what a search of ~/Audiobooks finds for the books without a recording.
   *  Verifying itself (the quick check) is the core's; here a verified match only starts. */
  const audiobookScanHandlers = new Set<(p: ScanProgress) => void>();
  const matchHandlers = new Set<(m: AudiobookMatch[]) => void>();
  let audiobookMatches: AudiobookMatch[] = [];
  /** What a search finds that comes close to no book: the preview's audiobook on its own. */
  const WINTER_RADIO = "~/Audiobooks/Ines Morrow - Winter Radio";
  let unmatched: AudiobookSuggestion[] = [];
  /** What "Find audiobooks" finds for each book, as the core found it (`found.json`). */
  const kept: Record<string, AudiobookMatch["candidates"]> = Object.fromEntries(
    fixtureFound.audiobooks.map((m) => [m.bookId, m.candidates]),
  );
  const emitMatches = () => matchHandlers.forEach((h) => h(structuredClone(audiobookMatches)));
  const details = new Map<string, BookDetails>();
  /** As the core does (core/details.rs): a field corrected by hand shows over the file's,
   *  which is kept so that `null` sets it back. */
  const fileDetails = new Map<
    string,
    { title: string; authors: string[]; coverUrl: string | null; series: BookSummary["series"] }
  >();
  const edits = new Map<string, { published?: string; series?: string; seriesPosition?: number }>();
  /** A picture chosen as a cover: the preview cannot read the file, so it shows a plain one. */
  const ownCover = (path: string) =>
    `data:image/svg+xml,${encodeURIComponent(
      `<svg xmlns="http://www.w3.org/2000/svg" width="210" height="300"><rect width="210" height="300" fill="#3d5a80"/><text x="16" y="280" fill="#fff" font-family="sans-serif" font-size="14">${path.split("/").pop()}</text></svg>`,
    )}`;
  const exportHandlers = new Set<(p: ExportProgress) => void>();
  const playerHandlers = new Set<(s: PlayerState) => void>();
  const engineHandlers = new Set<(s: EngineStatusView) => void>();
  /** As the core does: once the user asks for audio, the engine's status is theirs to see. */
  const wantEngine = () => {
    if (engine.wanted) return;
    engine.wanted = true;
    engineHandlers.forEach((h) => h({ ...engine }));
  };
  const generationHandlers = new Set<(s: GenerationState) => void>();
  const libraryHandlers = new Set<() => void>();
  let state = structuredClone(fixturePlayer.stopped) as PlayerState;
  /** The usual speed, which books without their own play at (`[player].speed`). */
  let usualSpeed = state.speed;
  /** A speed as the core keeps it: 0.5–3, to two decimals. */
  const keptSpeed = (speed: number) =>
    Number.isFinite(speed) ? Math.round(Math.min(3, Math.max(0.5, speed)) * 100) / 100 : 1;
  /** Whether book `id` is being listened to (playing or paused). */
  const isPlaying = (id: string) => state.bookId === id && state.status !== "stopped";
  /** The speed book `id` plays at, as the core chooses it when the book starts. */
  const speedOf = (id: string): Pick<PlayerState, "speed" | "speedScope"> => {
    const own = settingsOf(id).playbackSpeed;
    return own === null ? { speed: usualSpeed, speedScope: "usual" } : { speed: own, speedScope: "book" };
  };
  let timer: ReturnType<typeof setInterval> | undefined;
  /** A sentence said again is made after this long in the preview. */
  const REMAKE_MS = 1_800;
  let remaking: ReturnType<typeof setTimeout> | undefined;
  /** Times left as the core's state gives them, at 1x at the start of each sentence of each
   *  book (player.json's `at`). */
  const times = fixturePlayer.at as Record<
    string,
    Record<string, { chapterLeftMs: number | null; bookLeftMs: number | null; chapterMs: number | null }>
  >;
  /** A time left at `speed`, in whole seconds rounded down, as the core gives it. */
  const leftAt = (ms: number | null, speed: number) =>
    ms === null ? null : Math.floor(ms / Math.max(0.1, speed) / 1000) * 1000;
  /** The core's times left for the sentence `next` is at (a timeline brings its own). */
  const withTimes = (next: PlayerState): PlayerState => {
    if (next.positionMs !== null) return next;
    const at = next.bookId && next.segmentId ? times[next.bookId]?.[next.segmentId] : undefined;
    return {
      ...next,
      chapterLeftMs: leftAt(at?.chapterLeftMs ?? null, next.speed),
      bookLeftMs: leftAt(at?.bookLeftMs ?? null, next.speed),
      chapterMs: at?.chapterMs ?? null,
    };
  };
  /** Sends `next` as it is (tests set any state with it). */
  const sendPlayer = (next: PlayerState) => {
    state = { ...next };
    playerHandlers.forEach((h) => h({ ...state }));
  };
  const emitPlayer = (next: PlayerState) => {
    const timed = withTimes(next);
    sendPlayer({ ...timed, upNextTitle: upNextTitle(timed) });
  };
  const emitGeneration = (next: GenerationState) => generationHandlers.forEach((h) => h({ ...next }));
  const changed = () => libraryHandlers.forEach((h) => h());
  /** As the core after a pronunciation list changed: a prepared book that has the word is
   *  prepared no longer, and in the book listened to the first sentence with it is made again. */
  const remake = (list: string | null, rule: Pronunciation) => {
    for (const b of books) {
      if ((list ?? b.id) !== b.id || b.prepare?.state !== "done") continue;
      if (sentencesSaying(b.id, rule.written, rule.matchCase).length) b.prepare = null;
    }
    changed();
    const id = state.bookId;
    if (!id || (list ?? id) !== id) return;
    const first = sentencesSaying(id, rule.written, rule.matchCase)[0];
    if (!first) return;
    clearTimeout(remaking);
    const generation = { ...(structuredClone(fixtureGenerationPlaying) as GenerationState), bookId: id };
    emitGeneration({ ...generation, generatingSegmentId: first.segmentId });
    remaking = setTimeout(() => emitGeneration(generation), REMAKE_MS);
  };
  /** Saved articles, like the app (core/articles.rs): the preview's article (`article.json`,
   *  book-6) with an explicit fetch stage. `?article=ready|fetching|needsyou-signin|
   *  needsyou-cutshort|needsyou-check|failed` saves it in that stage. */
  const articleHandlers = new Set<(a: ArticleView) => void>();
  const articles = new Map<string, ArticleView>();
  /** Articles with a stored copy (read once fetched, kept when a later fetch fails). */
  const fetched = new Set<string>();
  const isArticle = (id: string) => articles.has(id);
  const setArticle = (id: string, change: Partial<ArticleView>) => {
    const view = { ...articles.get(id)!, ...change };
    articles.set(id, view);
    const b = books.find((b) => b.id === id);
    if (b) {
      const got = fetched.has(id);
      b.title = got ? fixtureArticle.summary.title : new URL(view.canonicalUrl).hostname;
      b.authors = got ? [...fixtureArticle.summary.authors] : [];
      b.article = {
        siteName: (got ? view.siteName : null) ?? new URL(view.canonicalUrl).hostname,
        lead: got ? fixtureArticle.summary.article.lead : null,
        stage: view.stage,
        error: view.error,
      };
    }
    articleHandlers.forEach((h) => h(structuredClone(view)));
    changed();
  };
  /** Fetches article `id` as the core's worker would: fetching, then `outcome`. */
  const fetchArticle = (id: string, outcome: ArticleView["stage"], via: ArticleView["via"] = "http") => {
    setArticle(id, { stage: { kind: "queued" } });
    setTimeout(() => setArticle(id, { stage: { kind: "fetching" } }), 300);
    setTimeout(() => {
      const got = outcome.kind !== "failed";
      if (got) fetched.add(id);
      const view = fixtureArticle.article as unknown as ArticleView;
      setArticle(id, {
        stage: outcome,
        via: got ? via : articles.get(id)!.via,
        error: got ? null : "The site answered with status 503.",
        siteName: got ? view.siteName : null,
        byline: got ? view.byline : null,
        capturedWords: got
          ? outcome.kind === "needsYou" && outcome.reason === "cutShort"
            ? 93
            : view.capturedWords
          : 0,
        declaredWords: outcome.kind === "needsYou" && outcome.reason === "cutShort" ? 1200 : null,
        fetchedAt: got ? Date.now() : articles.get(id)!.fetchedAt,
      });
    }, 1200);
  };
  /** Adds the preview's article in `stage`, as saved and fetched that far. */
  const addArticle = (stage: ArticleView["stage"], url = fixtureArticle.article.url) => {
    const { coverPath, ...summary } = structuredClone(fixtureArticle.summary) as unknown as Wire.BookSummary;
    const id = summary.id;
    if (stage.kind === "ready" || stage.kind === "needsYou") fetched.add(id);
    books.push({ ...summary, coverUrl: coverPath, addedAt: Date.now() });
    articles.set(id, { ...(structuredClone(fixtureArticle.article) as unknown as ArticleView), url, stage });
    setArticle(id, {
      stage,
      error: stage.kind === "failed" ? "The site answered with status 503." : null,
      fetchedAt: fetched.has(id) ? Date.now() : null,
      declaredWords: stage.kind === "needsYou" && stage.reason === "cutShort" ? 1200 : null,
    });
    return id;
  };
  const previewArticle = (
    {
      ready: { kind: "ready" },
      fetching: { kind: "fetching" },
      "needsyou-signin": { kind: "needsYou", reason: "signIn" },
      "needsyou-cutshort": { kind: "needsYou", reason: "cutShort" },
      "needsyou-check": { kind: "needsYou", reason: "check" },
      failed: { kind: "failed" },
    } as Record<string, ArticleView["stage"]>
  )[query.get("article") ?? ""];
  if (previewArticle) addArticle(previewArticle);
  /** The preview's saved email (`email.json`), read at once: it has no page on the web. */
  const addEmail = () => {
    const { coverPath, ...summary } = structuredClone(fixtureEmail.summary) as unknown as Wire.BookSummary;
    if (!articles.has(summary.id)) {
      books.push({ ...summary, coverUrl: coverPath, addedAt: Date.now() });
      articles.set(summary.id, structuredClone(fixtureEmail.article) as unknown as ArticleView);
      fetched.add(summary.id);
      changed();
    }
    return summary.id;
  };
  if (query.get("article") === "email") addEmail();
  /** An article's one chapter: the preview's email's, or the preview article's. */
  const articleChapter = (id: string) =>
    structuredClone(
      id === fixtureEmail.view.id ? fixtureEmail.chapter : fixtureArticle.chapter,
    ) as unknown as ChapterView;
  /** As the core does: an article from an email has no page to fetch, open or sign in to. */
  const onTheWeb = (id: string) => {
    if (articles.get(id)?.onTheWeb === false) throw refused("This came from an email and has no page on the web.");
  };
  /** A sign-in, as the core runs it (core/signin.rs): opening, waiting for the user, fetching
   *  with the session, done. `?signin=waiting|fetching` opens one on the preview's article. */
  let signIn: SignInView | null = null;
  const signInHandlers = new Set<(v: SignInView) => void>();
  const tellSignIn = (change: Partial<SignInView>) => {
    signIn = { ...signIn!, ...change };
    signInHandlers.forEach((h) => h(structuredClone(signIn!)));
  };
  const signInOpen = () => signIn !== null && ["opening", "waiting", "fetching"].includes(signIn.stage.kind);
  const openSignIn = (id: string) => {
    const article = articles.get(id);
    if (!article) throw refused("Not an article.");
    signIn = {
      articleId: id,
      site: new URL(article.canonicalUrl).hostname.replace(/^www\./, ""),
      reason: article.stage.kind === "needsYou" ? article.stage.reason : null,
      stage: { kind: "opening" },
      staySignedIn: true,
      result: null,
    };
  };
  /** The window's page, fetched as the article: it is whole now. */
  const fetchSignedIn = () => {
    tellSignIn({ stage: { kind: "fetching" } });
    setTimeout(() => {
      const flow = signIn!;
      fetched.add(flow.articleId);
      setArticle(flow.articleId, { stage: { kind: "ready" }, via: "browser", error: null, declaredWords: null });
      const at = Date.now();
      const kept = flow.staySignedIn
        ? flow.reason === "check"
          ? ({ kind: "checkPassed", at } as const)
          : ({ kind: "signedIn", at } as const)
        : ({ kind: "none" } as const);
      const site = sites.get(flow.site);
      sites.set(flow.site, {
        host: flow.site,
        fetch: null,
        siteData: "keep",
        ...site,
        session: kept,
        waitingArticle: null,
      });
      tellSignIn({ stage: { kind: "done" }, result: { kind: "ready" } });
    }, 400);
  };
  const signinParam = query.get("signin");
  if (signinParam === "waiting" || signinParam === "fetching") {
    if (!previewArticle) addArticle({ kind: "needsYou", reason: "signIn" });
    appSettings.browser.use = "fallback";
    openSignIn(fixtureArticle.summary.id);
    signIn = { ...signIn!, stage: { kind: signinParam } };
  }
  /** Feeds, like the app (core/feeds.rs): the subscriptions and entries the core made for the
   *  preview (`feeds.json`, times relative to now), one of them failing. `?feeds=empty` has
   *  none. Every entry opens as the preview's article (book-6), which stays off the library's
   *  lists (on the feed shelf) until it is saved. */
  const feedHandlers = new Set<() => void>();
  const feedsChanged = () => feedHandlers.forEach((h) => h());
  const loaded = Date.now();
  let feeds: FeedView[] =
    query.get("feeds") === "empty"
      ? []
      : (structuredClone(fixtureFeeds.feeds) as FeedView[]).map((f) => ({
          ...f,
          lastOkAt: f.lastOkAt === null ? null : loaded + f.lastOkAt,
          stage: f.stage.kind === "failing" ? { kind: "failing", sinceMs: loaded + f.stage.sinceMs } : f.stage,
        }));
  let entries: EntryView[] = (structuredClone(fixtureFeeds.entries) as EntryView[])
    .filter((e) => feeds.some((f) => f.id === e.feedId))
    .map((e) => ({ ...e, at: loaded + e.at }));
  /** An address that names a feed by its usual file name. */
  const isFeedAddress = (link: string) => /(feed|rss|atom)(\.xml|\.json)?\/?$/i.test(link);
  /** Items opened from a feed and not saved. */
  const feedShelf = new Set<string>();
  const inLibrary = (id: string | null) => id !== null && books.some((b) => b.id === id) && !feedShelf.has(id);
  const feedViews = () =>
    feeds.map((f) => ({ ...f, unread: entries.filter((e) => e.feedId === f.id && !e.read).length }));
  /** As the core does: what the entry's item needs, when its fetch fell short. */
  const needsOf = (id: string | null) => {
    const stage = id ? articles.get(id)?.stage : undefined;
    return stage?.kind === "needsYou" ? stage.reason : null;
  };
  const entryViews = () => entries.map((e) => ({ ...e, inLibrary: inLibrary(e.bookId), needsYou: needsOf(e.bookId) }));
  const getEntry = (id: number) => {
    const entry = entries.find((e) => e.id === id);
    if (!entry) throw refused("No such entry.");
    return entry;
  };
  /** The entry's item, made on the feed shelf the first time. */
  const entryItem = (entry: EntryView) => {
    if (entry.bookId && books.some((b) => b.id === entry.bookId)) return entry.bookId;
    let id = fixtureArticle.summary.id;
    if (!books.some((b) => b.id === id)) {
      // The feed's whole text is the copy; a summary-only entry has its page fetched.
      id = addArticle({ kind: entry.hasFullText ? "ready" : "queued" });
      if (!entry.hasFullText) fetchArticle(id, { kind: "ready" });
      feedShelf.add(id);
      setArticle(id, { inLibrary: false });
    }
    entry.bookId = id;
    return id;
  };
  /** Whole-book preparation, like the app (core/preparation.rs): each queued book has an explicit
   *  stage; the mock plans a book as soon as it is queued (books from `queueWaitingForTest` wait). */
  type PrepareEvent = "planned" | "pause" | "resume" | "promote" | "demote";
  /** The core's preparation stage machine, from the stages fixture. */
  const prepareStages = fixtureStages.prepare as Record<
    PrepareState,
    { next: Record<PrepareEvent, PrepareState>; actions: PrepareAction[]; someChaptersActions: PrepareAction[] }
  >;
  const nextStage = (stage: PrepareState, event: PrepareEvent): PrepareState => prepareStages[stage].next[event];
  /** A preparation view as the core makes it, with what it offers. */
  const prepareView = (
    state: PrepareState,
    done: number,
    total: number,
    position: number | null,
    chapters: number[] | null,
  ): PrepareView => ({
    state,
    done,
    total,
    position,
    chapters,
    actions: chapters ? prepareStages[state].someChaptersActions : prepareStages[state].actions,
  });
  const prepareQueue: { id: string; stage: PrepareState; chapters: number[] | null }[] = [];
  const prepareEntry = (id: string) => prepareQueue.find((e) => e.id === id);
  /** The first planned book is prepared; the others wait behind it. Then publishes the stages. */
  function settlePrepare() {
    let first = true;
    for (const entry of prepareQueue) {
      if (entry.stage === "queued" && first) entry.stage = nextStage(entry.stage, "promote");
      else if (entry.stage === "preparing" && !first) entry.stage = nextStage(entry.stage, "demote");
      if (entry.stage === "preparing") first = false;
    }
    prepareQueue.forEach((entry, position) => {
      const b = books.find((b) => b.id === entry.id);
      if (b?.prepare) b.prepare = prepareView(entry.stage, b.prepare.done, b.prepare.total, position, entry.chapters);
    });
    changed();
  }
  const unused = { files: 3, bytes: 12 * 1024 ** 2 };
  const getBook = (id: string) => {
    const b = books.find((b) => b.id === id);
    if (!b || b.status.kind !== "present") throw refused("Book is unavailable.");
    return b;
  };
  const getChapter = (id: string, chapter: number) => {
    getBook(id);
    const book = chapters[Number(id.split("-")[1])],
      result = book?.[chapter];
    if (!result) throw refused("no such chapter");
    const view = structuredClone(result);
    // The first book is illustrated: a picture after its first chapter's second block.
    if (id === "book-0" && chapter === 0 && view.blocks.length > 2)
      view.inserts = [
        {
          type: "picture",
          beforeBlock: 2,
          url: MOCK_PICTURE,
          alt: "A lighthouse at dusk",
          captionBlock: null,
          caption: null,
        },
      ];
    // A recording narrating the book lacks its last chapter, to show how that looks.
    if (recordings.get(id)?.narrates && recordings.get(id)?.stage === "ready") {
      view.notNarrated =
        book.length > 1 && chapter === book.length - 1
          ? view.blocks
              .flatMap((b) => b.segments)
              .filter((s) => s.speakable)
              .map((s) => s.id)
          : [];
      // As the core: no sentence of a book its recording narrates is said again.
      for (const segment of view.blocks.flatMap((b) => b.segments)) segment.canSayAgain = false;
    }
    return view;
  };
  const halt = () => {
    clearInterval(timer);
    timer = undefined;
  };
  /** An audiobook on its own plays on a timeline of its parts, as the core does
   *  (core/timeline.rs); its parts and their lengths come from the core's view of it. */
  /** Chapter `chapter`'s display text, as far as its sentences tell it: the words at their
   *  `starts`, a space between words of a block and line breaks between blocks. */
  const chapterText = (id: string, chapter: number) => {
    const text: string[] = [];
    for (const block of getChapter(id, chapter).blocks) {
      let first = true;
      for (const segment of block.segments) {
        segment.words.forEach((word, i) => {
          const at = segment.starts[i];
          while (text.length < at) text.push(first ? "\n" : " ");
          Array.from(word).forEach((c, k) => (text[at + k] = c));
          first = false;
        });
      }
    }
    return text;
  };
  const highlightsOf = (id: string): HighlightView[] =>
    structuredClone(
      highlights
        .filter((h) => h.bookId === id)
        .sort((a, b) => a.chapter - b.chapter || a.start - b.start || a.id - b.id)
        .map(({ bookId: _, ...h }) => h),
    );
  /** As the core: the text cut from the chapter, joined with the highlights it overlaps or touches. */
  const addHighlight = (id: string, chapter: number, start: number, end: number) => {
    const text = chapterText(id, chapter);
    if (start >= end || end > text.length) throw refused("that passage is not in the chapter");
    const joined = highlights.filter(
      (h) => h.bookId === id && h.chapter === chapter && h.start <= end && h.end >= start,
    );
    start = Math.min(start, ...joined.map((h) => h.start));
    end = Math.max(end, ...joined.map((h) => h.end));
    if (end - start > 2000) throw refused(`a highlight holds at most 2000 characters; this is ${end - start}`);
    for (const h of joined) highlights.splice(highlights.indexOf(h), 1);
    const added = nextHighlight++;
    highlights.push({
      id: added,
      bookId: id,
      chapter,
      chapterTitle: getChapter(id, chapter).title,
      start,
      end,
      text: text.slice(start, end).join(""),
      note: joined
        .map((h) => h.note)
        .filter(Boolean)
        .join("\n\n"),
      detached: false,
    });
    return { added, joined: joined.length > 0 };
  };
  const isAudiobook = (id: string) => books.find((b) => b.id === id)?.kind === "audiobook";
  const partsOf = (id: string) => {
    let start = 0;
    return (coreView(id)?.chapters ?? []).map((c) => {
      const part = { start, end: start + (c.durationMs ?? 0), title: c.title };
      start = part.end;
      return part;
    });
  };
  /** The part holding `at` (the last one at the very end). */
  const partAt = (parts: { start: number }[], at: number) => Math.max(0, parts.filter((p) => p.start <= at).length - 1);
  const timelineState = (id: string, at: number, status: PlayerState["status"]): PlayerState => {
    const parts = partsOf(id),
      total = parts.at(-1)?.end ?? 0,
      position = Math.min(total, Math.max(0, at)),
      index = partAt(parts, position),
      part = parts[index] ?? { start: 0, end: total };
    return {
      ...state,
      bookId: id,
      status,
      chapter: index,
      segmentId: null,
      wordIndex: null,
      chapterFraction: part.end > part.start ? (position - part.start) / (part.end - part.start) : 0,
      error: null,
      source: "recording",
      offText: false,
      chapterLeftMs: leftAt(part.end - position, state.speed),
      bookLeftMs: leftAt(total - position, state.speed),
      chapterMs: part.end - part.start,
      waitingForSync: false,
      positionMs: position,
      durationMs: total,
      partPositionMs: position - part.start,
    };
  };
  /** Saves an audiobook's place, finishing it at the end. */
  const keepTime = (id: string, at: number) => {
    const b = getBook(id),
      parts = partsOf(id),
      total = parts.at(-1)?.end ?? 0;
    const chapter = partAt(parts, at);
    b.progress = { chapter, segmentId: null, fraction: total ? at / total : 0, positionMs: at };
    if (at + 1000 >= total) finish(b);
    else b.readState = "inProgress";
    changed();
  };
  const TIMELINE_STEP = 250;
  const tickTimeline = () => {
    if (state.status !== "playing" || !state.bookId || state.positionMs === null) return;
    const id = state.bookId,
      at = state.positionMs + TIMELINE_STEP * state.speed;
    if (at >= (state.durationMs ?? 0)) {
      halt();
      emitPlayer(timelineState(id, state.durationMs ?? 0, "stopped"));
      ended(id);
    } else emitPlayer(timelineState(id, at, "playing"));
    keepTime(id, state.positionMs!);
  };
  const tick = () => {
    if (state.status !== "playing" || !state.bookId) return;
    if (isAudiobook(state.bookId)) return tickTimeline();
    const chapter = getChapter(state.bookId, state.chapter);
    const segments = chapter.blocks.flatMap((b) => b.segments).filter((s) => s.speakable);
    let index = segments.findIndex((s) => s.id === state.segmentId);
    let word = (state.wordIndex ?? -1) + 1;
    if (word >= segments[index].words.length) {
      index++;
      word = 0;
    }
    if (index >= segments.length) {
      const next = chapters[Number(state.bookId.split("-")[1])][state.chapter + 1];
      if (!next) {
        halt();
        emitPlayer({ ...state, status: "stopped", chapterFraction: 1 });
        ended(state.bookId);
        return;
      }
      emitPlayer({
        ...state,
        chapter: next.index,
        segmentId: next.blocks[0].segments[0].id,
        wordIndex: 0,
        chapterFraction: 0,
      });
    } else
      emitPlayer({
        ...state,
        segmentId: segments[index].id,
        wordIndex: word,
        chapterFraction: index / segments.length,
      });
    const book = getBook(state.bookId!);
    book.progress = {
      chapter: state.chapter,
      segmentId: state.segmentId,
      fraction: (state.chapter + state.chapterFraction) / chapters[Number(book.id.split("-")[1])].length,
      positionMs: null,
    };
    changed();
  };
  const run = () => {
    halt();
    timer = setInterval(tick, state.positionMs === null ? 320 / state.speed : TIMELINE_STEP);
  };
  const subscribe = async <T>(set: Set<(s: T) => void>, handler: (s: T) => void) => {
    set.add(handler);
    return () => {
      set.delete(handler);
    };
  };
  /** The window, which says when it goes fullscreen or back (as lib.rs `watch_fullscreen`). */
  let fullscreen = false;
  const fullscreenHandlers = new Set<(on: boolean) => void>();
  /** The window's question on closing while working; the preview has no window to close. */
  const closeAskedHandlers = new Set<() => void>();
  const backupHandlers = new Set<(progress: BackupProgressView) => void>();
  /** An import of links, as the core reports it (core/link_import.rs); the preview reads no
   *  file and fetches nothing, so it stays where the user put it. */
  const linkImportHandlers = new Set<(view: LinkImportView) => void>();
  const noImport: LinkImportView = {
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
  let linkImport: LinkImportView = { ...noImport };
  const setLinkImport = (view: LinkImportView) => {
    linkImport = view;
    linkImportHandlers.forEach((h) => h({ ...view }));
    return { ...view };
  };
  const finishedHandlers = new Set<(item: FinishedItem) => void>();
  /** Finishes item `b`, as the core does (core/reading.rs `after_finishing`): an article in the
   *  library just finished is archived when the setting says so, else offered for the archive. */
  const finish = (b: BookSummary) => {
    if (b.readState === "finished") return;
    b.readState = "finished";
    b.finishedAt = Date.now();
    if (!isArticle(b.id) || feedShelf.has(b.id) || b.archived) return;
    if (appSettings.archiveFinishedArticles) b.archived = true;
    else finishedHandlers.forEach((h) => h({ id: b.id, title: b.title }));
  };
  /** The queue, as the core keeps it (core/queue.rs): items to listen to next, in order. When the
   *  item playing ends, the first that can be listened to plays after a 2 s gap (anything the
   *  listener does meanwhile cancels that) and leaves the queue; Next at the last chapter hands
   *  over at once. */
  const queue: string[] = [];
  const queueHandlers = new Set<() => void>();
  const GAP_MS = 2_000;
  let handOver: ReturnType<typeof setTimeout> | undefined;
  /** The queued item that started last, while it plays: a feed's article is read once it ends. */
  let startedFromQueue: string | null = null;
  const queueChanged = () => {
    queueHandlers.forEach((h) => h());
    changed();
  };
  function whyNotOf(id: string): WhyNot | null {
    const book = books.find((b) => b.id === id);
    if (!book || book.status.kind !== "present") return "missing";
    const stage = articles.get(id)?.stage.kind;
    if (stage === "queued" || stage === "fetching") return "fetching";
    if (stage === "failed") return "notFetched";
    return null;
  }
  const titleOf = (id: string) => books.find((b) => b.id === id)?.title ?? "Untitled";
  const queueView = (): QueueView => ({
    items: queue.map((id) => ({ bookId: id, title: titleOf(id), whyNot: whyNotOf(id) })),
  });
  const putInQueue = (id: string, at: number) => {
    const from = queue.indexOf(id);
    if (from >= 0) queue.splice(from, 1);
    queue.splice(Math.min(at, queue.length), 0, id);
    queueChanged();
  };
  /** Item `id` started: it leaves the queue. */
  const started = (id: string) => {
    const at = queue.indexOf(id);
    if (at >= 0) {
      queue.splice(at, 1);
      startedFromQueue = id;
      queueChanged();
    } else if (startedFromQueue !== id) startedFromQueue = null;
  };
  /** The first queued item that can be listened to, other than `except`. */
  const nextInQueue = (except: string | null) => queue.find((id) => id !== except && whyNotOf(id) === null);
  /** While what plays has under 2 minutes left, the title of the item up next. */
  function upNextTitle(s: PlayerState): string | null {
    if (!s.bookId || s.bookLeftMs === null || s.bookLeftMs >= 120_000) return null;
    const next = nextInQueue(s.bookId);
    return next ? titleOf(next) : null;
  }
  const cancelHandOver = () => {
    clearTimeout(handOver);
    handOver = undefined;
  };
  const playNext = async (from: string | null) => {
    const next = nextInQueue(from);
    if (next) await api.play({ bookId: next, chapter: null, segmentId: null });
    return Boolean(next);
  };
  /** Item `id` played to its end. */
  const ended = (id: string | null) => {
    if (!id) return;
    if (startedFromQueue === id) {
      startedFromQueue = null;
      const entry = entries.find((e) => e.bookId === id);
      if (entry) {
        entry.read = true;
        getBook(id).archived = true;
        feedsChanged();
        changed();
      }
    }
    if (!queue.length) return;
    cancelHandOver();
    handOver = setTimeout(() => {
      handOver = undefined;
      void playNext(id);
    }, GAP_MS);
  };
  const api: AoideApi = {
    async logUi() {},
    async appInfo() {
      return { libraryFolder: "~/Documents/Aoide/Books" };
    },
    async takeOpenRequests() {
      return [];
    },
    async listBooks() {
      return structuredClone(
        books.filter((b) => !feedShelf.has(b.id)).map((b) => ({ ...b, queued: queue.includes(b.id) })),
      );
    },
    async queue() {
      return queueView();
    },
    async queueAdd({ bookId: id, place }) {
      getBook(id);
      putInQueue(id, place === "next" ? 0 : queue.length);
    },
    /** As the core: the entry's item is queued, and the entry stays unread. */
    async queueAddEntry({ entryId, place }) {
      const id = entryItem(getEntry(entryId));
      feedsChanged();
      putInQueue(id, place === "next" ? 0 : queue.length);
      return id;
    },
    async queueRemove({ bookId: id }) {
      const at = queue.indexOf(id);
      if (at < 0) return;
      queue.splice(at, 1);
      queueChanged();
    },
    async queueMove({ bookId: id, to }) {
      if (!queue.includes(id)) throw refused("That item is not in the queue.");
      putInQueue(id, to);
    },
    async queueClear() {
      queue.splice(0);
      queueChanged();
    },
    onQueueChanged: (h) => subscribe(queueHandlers, h),
    async setFullscreen({ on }) {
      if (on === fullscreen) return;
      fullscreen = on;
      fullscreenHandlers.forEach((h) => h(on));
    },
    onFullscreen: (h) => subscribe(fullscreenHandlers, h),
    onCloseAsked: (h) => subscribe(closeAskedHandlers, h),
    onBackupProgress: (h) => subscribe(backupHandlers, h),
    onLinkImport: (h) => subscribe(linkImportHandlers, h),
    async chooseLinksFile() {
      return "~/Downloads/pocket.csv";
    },
    async readLinks() {
      return setLinkImport({
        ...noImport,
        stage: "choosing",
        source: "Pocket",
        found: 12,
        unread: 9,
        already: 3,
        duplicates: 1,
      });
    },
    async startLinkImport({ choice }) {
      if (linkImport.stage !== "choosing") throw refused("Choose a file to import first.");
      const of =
        choice.which === "all"
          ? linkImport.found
          : choice.which === "unread"
            ? linkImport.unread
            : Math.min(choice.count, linkImport.found);
      return setLinkImport({ ...linkImport, stage: "fetching", of });
    },
    async linkImport() {
      return { ...linkImport };
    },
    async pauseLinkImport({ paused }) {
      const from = paused ? "fetching" : "paused";
      if (linkImport.stage !== from) return { ...linkImport };
      return setLinkImport({ ...linkImport, stage: paused ? "paused" : "fetching" });
    },
    async cancelLinkImport() {
      if (linkImport.stage === "choosing") return setLinkImport({ ...noImport });
      return setLinkImport({ ...linkImport, stage: "done" });
    },
    async retryLinkImportArchived() {
      if (linkImport.stage !== "done" || !linkImport.failed) throw refused("Nothing to try again.");
      return setLinkImport({ ...linkImport, stage: "fetching" });
    },
    async dismissLinkImport() {
      setLinkImport({ ...noImport });
    },
    async chooseBackupTarget() {
      return "~/Aoide backup.aoide-backup";
    },
    async chooseBackupFile() {
      return "~/Aoide backup.aoide-backup";
    },
    /** The preview has no library files: it says how far it got, as the core does. */
    async backUp({ path }) {
      const of = 12;
      for (let done = 0; done <= of; done += 4)
        backupHandlers.forEach((h) => h({ done, of, path, automatic: false, finished: false }));
      backupHandlers.forEach((h) => h({ done: of, of, path, automatic: false, finished: true }));
      return { path, bytes: 4_200_000 };
    },
    async inspectBackup({ path }) {
      return {
        createdAt: Date.UTC(2026, 10, 1, 9, 30),
        aoideVersion: "0.1.0",
        booksIncluded: false,
        bytes: 4_200_000,
        refused: /newer/.test(path)
          ? "This backup was made by a newer version of Aoide. Update Aoide to restore it."
          : null,
      };
    },
    /** Aoide restarts to restore; the preview has nothing to restart. */
    async restoreBackup() {},
    /** Keeping or quitting is the window's; a remembered answer becomes the setting, as in the
     *  core (core/presence.rs `choose_close`). */
    async chooseClose({ keep, remember }) {
      if (remember) appSettings.close = keep ? "keepWhileWorking" : "quit";
    },
    /** The series an item is in (core/facets.rs `series_shelf`). */
    async series({ bookId: id }) {
      const items = await api.listBooks();
      const name = items.find((b) => b.id === id)?.series?.name;
      if (name === undefined) return null;
      const place = (b: BookSummary) => b.series?.position ?? null;
      return {
        name,
        items: items
          .filter((b) => b.series && seriesKey(b.series.name) === seriesKey(name))
          .sort((a, b) => {
            const [pa, pb] = [place(a), place(b)];
            const byPlace = pa !== null && pb !== null ? pa - pb : Number(pa === null) - Number(pb === null);
            return (
              byPlace ||
              (a.title.toLowerCase() < b.title.toLowerCase()
                ? -1
                : a.title.toLowerCase() > b.title.toLowerCase()
                  ? 1
                  : 0)
            );
          })
          .map((b) => ({ bookId: b.id, position: place(b), state: b.readState })),
      };
    },
    async home() {
      return homeOverride ? structuredClone(homeOverride) : homeOf(await api.listBooks());
    },
    /** A saved email becomes the preview's email article; other files are not read. */
    async importFiles({ paths }) {
      return paths.map((path) =>
        /\.eml$/i.test(path)
          ? { path, bookId: addEmail(), error: null }
          : { path, bookId: null, error: "Browser preview cannot read local paths. Add books in the desktop app." },
      );
    },
    /** As the core does for the preview's audiobook (`audiobook.json`): added once, then the
     *  same item; a protected file is refused. */
    async addAudiobook({ paths }) {
      const path = paths[0] ?? "";
      if (/\.(aax|aa)$/i.test(path))
        return {
          path,
          bookId: null,
          error: `${path.split("/").pop()}: Protected by Audible DRM. Aoide plays only audio files that play in any player.`,
        };
      const { coverPath, ...summary } = structuredClone(fixtureAudiobook.summary) as unknown as Wire.BookSummary;
      if (!books.some((b) => b.id === summary.id)) {
        books.push({ ...summary, coverUrl: coverPath, addedAt: Date.now() });
        recordings.set(summary.id, structuredClone(fixtureAudiobook.recording) as unknown as RecordingView);
        changed();
      }
      return { path, bookId: summary.id, error: null };
    },
    async rescanLibrary() {
      changed();
    },
    async collections() {
      return shelves
        .map((s) => ({ ...s, books: books.filter((b) => b.collections.includes(s.id) && !b.archived).length }))
        .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
    },
    async createCollection({ name }) {
      const id = (shelves.at(-1)?.id ?? 0) + 1;
      shelves.push({ id, name: shelfName(name), createdAt: Date.now() });
      changed();
      return id;
    },
    async renameCollection({ id, name }) {
      const shelf = shelves.find((s) => s.id === id);
      if (!shelf) throw refused("no such collection");
      shelf.name = shelfName(name, id);
      changed();
    },
    async deleteCollection({ id }) {
      const at = shelves.findIndex((s) => s.id === id);
      if (at >= 0) shelves.splice(at, 1);
      books.forEach((b) => {
        b.collections = b.collections.filter((c) => c !== id);
      });
      changed();
      // As the core does: a feed whose entries went on it is left with none.
      if (feeds.some((f) => f.collectionId === id)) {
        feeds = feeds.map((f) => (f.collectionId === id ? { ...f, collectionId: null } : f));
        feedsChanged();
      }
    },
    /** Like the core (library/tags.rs): every tag on some item, by name, with its count. */
    async tags() {
      const counts = new Map<string, number>();
      for (const book of books) for (const tag of book.tags) counts.set(tag, (counts.get(tag) ?? 0) + 1);
      return [...counts]
        .map(([name, n]) => ({ name, books: n }))
        .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
    },
    /** Exactly these, trimmed, one of each ignoring case, by name. */
    async setBookTags({ bookId: id, tags }) {
      const kept: string[] = [];
      for (const tag of tags.map((t) => t.trim().replace(/\s+/g, " ")))
        if (tag && !kept.some((k) => k.toLowerCase() === tag.toLowerCase())) kept.push(tag);
      getBook(id).tags = kept.sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base" }));
      changed();
    },
    /** A model that tags every item "Maps" (core/tagging.rs keeps a model's tags beside the
     *  item's own). */
    async tagWithModel({ bookId: id }) {
      if (!appSettings.tagging.enabled) throw refused("Turn on tagging with a model in Settings › Library first");
      const book = getBook(id);
      if (!book.tags.includes("Maps")) book.tags = [...book.tags, "Maps"].sort((a, b) => a.localeCompare(b));
      changed();
      return ["Maps"];
    },
    async tagUntaggedWithModel() {
      if (!appSettings.tagging.enabled) throw refused("Turn on tagging with a model in Settings › Library first");
      const untagged = books.filter((b) => b.status.kind !== "error" && !b.tags.length);
      for (const book of untagged) book.tags = ["Maps"];
      changed();
      return { tagged: untagged.length, missed: 0 };
    },
    async setBookCollections({ bookId: id, ids }) {
      if (ids.some((c) => !shelves.some((s) => s.id === c))) throw refused("no such collection");
      getBook(id).collections = shelves
        .filter((s) => ids.includes(s.id))
        .sort((a, b) => a.name.localeCompare(b.name))
        .map((s) => s.id);
      changed();
    },
    async archive({ bookId: id, on }) {
      const book = getBook(id);
      if (book.archived === on) return;
      book.archived = on;
      changed();
    },
    async removeBook({ bookId: id }) {
      // As the core does: an item an entry still opens goes back to the feed shelf.
      if (entries.some((e) => e.bookId === id) && !feedShelf.has(id)) {
        feedShelf.add(id);
        setArticle(id, { inLibrary: false });
        changed();
        feedsChanged();
        return;
      }
      if (state.bookId === id) await api.stop();
      const index = books.findIndex((b) => b.id === id);
      if (index >= 0) books.splice(index, 1);
      articles.delete(id);
      fetched.delete(id);
      changed();
    },
    async showInFolder({ bookId: id }) {
      getBook(id);
    },
    async openBook({ bookId: id }): Promise<BookView> {
      const b = getBook(id);
      b.lastOpenedAt = Date.now();
      return api.bookOverview({ bookId: id });
    },
    async bookOverview({ bookId: id }): Promise<BookView> {
      const b = getBook(id),
        view = coreView(id);
      if (isArticle(id) && !fetched.has(id)) throw refused(articles.get(id)!.error ?? "Not fetched yet.");
      if (!view) throw refused("Book is unavailable.");
      const voice = settings.get(id)?.voiceId,
        offered = (await api.engineVoices()).map((v) => v.id);
      const voiceWarning =
        voice && !offered.includes(voice)
          ? `${voice.replace(".wav", "")} is not one of Mock engine's voices, so Emily reads this book. Choose a voice in the book's settings.`
          : null;
      // Chapters left out of the narration (core/leaving_out.rs): marked, and not counted
      // in the listening estimate.
      const leftOut = settingsOf(id).leftOut;
      const chapters = view.chapters.map((c) => ({ ...c, leftOut: leftOut.includes(c.index) }));
      return {
        ...view,
        chapters,
        chaptersLeftOut: chapters.filter((c) => c.leftOut).length,
        narratedWordCount: view.wordCount - chapters.filter((c) => c.leftOut).reduce((n, c) => n + c.wordCount, 0),
        title: b.title,
        progress: structuredClone(b.progress),
        voiceWarning,
        mode: b.mode,
        shape: b.shape,
      };
    },
    async setLeftOut({ bookId: id, chapters }) {
      const count = coreView(id)?.chapters.length ?? 0;
      const leftOut = [...new Set(chapters.filter((c) => c < count))].sort((a, b) => a - b);
      settings.set(id, { ...settingsOf(id), leftOut });
      return api.bookOverview({ bookId: id });
    },
    async setMode({ bookId: id, mode }) {
      const b = getBook(id);
      if (mode === "read" && isAudiobook(id)) throw refused(NO_TEXT);
      if (mode === "read" && state.bookId === id && state.status === "playing") await api.pause();
      b.mode = mode;
      b.shape = mode === "read" ? "read" : "readAlong";
      settings.set(id, { ...settingsOf(id), mode });
      changed();
      return api.bookOverview({ bookId: id });
    },
    async chapter({ bookId: id, chapter }) {
      if (isAudiobook(id)) throw refused(NO_TEXT);
      if (isArticle(id)) {
        if (!fetched.has(id)) throw refused(articles.get(id)!.error ?? "Not fetched yet.");
        if (chapter !== 0) throw refused("no such chapter");
        return articleChapter(id);
      }
      return getChapter(id, chapter);
    },
    async markFinished({ bookId: id }) {
      finish(getBook(id));
      changed();
    },
    async markUnread({ bookId: id }) {
      const b = getBook(id);
      b.readState = "unread";
      b.finishedAt = null;
      b.progress = null;
      changed();
    },
    async setReadingPlace({ bookId: id, chapter, segmentId }) {
      const b = getBook(id);
      if (state.bookId === id && state.status === "playing") return null;
      // As the core does: the place's share of the book's spoken segments.
      const all = chapters[Number(id.split("-")[1])].flatMap((c, n) =>
        c.blocks.flatMap((block) => block.segments.filter((s) => s.speakable).map((s) => `${n}/${s.id}`)),
      );
      const index = all.indexOf(`${chapter}/${segmentId}`);
      if (index < 0) throw refused("that place is not in the book");
      b.progress = { chapter, segmentId, fraction: all.length ? index / all.length : 0, positionMs: null };
      if (index === all.length - 1) finish(b);
      else if (b.readState === "unread") b.readState = "inProgress";
      changed();
      return structuredClone(b.progress);
    },
    async readingActivity({ bookId: id }) {
      getBook(id);
      readMs.set(id, (readMs.get(id) ?? 0) + 15_000);
    },
    async play({ bookId: id, chapter = null, segmentId = null }) {
      const book = getBook(id);
      cancelHandOver();
      started(id);
      if (isAudiobook(id)) {
        const parts = partsOf(id),
          total = parts.at(-1)?.end ?? 0;
        const saved = state.bookId === id ? state.positionMs : (book.progress?.positionMs ?? null);
        // As the core does: from the part asked for, else the saved place (from the start once
        // it was finished).
        const resume = saved !== null && saved + 1000 < total ? saved : 0;
        const at = chapter !== null ? (parts[chapter]?.start ?? 0) : resume;
        if (!isPlaying(id)) state = { ...state, ...speedOf(id) };
        emitPlayer(timelineState(id, at, "playing"));
        run();
        return;
      }
      wantEngine();
      // Playing a book by any route means listening to it.
      if (book.mode !== "listen") {
        book.mode = "listen";
        book.shape = "readAlong";
        changed();
      }
      const c = chapter ?? book.progress?.chapter ?? 0;
      const segments = getChapter(id, c)
        .blocks.flatMap((b) => b.segments)
        .filter((s) => s.speakable);
      const segment =
        segments.find((s) => s.id === (segmentId ?? (chapter === null ? book.progress?.segmentId : null))) ??
        segments[0];
      const source = recordings.get(id)?.narrates ? "recording" : "speech";
      emitPlayer({
        ...state,
        ...(isPlaying(id) ? {} : speedOf(id)),
        bookId: id,
        chapter: c,
        segmentId: segment.id,
        wordIndex: 0,
        status: "playing",
        chapterFraction: segments.indexOf(segment) / segments.length,
        error: null,
        source,
        offText: false,
        waitingForSync: false,
        positionMs: null,
        durationMs: null,
        partPositionMs: null,
      });
      if (sleptJustNow) {
        // `?sleep=again|again-chapter`: the sleep timer had just paused playback, so playing
        // starts it again, as the core does (core/controls.rs).
        const endOfChapter = sleptJustNow === "again-chapter";
        sleptJustNow = null;
        emitPlayer({
          ...state,
          sleep: { remainingMs: endOfChapter ? null : 15 * 60_000, endOfChapter, extended: true },
        });
      }
      emitGeneration({ ...(structuredClone(fixtureGenerationPlaying) as GenerationState), bookId: id });
      run();
    },
    async pause() {
      halt();
      cancelHandOver();
      emitPlayer({ ...state, status: "paused" });
    },
    async resume() {
      cancelHandOver();
      if (state.bookId) {
        emitPlayer({ ...state, status: "playing" });
        run();
      }
    },
    async stop() {
      halt();
      cancelHandOver();
      // With nothing playing, the usual speed applies.
      emitPlayer({ ...state, status: "stopped", wordIndex: null, speed: usualSpeed, speedScope: "usual" });
    },
    /** As the core does: the playing book's own speed when it has one, else the usual one. */
    async setSpeed({ speed: asked }) {
      const speed = keptSpeed(asked);
      if (state.speedScope === "book" && state.bookId)
        settings.set(state.bookId, { ...settingsOf(state.bookId), playbackSpeed: speed });
      else usualSpeed = speed;
      emitPlayer({ ...state, speed });
      if (state.status === "playing") run();
    },
    async setBookSpeed({ bookId: id, on }) {
      getBook(id);
      const playing = isPlaying(id);
      const own = on ? (playing ? state.speed : usualSpeed) : null;
      settings.set(id, { ...settingsOf(id), playbackSpeed: own });
      if (playing) emitPlayer({ ...state, ...speedOf(id) });
    },
    async setVolume({ volume }) {
      emitPlayer({ ...state, volume: Math.min(1, Math.max(0, volume)) });
    },
    async engineStatus() {
      return { ...engine };
    },
    async speechProblems() {
      return structuredClone(speechProblems);
    },
    async clearSpeechProblems() {
      speechProblems = [];
    },
    async retrySpeechServer() {
      showServer("starting");
    },
    async stopSpeechServer() {
      showServer("stopped");
    },
    async showSpeechServerLog() {},
    async engineOptions() {
      return { descriptors: structuredClone(descriptors), profile: structuredClone(profile) };
    },
    /** A narration line and a quoted one from the book, in two voices (core/voicing.rs). */
    async previewDialogue({ bookId: id }) {
      getBook(id);
      return silentWav();
    },
    async previewVoice() {
      return silentWav();
    },
    async previewPauses() {
      return silentWav();
    },
    async engineVoices() {
      return ["Emily.wav", "Adrian.wav", "Olivia.wav"].map((id) => ({ id, displayName: id.replace(".wav", "") }));
    },
    async saveEngineProfile({ profile: next }) {
      if (!descriptors.some((d) => d.kind === next.kind)) throw refused(`No adapter for engine kind \`${next.kind}\``);
      if (next.network === "loopback" && !/^https?:\/\/(127\.|localhost|\[::1\])/.test(next.baseUrl))
        throw refused("Only loopback addresses are allowed unless home-network access is on.");
      profile = JSON.parse(JSON.stringify(next));
      engine.profileName = next.name;
    },
    async bookSettings({ bookId: id }) {
      return structuredClone(settingsOf(id));
    },
    async saveBookSettings({ bookId: id, settings: next }) {
      getBook(id);
      // The chapters left out change only through setLeftOut, the book's speed only through
      // setBookSpeed and setSpeed.
      const { leftOut, playbackSpeed } = settingsOf(id);
      settings.set(id, { ...JSON.parse(JSON.stringify(next)), leftOut, playbackSpeed });
      const recording = recordings.get(id);
      if (recording) emitSync(recording); // Whether it narrates the book may have changed.
    },
    async prepareBook({ bookId: id }) {
      wantEngine();
      const b = getBook(id);
      const entry = prepareEntry(id) ?? prepareQueue[prepareQueue.push({ id, stage: "waiting", chapters: null }) - 1];
      entry.chapters = null;
      entry.stage = nextStage(entry.stage, "planned");
      if (entry.stage === "queued") b.prepare = prepareView("queued", 3, 120, 0, null);
      settlePrepare();
    },
    async setPreparePaused({ bookId: id, paused }) {
      const entry = prepareEntry(id);
      if (entry) entry.stage = nextStage(entry.stage, paused ? "pause" : "resume");
      if (entry?.stage === "waiting") entry.stage = nextStage(entry.stage, "planned"); // The mock plans at once.
      settlePrepare();
    },
    async prepareChapter({ bookId: id, chapter }) {
      const b = getBook(id);
      if (!chapters[Number(id.split("-")[1])]?.[chapter]) throw refused("no such chapter");
      const entry = prepareEntry(id);
      if (entry) {
        if (entry.chapters && !entry.chapters.includes(chapter))
          entry.chapters = [...entry.chapters, chapter].sort((x, y) => x - y);
        settlePrepare();
        return;
      }
      const added = prepareQueue[prepareQueue.push({ id, stage: "waiting", chapters: [chapter] }) - 1];
      added.stage = nextStage(added.stage, "planned");
      b.prepare = prepareView(added.stage, 0, 12, 0, [chapter]);
      settlePrepare();
    },
    async chapterAudio({ bookId: id }) {
      getBook(id);
      if (isAudiobook(id)) return [];
      const list = chapters[Number(id.split("-")[1])],
        queued = prepareEntry(id);
      return list.map((_, c) => {
        const total = 12;
        if (id === "book-0" && c === 0) return { done: total, total };
        return { done: queued && (queued.chapters ?? [c]).includes(c) ? 3 : 0, total };
      });
    },
    async cancelPrepare({ bookId: id }) {
      const b = books.find((b) => b.id === id);
      if (b) b.prepare = null;
      const at = prepareQueue.findIndex((e) => e.id === id);
      if (at >= 0) prepareQueue.splice(at, 1);
      settlePrepare();
    },
    async sayAgain({ bookId: id, segmentId }) {
      if (isAudiobook(id)) throw refused(NO_TEXT);
      const chapter = Number(segmentId.split(":")[0]);
      const view = isArticle(id)
        ? articleChapter(id)
        : chapters[Number(id.split("-")[1])]?.[chapter] && getChapter(id, chapter);
      const segment = view?.blocks.flatMap((b) => b.segments).find((s) => s.id === segmentId);
      if (!segment) throw refused("no such sentence");
      if (!segment.canSayAgain) throw refused("this sentence is not said by the speech engine");
      if (state.bookId !== id) return api.play({ bookId: id, chapter, segmentId });
      // As the core does: the playhead goes to the sentence, playing or paused as it was, and
      // waits there while the sentence is made again.
      const playing = state.status !== "paused";
      halt();
      clearTimeout(remaking);
      emitPlayer({ ...state, chapter, segmentId, wordIndex: 0, status: playing ? "buffering" : "paused" });
      const generation = { ...(structuredClone(fixtureGenerationPlaying) as GenerationState), bookId: id };
      emitGeneration({ ...generation, generatingSegmentId: segmentId });
      remaking = setTimeout(() => {
        emitGeneration(generation);
        if (!playing || state.bookId !== id) return;
        emitPlayer({ ...state, status: "playing" });
        run();
      }, REMAKE_MS);
    },
    async regenerateBook({ bookId: id }) {
      getBook(id).audioBytes = 0;
      changed();
    },
    async storageReport() {
      const audio = books.filter((b) => b.audioBytes > 0).sort((a, b) => b.audioBytes - a.audioBytes);
      return {
        audioBytes: audio.reduce((n, b) => n + b.audioBytes, 0),
        coversBytes: 1_400_000,
        articlesBytes: 240_000,
        booksBytes: 3_600_000,
        databaseBytes: 240_000,
        syncBytes: recordings.size * 1_900_000,
        searchBytes: appSettings.searchText ? 240_000 : 0,
        picturesBytes: 3_400_000,
        modelsBytes: recordings.size ? 147_964_211 : 0,
        browserBytes: browserDownloaded ? 512_000_000 : 0,
        unusedFiles: unused.files,
        unusedBytes: unused.bytes,
        books: audio.map((b) => ({
          bookId: b.id,
          bytes: b.audioBytes,
          segments: Math.ceil(b.audioBytes / 40_000),
          newestAt: 1_000,
          inUse: state.bookId === b.id && state.status !== "stopped",
        })),
      };
    },
    async cleanUnusedFiles() {
      const freed = { ...unused };
      unused.files = 0;
      unused.bytes = 0;
      return freed;
    },
    async setSleepTimer({ minutes = null, endOfChapter }) {
      emitPlayer({
        ...state,
        sleep:
          minutes === null && !endOfChapter
            ? null
            : { remainingMs: minutes === null ? null : minutes * 60_000, endOfChapter, extended: false },
      });
    },
    /** The library's text searched, like the core (core/indexing.rs, FTS5): every word must be
     *  in the block (the last as a word's start), accents and case folded; each item's best
     *  three blocks (most matches), items by their best. */
    async searchLibrary({ query }) {
      if (!appSettings.searchText) return [];
      const fold = (w: string) =>
        w
          .normalize("NFD")
          .replace(/\p{M}/gu, "")
          .toLowerCase()
          .replace(/[^\p{L}\p{N}]/gu, "");
      const terms = query.split(/\s+/).map(fold).filter(Boolean);
      if (!terms.length) return [];
      const matches = (word: string, t: number) =>
        t === terms.length - 1 ? word.startsWith(terms[t]) : word === terms[t];
      const found: LibraryHits[] = [];
      chapters.forEach((book, b) => {
        const id = `book-${b}`;
        if (!books.some((x) => x.id === id)) return;
        const hits: { hit: PassageHitView; score: number }[] = [];
        for (const chapter of book)
          for (const block of chapter.blocks) {
            const words = block.segments.flatMap((s) =>
              s.words.map((w, i) => ({ text: w, folded: fold(w), segment: s.id, start: s.starts[i] })),
            );
            if (!terms.every((_, t) => words.some((w) => matches(w.folded, t)))) continue;
            const marked = words.map((w) => terms.some((_, t) => matches(w.folded, t)));
            const first = words[marked.indexOf(true)];
            let snippet = "",
              at = 0;
            const marks: [number, number][] = [];
            words.forEach((w, i) => {
              if (i) {
                snippet += " ";
                at += 1;
              }
              const length = Array.from(w.text).length;
              // The word's letters, without the punctuation around them.
              const lead = Array.from(w.text).findIndex((c) => /[\p{L}\p{N}]/u.test(c));
              const core = Array.from(w.text.replace(/[^\p{L}\p{N}]+$/u, "")).length;
              if (marked[i]) marks.push([at + lead, at + core]);
              snippet += w.text;
              at += length;
            });
            const lead = Array.from(first.text).findIndex((c) => /[\p{L}\p{N}]/u.test(c));
            const core = Array.from(first.text.replace(/[^\p{L}\p{N}]+$/u, "")).length;
            hits.push({
              score: marks.length,
              hit: {
                chapter: chapter.index,
                segmentId: first.segment,
                snippet,
                marks,
                found: [first.start + lead, first.start + core],
              },
            });
          }
        if (!hits.length) return;
        hits.sort((x, y) => y.score - x.score);
        found.push({ bookId: id, hits: hits.slice(0, 3).map((h) => h.hit), more: Math.max(0, hits.length - 3) });
      });
      return found.sort((a, b) => b.hits[0].marks.length - a.hits[0].marks.length).slice(0, 30);
    },
    async searchBook({ bookId: id, query }) {
      getBook(id);
      const q = query.trim().toLowerCase();
      if (q.length < 2) return [];
      const hits: SearchHit[] = [];
      chapters[Number(id.split("-")[1])].forEach((chapter) =>
        chapter.blocks.forEach((block) =>
          block.segments.forEach((segment) => {
            const text = segment.words.join(" ");
            const at = text.toLowerCase().indexOf(q);
            if (at >= 0)
              hits.push({
                chapter: chapter.index,
                chapterTitle: chapter.title,
                segmentId: segment.id,
                snippet: text,
                matchStart: at,
                matchEnd: at + q.length,
              });
          }),
        ),
      );
      return hits;
    },
    async skipChapter({ delta }) {
      cancelHandOver();
      // As the core: forward from the last chapter, the first queued item plays at once.
      const last = state.bookId
        ? (state.positionMs !== null
            ? partsOf(state.bookId).length
            : (chapters[Number(state.bookId.split("-")[1])]?.length ?? 1)) - 1
        : 0;
      if (delta > 0 && state.bookId && state.chapter >= last && (await playNext(state.bookId))) return;
      if (state.bookId && state.positionMs !== null) {
        const parts = partsOf(state.bookId),
          part = Math.min(parts.length - 1, Math.max(0, state.chapter + delta));
        await api.seek({ bookId: state.bookId, positionMs: parts[part].start });
        await api.resume();
      } else if (state.bookId) {
        const list = chapters[Number(state.bookId.split("-")[1])];
        const c = Math.min(list.length - 1, Math.max(0, state.chapter + delta));
        await api.play({ bookId: state.bookId, chapter: c, segmentId: list[c].blocks[0].segments[0].id });
      }
    },
    async seek({ bookId: id, positionMs }) {
      cancelHandOver();
      const total = partsOf(id).at(-1)?.end ?? 0,
        at = Math.min(total, Math.max(0, positionMs));
      if (state.bookId === id && state.positionMs !== null) emitPlayer(timelineState(id, at, state.status));
      keepTime(id, at);
    },
    async skip({ bookId: id, deltaMs }) {
      const at = state.bookId === id ? state.positionMs : (getBook(id).progress?.positionMs ?? 0);
      await api.seek({ bookId: id, positionMs: (at ?? 0) + deltaMs });
    },
    async bookmarks({ bookId: id }) {
      return structuredClone(
        bookmarks
          .filter((b) => b.bookId === id)
          .sort(
            (a, b) =>
              a.chapter - b.chapter ||
              a.charOffset - b.charOffset ||
              (a.positionMs ?? 0) - (b.positionMs ?? 0) ||
              a.createdAt - b.createdAt,
          ),
      );
    },
    async addBookmark({ bookId: id, note, segmentId: chosen = null }) {
      const book = getBook(id);
      const playing = state.bookId === id;
      if (isAudiobook(id)) {
        if (chosen) throw refused("an audiobook without text has no sentences to bookmark");
        const at = (playing ? state.positionMs : book.progress?.positionMs) ?? 0,
          parts = partsOf(id),
          part = partAt(parts, at);
        const mark: Bookmark = {
          id: nextBookmark++,
          bookId: id,
          chapter: part,
          charOffset: 0,
          segmentId: null,
          excerpt: parts[part]?.title ?? "",
          note,
          createdAt: Math.floor(Date.now() / 1000),
          positionMs: at,
        };
        bookmarks.push(mark);
        return structuredClone(mark);
      }
      // As the core does: the sentence chosen, else the playhead, else the saved place.
      const chapter = chosen ? Number(chosen.split(":")[0]) : playing ? state.chapter : (book.progress?.chapter ?? 0);
      const segmentId = chosen ?? (playing ? state.segmentId : book.progress?.segmentId) ?? null;
      const inBook = Boolean(chapters[Number(id.split("-")[1])]?.[chapter]);
      const segment = inBook
        ? getChapter(id, chapter)
            .blocks.flatMap((b) => b.segments)
            .find((s) => s.id === segmentId)
        : undefined;
      if (chosen && !segment) throw refused(`no sentence ${chosen} in this book`);
      const mark: Bookmark = {
        id: nextBookmark++,
        bookId: id,
        chapter,
        charOffset: Number(segmentId?.split(":")[1] ?? 0) * 100,
        segmentId,
        excerpt: segment ? segment.words.slice(0, 12).join(" ") : "",
        note,
        createdAt: Math.floor(Date.now() / 1000),
        positionMs: null,
      };
      bookmarks.push(mark);
      return structuredClone(mark);
    },
    async updateBookmarkNote({ id, note }) {
      const mark = bookmarks.find((b) => b.id === id);
      if (!mark) throw refused("Bookmark not found.");
      mark.note = note;
    },
    async deleteBookmark({ id }) {
      const index = bookmarks.findIndex((b) => b.id === id);
      if (index >= 0) bookmarks.splice(index, 1);
    },
    async highlights({ bookId: id }) {
      return highlightsOf(id);
    },
    async addHighlight({ bookId: id, chapter, start, end }) {
      if (isAudiobook(id)) throw refused("an audiobook has no text");
      addHighlight(id, chapter, start, end);
      return highlightsOf(id);
    },
    /** As the core: the sentence at the playhead (the mock's playhead is always well into it). */
    async highlightSpoken({ bookId: id }) {
      if (isAudiobook(id)) throw refused("an audiobook has no text");
      if (state.bookId !== id || !state.segmentId) throw refused("this book is not being listened to");
      const segment = getChapter(id, state.chapter ?? 0)
        .blocks.flatMap((b) => b.segments)
        .find((s) => s.id === state.segmentId);
      if (!segment?.starts.length) throw refused("there is no sentence here to highlight");
      const last = segment.starts.length - 1;
      const { added, joined } = addHighlight(
        id,
        state.chapter ?? 0,
        segment.starts[0],
        segment.starts[last] + Array.from(segment.words[last]).length,
      );
      return { id: added, canUndo: !joined, highlights: highlightsOf(id) };
    },
    async updateHighlightNote({ id, note }) {
      const found = highlights.find((h) => h.id === id);
      if (!found) throw refused("that highlight is not there any more");
      found.note = note.trim();
      return highlightsOf(found.bookId);
    },
    async deleteHighlight({ id }) {
      const index = highlights.findIndex((h) => h.id === id);
      if (index < 0) throw refused("that highlight is not there any more");
      const [gone] = highlights.splice(index, 1);
      return highlightsOf(gone.bookId);
    },
    async notesMarkdown({ bookId: id }) {
      const book = getBook(id);
      const lines = [`# ${book.title ?? "Untitled"}`, `*${book.authors.join(", ")}* · exported from Aoide`];
      let chapter = -1;
      for (const h of highlightsOf(id)) {
        if (h.chapter !== chapter) lines.push("", `## ${h.chapterTitle || `Chapter ${h.chapter + 1}`}`);
        chapter = h.chapter;
        lines.push("", ...h.text.split("\n").map((l) => (l ? `> ${l}` : ">")));
        if (h.note) lines.push("", h.note);
      }
      return lines.join("\n") + "\n";
    },
    async chooseNotesTarget({ fileName }) {
      return `~/${fileName}`;
    },
    async exportNotes({ bookId: id }) {
      getBook(id);
    },
    async chooseNotesFolder() {
      return "~/Notes";
    },
    async exportAllNotes() {
      return new Set([...highlights.map((h) => h.bookId), ...bookmarks.map((b) => b.bookId)]).size;
    },
    async pronunciations({ bookId } = {}) {
      return structuredClone(
        pronunciations
          .filter((p) => p.bookId === (bookId ?? null))
          .sort((a, b) => a.createdAt - b.createdAt || a.id - b.id),
      );
    },
    async pronunciationAffected({ bookId: id, written, spoken, matchCase }) {
      getBook(id);
      const [word, said] = [listWords(written), listWords(spoken)];
      if (!word) throw refused("say which word is written");
      if (!said) throw refused("say how the word is said");
      const saved = pronunciations.find((p) => p.bookId === id && p.written === word);
      if (saved && saved.spoken === said && saved.matchCase === matchCase) return { sentences: 0, cached: 0 };
      const found = sentencesSaying(id, word, matchCase);
      // As chapterAudio: the first book's first chapter is made.
      return { sentences: found.length, cached: id === "book-0" ? found.filter((f) => f.chapter === 0).length : 0 };
    },
    async savePronunciation({ bookId, written, spoken, matchCase }) {
      const [word, said] = [listWords(written), listWords(spoken)];
      if (!word) throw refused("say which word is written");
      if (!said) throw refused("say how the word is said");
      const book = bookId ?? null;
      let entry = pronunciations.find((p) => p.bookId === book && p.written === word);
      if (entry) Object.assign(entry, { spoken: said, matchCase });
      else {
        entry = {
          id: nextPronunciation++,
          bookId: book,
          written: word,
          spoken: said,
          matchCase,
          createdAt: Date.now(),
        };
        pronunciations.push(entry);
      }
      remake(book, entry);
      return structuredClone(entry);
    },
    async deletePronunciation({ id }) {
      const at = pronunciations.findIndex((p) => p.id === id);
      if (at < 0) throw refused("no such pronunciation");
      const [entry] = pronunciations.splice(at, 1);
      remake(entry.bookId, entry);
    },
    async previewPronunciation({ spoken }) {
      if (!listWords(spoken)) throw refused("say how the word is said");
      return silentWav();
    },
    async bookListening({ bookId: id }) {
      getBook(id);
      return id === "book-0"
        ? [
            { day: "2026-11-02", ms: 1_500_000 },
            { day: "2026-11-08", ms: 2_400_000 },
            { day: "2026-11-10", ms: 1_500_000 },
          ]
        : [];
    },
    async activityStats() {
      const ids = [...new Set([...listened.keys(), ...readMs.keys()])];
      const byBook = ids
        .map((bookId) => ({ bookId, readingMs: readMs.get(bookId) ?? 0, listeningMs: listened.get(bookId) ?? 0 }))
        .sort((a, b) => b.readingMs + b.listeningMs - (a.readingMs + a.listeningMs));
      return {
        readingMs: byBook.reduce((t, b) => t + b.readingMs, 0),
        listeningMs: byBook.reduce((t, b) => t + b.listeningMs, 0),
        byDay: [
          { day: "2026-11-08", readingMs: 600_000, listeningMs: 2_400_000 },
          { day: "2026-11-09", readingMs: 1_200_000, listeningMs: 3_000_000 },
          { day: "2026-11-10", readingMs: 0, listeningMs: 1_200_000 },
        ],
        byBook,
      };
    },
    async bookDetails({ bookId: id }) {
      getBook(id);
      const stored = structuredClone(
        details.get(id) ?? {
          description: "An original story used in the browser preview.",
          publisher: null,
          isbn: null,
          published: null,
          subjects: [],
          openLibraryKey: null,
        },
      );
      return { ...stored, published: edits.get(id)?.published ?? stored.published };
    },
    async editDetails({ bookId: id, patch }) {
      const book = books.find((b) => b.id === id);
      if (!book) throw refused("No such item.");
      if (!fileDetails.has(id))
        fileDetails.set(id, {
          title: book.title,
          authors: [...book.authors],
          coverUrl: book.coverUrl,
          series: structuredClone(book.series),
        });
      const file = fileDetails.get(id)!;
      const edited = new Set(book.edited);
      const mark = (field: DetailField, on: boolean) => (on ? edited.add(field) : edited.delete(field));
      const blank = (text: string | null | undefined) => text?.trim() || null;
      if (patch.title !== undefined) {
        const title = blank(patch.title);
        book.title = title ?? file.title;
        mark("title", title !== null);
      }
      if (patch.authors !== undefined) {
        const authors = patch.authors?.map((a) => a.trim()).filter(Boolean) ?? null;
        book.authors = authors ?? [...file.authors];
        mark("authors", authors !== null);
      }
      if (patch.published !== undefined) {
        const published = blank(patch.published);
        edits.set(id, { ...edits.get(id), published: published ?? undefined });
        mark("published", published !== null);
      }
      if (patch.series !== undefined || patch.seriesPosition !== undefined) {
        // As the core: the name and the number are each kept over the file's; an empty name
        // is in no series.
        const kept = { ...edits.get(id) };
        if (patch.series !== undefined) kept.series = patch.series === null ? undefined : patch.series.trim();
        if (patch.seriesPosition !== undefined) kept.seriesPosition = patch.seriesPosition ?? undefined;
        edits.set(id, kept);
        const name = kept.series ?? file.series?.name ?? "";
        book.series = name ? { name, position: kept.seriesPosition ?? file.series?.position ?? null } : null;
        mark("series", kept.series !== undefined || kept.seriesPosition !== undefined);
      }
      if (patch.cover !== undefined) {
        book.coverUrl = patch.cover ? ownCover(patch.cover) : file.coverUrl;
        mark("cover", Boolean(patch.cover));
      }
      book.edited = (["title", "authors", "cover", "published", "series"] as const).filter((f) => edited.has(f));
      changed();
    },
    async lookupMetadata({ bookId: id, refresh: _refresh }): Promise<MetadataMatch[]> {
      const book = getBook(id);
      if (!appSettings.metadataLookup) throw refused("Online metadata lookup is turned off in Settings.");
      return [
        {
          key: "/works/OL0001W",
          title: book.title,
          authors: book.authors,
          published: "2019",
          publisher: "Preview Press",
          isbn: null,
          subjects: ["Fiction"],
          coverUrl: null,
        },
      ];
    },
    async applyMetadata({ bookId: id, candidate: match }) {
      getBook(id);
      const next: BookDetails = {
        description: details.get(id)?.description ?? null,
        publisher: match.publisher,
        isbn: match.isbn,
        published: match.published,
        subjects: match.subjects,
        openLibraryKey: match.key,
      };
      details.set(id, next);
      changed();
      return structuredClone(next);
    },
    async appSettings() {
      return { ...appSettings };
    },
    async updateAppSettings({ patch }) {
      const inbox = appSettings.inbox;
      appSettings = JSON.parse(JSON.stringify({ ...appSettings, ...patch }));
      if (appSettings.inbox !== inbox) inboxHandlers.forEach((h) => h(inboxView()));
      return JSON.parse(JSON.stringify(appSettings));
    },
    async inboxStatus() {
      return inboxView();
    },
    onInboxState: (h) => subscribe(inboxHandlers, h),
    async preferences() {
      return structuredClone(preferences);
    },
    async saveAppearance({ appearance: next }) {
      preferences = { ...preferences, appearance: { ...next } };
    },
    onConfigChanged: (h) => subscribe(configHandlers, h),
    async exportBook({ bookId: id }) {
      getBook(id);
      const path = `~/Documents/Aoide/Exports/${books.find((b) => b.id === id)!.title}.m4b`;
      exportHandlers.forEach((h) => h({ bookId: id, stage: "done", done: 1, total: 1, path, error: null }));
      return path;
    },
    async exportReadAlong({ bookId: id }) {
      getBook(id);
      const path = `~/Documents/Aoide/Exports/${books.find((b) => b.id === id)!.title} (read-along).epub`;
      exportHandlers.forEach((h) => h({ bookId: id, stage: "done", done: 1, total: 1, path, error: null }));
      return path;
    },
    async scanForBooks() {
      scanHandlers.forEach((h) =>
        h({ stage: "walking", dirs: 1200, files: 48000, found: 3, read: 0, toRead: 0, error: null }),
      );
      scanned = true;
      scanHandlers.forEach((h) => h({ stage: "done", dirs: 0, files: 0, found: 4, read: 2, toRead: 2, error: null }));
      return listed();
    },
    async cancelScan() {},
    async discoveredBooks() {
      return scanned ? listed() : [];
    },
    async fetchDiscoveredDetails({ ids }) {
      for (const id of ids) {
        const c = found.find((f) => f.id === id);
        // Open Library's answer, as the core kept it (`found.json`).
        const details = (fixtureFound.details as Record<string, Candidate["suggestion"]>)[id];
        if (c && c.status === "new" && details) c.suggestion = structuredClone(details);
        detailHandlers.forEach((h) => h(id));
      }
    },
    async importDiscovered({ choices }) {
      const results = [];
      for (const [done, choice] of choices.entries()) {
        importHandlers.forEach((h) =>
          h({ done, total: choices.length, current: found.find((f) => f.id === choice.id)?.title ?? null }),
        );
        const c = found.find((f) => f.id === choice.id);
        if (!c || c.status === "unreadable") {
          results.push({ path: c?.paths[0] ?? "", bookId: null, error: "not a readable EPUB" });
          continue;
        }
        c.status = "inLibrary";
        books.push({
          ...coreBooks()[3],
          id: c.id,
          title: c.title,
          authors: c.authors,
          language: c.language,
          addedAt: 900,
        });
        changed();
        results.push({ path: c.paths[0], bookId: c.id, error: null });
        if (choice.pairAudiobook && c.audiobook.length) await api.attachRecording({ bookId: c.id, paths: c.audiobook });
      }
      return results;
    },
    onScanProgress: (h) => subscribe(scanHandlers, h),
    async scanForAudiobooks() {
      audiobookScanHandlers.forEach((h) =>
        h({ stage: "walking", dirs: 800, files: 21000, found: 40, read: 0, toRead: 0, error: null }),
      );
      const verify = appSettings.verifyAudiobooks;
      const under = new Map(audiobookMatches.map((m) => [m.bookId, m]));
      audiobookMatches = Object.entries(kept).flatMap(([bookId, candidates]): AudiobookMatch[] => {
        const before = under.get(bookId);
        if (before && before.stage.kind !== "found") return [before];
        if (recordings.has(bookId)) return [];
        return [{ bookId, candidates, stage: verify ? { kind: "verifying", candidate: 0 } : { kind: "found" } }];
      });
      for (const m of audiobookMatches)
        if (m.stage.kind === "verifying" && !recordings.has(m.bookId))
          await api.attachRecording({ bookId: m.bookId, paths: m.candidates[0].paths });
      unmatched = structuredClone(fixtureFound.unmatched);
      audiobookScanHandlers.forEach((h) =>
        h({ stage: "done", dirs: 0, files: 0, found: 0, read: 0, toRead: 0, error: null }),
      );
      emitMatches();
      return structuredClone(audiobookMatches);
    },
    async audiobookMatches() {
      return structuredClone(audiobookMatches);
    },
    async unmatchedAudiobooks() {
      const added = books.some((b) => b.id === fixtureAudiobook.summary.id);
      return structuredClone(unmatched.filter((u) => !(added && u.paths[0] === WINTER_RADIO)));
    },
    async cancelAudiobookScan() {},
    async lookForAudiobooks({ ids }) {
      if (!appSettings.audiobooks.onAdd) return 0;
      await api.scanForAudiobooks();
      return audiobookMatches.filter((m) => ids.includes(m.bookId) && m.stage.kind === "found").length;
    },
    async pairAudiobooks({ choices }) {
      const outcomes = [];
      for (const { bookId, candidate } of choices) {
        const match = audiobookMatches.find((m) => m.bookId === bookId);
        try {
          if (!match?.candidates[candidate]) throw refused("no such audiobook match");
          await api.attachRecording({ bookId, paths: match.candidates[candidate].paths });
          match.stage = { kind: "paired", candidate };
          outcomes.push({ bookId, error: null });
        } catch (e) {
          outcomes.push({ bookId, error: e instanceof Error ? e.message : String(e) });
        }
      }
      emitMatches();
      return outcomes;
    },
    onAudiobookScan: (h) => subscribe(audiobookScanHandlers, h),
    onAudiobookMatches: (h) => subscribe(matchHandlers, h),
    onDiscoveryDetails: (h) => subscribe(detailHandlers, h),
    onDiscoveryImport: (h) => subscribe(importHandlers, h),
    onExportProgress: (h) => subscribe(exportHandlers, h),
    onPlayerState: (h) => subscribe(playerHandlers, h),
    onEngineStatus: (h) => subscribe(engineHandlers, h),
    onGenerationState: (h) => subscribe(generationHandlers, h),
    async onLibraryChanged(h) {
      libraryHandlers.add(h);
      return () => {
        libraryHandlers.delete(h);
      };
    },
    async inspectRecording({ paths }): Promise<InspectionView> {
      const audio = paths.filter((p) => /\.(m4b|m4a|mp3|flac|ogg|opus|wav)$/i.test(p) || !/\.[a-z0-9]+$/i.test(p));
      const protectedFiles = paths.filter((p) => /\.(aax|aa)$/i.test(p));
      const name = (p: string) => p.split("/").pop() ?? p;
      return {
        files: audio.map((path, i) => ({
          path,
          name: name(path),
          durationMs: hour + i * 600_000,
          codec: path.endsWith(".mp3") ? "mp3" : "aac",
        })),
        problems: protectedFiles.map((path) => ({
          path,
          name: name(path),
          problem: "Protected by Audible DRM. Aoide plays only audio files that play in any player.",
          drm: true,
        })),
        totalMs: audio.length * hour,
        chapters: audio.length * 3,
        title: audio.some((p) => p.toLowerCase().includes("lantern")) ? "The Lantern Keeper" : null,
        author: audio.some((p) => p.toLowerCase().includes("lantern")) ? "Mira Vale" : null,
        suggestedBook: audio.some((p) => p.toLowerCase().includes("lantern")) ? "book-0" : null,
      };
    },
    /** As the core does: the book takes the audiobook's recording (which starts syncing),
     *  bookmarks and collections, and the audiobook item goes. */
    /** As the core does for the preview's article (`article.json`): saved once, then fetched;
     *  a link saved again is the same item, fetched again unless it is ready. */
    async saveArticle({ url }) {
      parseLink(url);
      const id = fixtureArticle.summary.id;
      const known = articles.get(id);
      if (!known) {
        addArticle({ kind: "queued" }, url.trim());
        fetchArticle(id, { kind: "ready" });
      } else if (known.stage.kind !== "ready") fetchArticle(id, { kind: "ready" });
      // As the core does: an item opened from a feed is kept once its link is added.
      if (feedShelf.delete(id)) {
        setArticle(id, { inLibrary: true });
        changed();
        feedsChanged();
      }
      return id;
    },
    async refetchArticle({ bookId: id, source }) {
      const view = articles.get(id);
      if (!view) throw refused("Not an article.");
      onTheWeb(id);
      if (source === "browser" && appSettings.browser.use === "off")
        throw refused("The browser is off. Turn it on in Settings › Sites.");
      if (view.stage.kind === "queued" || view.stage.kind === "fetching") return;
      fetchArticle(id, { kind: "ready" }, source);
    },
    async article({ bookId: id }) {
      const view = articles.get(id);
      if (!view) throw refused("Not an article.");
      return structuredClone(view);
    },
    async feeds() {
      return feedViews();
    },
    /** As the core does: a known address is the feed already subscribed; any other address
     *  ending in a feed's usual name subscribes to a new, empty feed; anything else is no feed. */
    async subscribe({ url }): Promise<SubscribeResult> {
      const link = parseLink(url).href;
      const known = feedViews().find((f) => f.url === link);
      if (known) return { kind: "subscribed", value: known };
      if (!isFeedAddress(link)) {
        // A site's front page names two feeds; any other page none.
        if (new URL(link).pathname !== "/") return { kind: "notAFeed" };
        return {
          kind: "choose",
          value: [
            { url: new URL("/feed.xml", link).href, title: "Articles", subscribed: false },
            { url: new URL("/comments/feed.xml", link).href, title: "Comments", subscribed: false },
          ].map((f) => ({ ...f, subscribed: feeds.some((known) => known.url === f.url) })),
        };
      }
      const feed: FeedView = {
        id: Math.max(0, ...feeds.map((f) => f.id)) + 1,
        // Its own title, for a feed on the preview's web (`web.json`).
        title: fixtureWeb.feeds.find((f) => f.url === link)?.title ?? new URL(link).hostname,
        url: link,
        siteUrl: new URL("/", link).href,
        unread: 0,
        stage: { kind: "idle" },
        error: null,
        lastOkAt: Date.now(),
        collectionId: null,
        folder: null,
      };
      feeds = [...feeds, feed].sort((a, b) => (a.title ?? a.url).localeCompare(b.title ?? b.url));
      // Articles that name it now say it is subscribed.
      for (const [id, view] of articles)
        if (view.feeds.some((f) => f.url === link))
          setArticle(id, { feeds: view.feeds.map((f) => (f.url === link ? { ...f, subscribed: true } : f)) });
      feedsChanged();
      return { kind: "subscribed", value: feed };
    },
    /** As the core does: the feed's entries go, and the items only they opened. */
    async unsubscribe({ feedId }) {
      feeds = feeds.filter((f) => f.id !== feedId);
      entries = entries.filter((e) => e.feedId !== feedId);
      for (const id of [...feedShelf]) {
        if (entries.some((e) => e.bookId === id)) continue;
        feedShelf.delete(id);
        await api.removeBook({ bookId: id });
      }
      feedsChanged();
    },
    async feedEntries({ feedId = null, folder = null, filter, before = null, limit = null }) {
      const all = entryViews().filter(
        (e) =>
          (feedId === null || e.feedId === feedId) &&
          (folder === null || feeds.find((f) => f.id === e.feedId)?.folder === folder) &&
          (filter === "all" || (filter === "unread" ? !e.read : e.inLibrary)),
      );
      const start = before === null ? 0 : all.findIndex((e) => e.id === before) + 1;
      return structuredClone(all.slice(start, start + Math.min(limit ?? 50, 200)));
    },
    async openFeedEntry({ entryId }) {
      const entry = getEntry(entryId);
      const id = entryItem(entry);
      entry.read = true;
      feedsChanged();
      return id;
    },
    async saveFeedEntry({ entryId }) {
      const id = entryItem(getEntry(entryId));
      await api.saveToLibrary({ bookId: id });
      return id;
    },
    /** As the core does: saved, an entry's item goes on its feed's collection too. */
    async saveToLibrary({ bookId: id }) {
      if (!feedShelf.delete(id)) return;
      const feedId = entries.find((e) => e.bookId === id)?.feedId;
      const collection = feeds.find((f) => f.id === feedId)?.collectionId ?? null;
      const book = books.find((b) => b.id === id);
      if (book && collection !== null && !book.collections.includes(collection))
        book.collections = shelves
          .filter((s) => s.id === collection || book.collections.includes(s.id))
          .sort((a, b) => a.name.localeCompare(b.name))
          .map((s) => s.id);
      setArticle(id, { inLibrary: true });
      feedsChanged();
    },
    async setFeedCollection({ feedId, collectionId = null }) {
      if (!feeds.some((f) => f.id === feedId)) throw refused("No such feed.");
      if (collectionId !== null && !shelves.some((s) => s.id === collectionId)) throw refused("no such collection");
      feeds = feeds.map((f) => (f.id === feedId ? { ...f, collectionId } : f));
      feedsChanged();
    },
    /** As the core does: a feed address subscribes, anything else is saved as an article. */
    async addLink({ url }): Promise<AddedLink> {
      if (isFeedAddress(parseLink(url).href)) {
        const result = await api.subscribe({ url });
        if (result.kind === "subscribed") return result;
      }
      return { kind: "saved", value: await api.saveArticle({ url }) };
    },
    async markEntryRead({ entryId, read }) {
      getEntry(entryId).read = read;
      feedsChanged();
    },
    async markAllRead({ feedId = null, folder = null } = {}) {
      const inFolder = (id: number) => folder === null || feeds.find((f) => f.id === id)?.folder === folder;
      for (const entry of entries)
        if ((feedId === null || entry.feedId === feedId) && inFolder(entry.feedId)) entry.read = true;
      feedsChanged();
    },
    /** As the core does: a blank name is no folder. */
    async setFeedFolder({ feedId, folder = null }) {
      if (!feeds.some((f) => f.id === feedId)) throw refused("No such feed.");
      const name = folder?.trim() || null;
      feeds = feeds.map((f) => (f.id === feedId ? { ...f, folder: name } : f));
      feedsChanged();
    },
    /** As the core does: renamed onto another folder, the two become one. */
    async renameFeedFolder({ from, to }) {
      const name = to.trim();
      if (!name) throw refused("a folder needs a name");
      feeds = feeds.map((f) => (f.folder === from ? { ...f, folder: name } : f));
      feedsChanged();
    },
    /** Feeds that answer were just checked; a failing one still fails. */
    async checkFeeds() {
      const now = Date.now();
      feeds = feeds.map((f) => (f.stage.kind === "failing" ? f : { ...f, lastOkAt: now }));
      feedsChanged();
    },
    /** The preview opens nothing. */
    async openFeedSite({ feedId }) {
      if (!feeds.some((f) => f.id === feedId)) throw refused("No such feed.");
    },
    async chooseOpml() {
      return "~/subscriptions.opml";
    },
    async chooseOpmlTarget() {
      return "~/subscriptions.opml";
    },
    /** The preview's subscription list (`web.json`), whatever file is chosen: feeds already
     *  subscribed are skipped. */
    async importOpml() {
      const listed = fixtureWeb.opml;
      let added = 0;
      for (const item of listed) {
        if (feeds.some((f) => f.url === item.url)) continue;
        added++;
        feeds.push({
          id: Math.max(0, ...feeds.map((f) => f.id)) + 1,
          title: item.title,
          url: item.url,
          siteUrl: new URL("/", item.url).href,
          unread: 0,
          stage: { kind: "idle" },
          error: null,
          lastOkAt: null,
          collectionId: null,
          folder: null,
        });
      }
      feeds.sort((a, b) => (a.title ?? a.url).localeCompare(b.title ?? b.url));
      feedsChanged();
      return { added, skipped: listed.length - added };
    },
    async exportOpml() {
      return feeds.length;
    },
    onFeedsChanged: (h) => subscribe(feedHandlers, h),
    /** The preview opens nothing. */
    async openOriginal({ bookId: id }) {
      if (!articles.has(id)) throw refused("Not an article.");
      onTheWeb(id);
    },
    async mergeAudiobook({ audiobookId, bookId }) {
      if (!isAudiobook(audiobookId)) throw refused("That is not an audiobook on its own.");
      const book = getBook(bookId),
        audio = getBook(audiobookId);
      if (book.kind !== "book") throw refused("That is not a book with text.");
      if (recordings.has(bookId)) throw refused("That book already has a recording.");
      if (state.bookId === audiobookId) await api.stop();
      const recording = recordings.get(audiobookId)!;
      recordings.delete(audiobookId);
      for (const mark of bookmarks) if (mark.bookId === audiobookId) mark.bookId = bookId;
      book.collections = [...new Set([...book.collections, ...audio.collections])];
      book.addedAt = Math.min(book.addedAt, audio.addedAt);
      books.splice(books.indexOf(audio), 1);
      emitSync({ ...recording, bookId, stage: "checking", transcribedMs: 0, remainingMs: null });
      changed();
      simulateSync(bookId);
    },
    async attachRecording({ bookId, paths }) {
      getBook(bookId);
      const found = await api.inspectRecording({ paths });
      if (found.problems.length) throw refused(`${found.problems[0].name}: ${found.problems[0].problem}`);
      if (!found.files.length) throw refused("No audio files were found there.");
      emitSync({
        ...recordingIn(bookId, "checking"),
        files: found.files,
        totalMs: found.totalMs,
        chapters: found.chapters,
        transcribedMs: 0,
        remainingMs: null,
      });
      changed();
      simulateSync(bookId);
      return structuredClone(recordings.get(bookId)!);
    },
    async recording({ bookId }) {
      const r = recordings.get(bookId);
      return r ? structuredClone(r) : null;
    },
    async recordings() {
      return [...recordings.values()].map((r) => structuredClone(r));
    },
    async syncReport({ bookId }): Promise<SyncReportView> {
      const r = recordings.get(bookId);
      if (!r?.summary) throw refused("the recording is not matched yet");
      const list = chapters[Number(bookId.split("-")[1])] ?? [];
      return {
        summary: { ...r.summary },
        chapters: list.map((c, i) => ({
          index: i,
          title: c.title,
          words: 5000,
          matchedWords: i === 1 && r.stage === "poorMatch" ? 400 : 4700,
          timedWords: 4980,
          startMs: i * hour,
          endMs: (i + 1) * hour - 2_000,
        })),
        offText: [
          {
            startMs: 0,
            endMs: 31_000,
            excerpt: "This is a recording of The Lantern Keeper, read for you by a narrator",
          },
          {
            startMs: r.totalMs - 28_000,
            endMs: r.totalMs,
            excerpt: "That was The Lantern Keeper. Thank you for listening",
          },
        ],
      };
    },
    async syncAction({ bookId, action }) {
      const r = recordings.get(bookId);
      if (!r) throw refused("the book has no recording");
      const stage = syncStages[r.stage].next[action];
      if (stage === r.stage) return;
      emitSync({
        ...r,
        stage,
        accepted: action === "accept" || r.accepted,
        transcribedMs: action === "resync" ? 0 : r.transcribedMs,
        summary: action === "resync" ? null : r.summary,
      });
      changed();
      if (stage === "waiting" || stage === "checking") simulateSync(bookId);
    },
    async removeRecording({ bookId }) {
      recordings.delete(bookId);
      const book = books.find((b) => b.id === bookId);
      if (book) book.recording = null;
      if (settings.has(bookId)) settings.set(bookId, { ...settingsOf(bookId), narration: null });
      changed();
    },
    async setNarration({ bookId, narration }) {
      const recording = recordings.get(bookId);
      if (narration === "recording" && !recording?.listenable) throw refused("the recording is not synced yet");
      await api.saveBookSettings({ bookId, settings: { ...settingsOf(bookId), narration } });
    },
    async recognitionOptions() {
      const models: [string, "tiny" | "base" | "small", boolean, number][] = [
        ["tiny.en", "tiny", true, 77_704_715],
        ["tiny", "tiny", false, 77_691_713],
        ["base.en", "base", true, 147_964_211],
        ["base", "base", false, 147_951_465],
        ["small.en", "small", true, 487_614_201],
        ["small", "small", false, 487_601_967],
      ];
      return {
        settings: structuredClone(appSettings.recognition),
        gpuBuild: false,
        modelsFolder: "~/.local/share/com.aoide.audiobooks/models",
        models: models.map(([id, size, englishOnly, bytes]) => ({
          id,
          size,
          englishOnly,
          bytes,
          downloaded: downloaded.has(id),
        })),
      };
    },
    async downloadModel({ modelId }) {
      const total = 147_964_211;
      for (const share of [0.25, 0.6, 1]) {
        await new Promise((resolve) => setTimeout(resolve, 150));
        downloadHandlers.forEach((h) =>
          h({
            id: modelId,
            stage: "downloading",
            doneBytes: Math.round(total * share),
            totalBytes: total,
            error: null,
          }),
        );
      }
      downloaded.add(modelId);
      downloadHandlers.forEach((h) =>
        h({ id: modelId, stage: "done", doneBytes: total, totalBytes: total, error: null }),
      );
      for (const r of recordings.values())
        if (r.stage === "needsModel") {
          emitSync({ ...r, stage: "transcribing" });
          simulateSync(r.bookId);
        }
    },
    async chooseBooks() {
      return ["~/Downloads/the-lantern-keeper.epub"];
    },
    async chooseImage() {
      return "~/Pictures/cover.png";
    },
    async chooseFolder() {
      return "~/Audiobooks";
    },
    async chooseAudio({ folder }) {
      return folder ? ["~/Audiobooks/The Lantern Keeper"] : ["~/Audiobooks/the-lantern-keeper.m4b"];
    },
    async playRecordingAt({ bookId, atMs }) {
      if (!recordings.get(bookId)?.narrates)
        throw refused("choose the recording as the book's narration to listen to it");
      const list = chapters[Number(bookId.split("-")[1])] ?? [];
      await api.play({ bookId, chapter: Math.min(list.length - 1, Math.floor(atMs / hour)), segmentId: null });
    },
    onSyncState: (h) => subscribe(syncHandlers, h),
    onArticleState: (h) => subscribe(articleHandlers, h),
    onModelDownload: (h) => subscribe(downloadHandlers, h),
    onBrowserDownload: (h) => subscribe(browserDownloadHandlers, h),
    async sites() {
      // As the core does: a site with an article waiting for the user is listed with it.
      for (const [id, article] of articles)
        if (article.stage.kind === "needsYou") {
          const host = siteHost(new URL(article.canonicalUrl).hostname);
          sites.set(host, {
            host,
            fetch: null,
            siteData: "keep",
            ...sites.get(host),
            session:
              sites.get(host)?.session.kind === "signedIn"
                ? sites.get(host)!.session
                : { kind: "askedFor", reason: article.stage.reason, at: Date.now() },
            waitingArticle: id,
          });
        }
      return [...sites.values()].sort((a, b) => a.host.localeCompare(b.host)).map((s) => structuredClone(s));
    },
    async setSiteRule({ host, fetch = null, siteData }) {
      const key = siteHost(host);
      if (!key) throw refused("No site was named.");
      sites.set(key, {
        ...(sites.get(key) ?? { host: key, session: { kind: "none" }, waitingArticle: null }),
        fetch,
        siteData,
      });
    },
    async signOutSite({ host }) {
      browserOff();
      const site = sites.get(siteHost(host));
      if (site && (site.session.kind === "signedIn" || site.session.kind === "checkPassed"))
        site.session = { kind: "none" };
    },
    async forgetSite({ host }) {
      browserOff();
      sites.delete(siteHost(host));
    },
    async startSignIn({ bookId: id }) {
      onTheWeb(id);
      browserOff();
      if (signInOpen()) throw refused("A sign-in is already open. Finish or cancel it first.");
      openSignIn(id);
      setTimeout(() => tellSignIn({ stage: { kind: "waiting" } }), 200);
      return structuredClone(signIn!);
    },
    async signIn() {
      return signIn && structuredClone(signIn);
    },
    async finishSignIn() {
      if (signIn?.stage.kind !== "waiting") throw refused("No sign-in is open.");
      fetchSignedIn();
    },
    async cancelSignIn() {
      if (!signInOpen()) throw refused("No sign-in is open.");
      if (signIn!.stage.kind !== "fetching") tellSignIn({ stage: { kind: "cancelled" } });
    },
    async showSignInWindow() {
      if (!signInOpen()) throw refused("No sign-in is open.");
    },
    async staySignedIn({ stay }) {
      if (!signInOpen()) throw refused("No sign-in is open.");
      tellSignIn({ staySignedIn: stay });
    },
    onSignInState: (h) => subscribe(signInHandlers, h),
    async chooseBrowser() {
      return "/opt/chromium/chrome";
    },
    async clearBrowserData() {
      for (const site of sites.values())
        if (site.session.kind === "signedIn" || site.session.kind === "checkPassed") site.session = { kind: "none" };
    },
    async browserStatus() {
      return browserStatus();
    },
    /** As the core does: refused while off; progress, a check, unpacking, done; resumable. */
    async downloadBrowser() {
      if (appSettings.browser.use === "off") throw refused("The browser is off. Turn it on in Settings › Sites.");
      if (browserDownload && ["downloading", "checking", "unpacking"].includes(browserDownload.stage)) return;
      const from = browserDownload?.stage === "cancelled" ? browserDownload.doneBytes : 0;
      cancelBrowser = false;
      browserDownload = {
        stage: "downloading",
        version: BROWSER_VERSION,
        doneBytes: from,
        totalBytes: BROWSER_BYTES,
        error: null,
      };
      tellBrowserDownload({});
      for (const share of [0.3, 0.65, 1]) {
        await new Promise((resolve) => setTimeout(resolve, 150));
        if (cancelBrowser) return tellBrowserDownload({ stage: "cancelled" });
        tellBrowserDownload({ doneBytes: Math.max(from, Math.round(BROWSER_BYTES * share)) });
      }
      tellBrowserDownload({ stage: "checking" });
      tellBrowserDownload({ stage: "unpacking" });
      browserDownloaded = true;
      tellBrowserDownload({ stage: "done" });
    },
    async cancelBrowserDownload() {
      cancelBrowser = true;
    },
    async removeBrowser() {
      if (browserDownload && ["downloading", "checking", "unpacking"].includes(browserDownload.stage))
        throw refused("The browser is still downloading. Cancel it first.");
      browserDownloaded = false;
      browserDownload = null;
    },
    onProblem: (h) => subscribe(problemHandlers, h),
    // The browser preview has no desktop notifications to click.
    onOpenItem: (h) => subscribe(new Set<(id: string) => void>(), h),
    // Nor a browser to send it pages.
    onLinkSaved: (h) => subscribe(new Set<(saved: SavedLink) => void>(), h),
    // Nor a folder it watches.
    onInboxAdded: (h) => subscribe(new Set<(books: InboxBook[]) => void>(), h),
    onFinished: (h) => subscribe(finishedHandlers, h),
  };
  /** Test hooks: put a book at a share of its length, or give it generated audio. */
  const setProgressForTest = (id: string, fraction: number) => {
    getBook(id).progress = { chapter: 0, segmentId: null, fraction, positionMs: null };
    changed();
  };
  const setAudioForTest = (id: string, bytes: number) => {
    getBook(id).audioBytes = bytes;
    changed();
  };
  /** A book left in the queue by an earlier run that has not started again (resuming at startup is off). */
  const queueWaitingForTest = (id: string) => {
    prepareQueue.push({ id, stage: "waiting", chapters: null });
    getBook(id).prepare = prepareView("waiting", 0, 0, 0, null);
    settlePrepare();
  };
  /** Gives a book a recording in a stage (and, when ready, the recording as its narration). */
  const setRecordingForTest = (
    id: string,
    stage: SyncStage,
    narration: "speech" | "recording" | null = stage === "ready" ? "recording" : null,
    more: Partial<RecordingView> = {},
  ) => {
    settings.set(id, { ...settingsOf(id), narration });
    // As the core would report it: a ready recording chosen as the narration is what plays.
    if (narration !== "speech" && stage === "ready") getBook(id).audio = "recording";
    emitSync({ ...recordingIn(id, stage), ...more });
    changed();
  };
  /** Home as the core would make it for another library (an empty one, say). */
  const setHomeForTest = (view: HomeView) => {
    homeOverride = structuredClone(view);
    changed();
  };
  /** What a book would play with, as the core would report it. */
  const setAudioSourceForTest = (id: string, audio: BookSummary["audio"]) => {
    getBook(id).audio = audio;
    changed();
  };
  /** `count` older entries, read, from the first feed (to page through). */
  const addFeedEntriesForTest = (count: number) => {
    const oldest = Math.min(...entries.map((e) => e.at));
    const first = Math.max(...entries.map((e) => e.id));
    for (let i = 1; i <= count; i++)
      entries.push({
        ...entries[0],
        id: first + i,
        title: `Older notice ${i}`,
        at: oldest - i * 3_600_000,
        read: true,
        bookId: null,
      });
    feedsChanged();
  };
  return Object.assign(api, {
    addFeedEntriesForTest,
    setHomeForTest,
    setAudioSourceForTest,
    setRecordingForTest,
    setProgressForTest,
    setAudioForTest,
    queueWaitingForTest,
    dispose: () => {
      halt();
      cancelHandOver();
      clearTimeout(remaking);
      clearInterval(syncTimer);
    },
    emitPlayer: sendPlayer,
    /** The core asks whether to keep working (the window was closed while working). */
    emitCloseAsked: () => closeAskedHandlers.forEach((h) => h()),
    /** The core reports an import of links (for tests). */
    emitLinkImport: (view: LinkImportView) => setLinkImport(view),
    emitGeneration,
    emitConfig,
    emitProblem: (p: ProblemView) => problemHandlers.forEach((h) => h({ ...p })),
    /** A status from the core; `wanted` defaults to true (the user has asked to listen). */
    emitEngine: (
      s: Omit<EngineStatusView, "wanted" | "server"> & Partial<Pick<EngineStatusView, "wanted" | "server">>,
    ) => engineHandlers.forEach((h) => h({ wanted: true, server: null, ...s })),
  });
}
