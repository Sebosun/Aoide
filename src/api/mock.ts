import type {
  ArticleView,
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
  SyncReportView,
  PrepareState,
  Preferences,
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
  AppSettings,
  BookDetails,
  MetadataMatch,
  ExportProgress,
  SearchHit,
  HomeView,
} from "@/api/types";
import type * as Wire from "@/api/contract";
import fixtureLibrary from "@/tests/fixtures/library.json";
import fixtureViews from "@/tests/fixtures/book-views.json";
import fixturePlayer from "@/tests/fixtures/player.json";
import fixtureGenerationPlaying from "@/tests/fixtures/generation-playing.json";
import fixtureRecording from "@/tests/fixtures/recording.json";
import fixtureHome from "@/tests/fixtures/home.json";
import fixtureAudiobook from "@/tests/fixtures/audiobook.json";
import fixtureArticle from "@/tests/fixtures/article.json";
import fixtureFeeds from "@/tests/fixtures/feeds.json";

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
        : (fixtureViews as unknown as Record<string, BookView>)[id],
  );
/** A saved link as the core reads it (core/articles.rs `parse_link`). */
const parseLink = (link: string) => {
  const text = link.trim();
  const invalid = () => new Error("That is not a web address.");
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
  const engine: EngineStatusView = {
    profileName: "Local Chatterbox",
    state: "ready",
    detail: "Browser preview · simulated audio",
    wanted: false,
  };
  const books = coreBooks();
  /** Home as the core made it for these books; `setHome` replaces it (an empty library). */
  let home: HomeView = structuredClone(fixtureHome);
  const shelves: { id: number; name: string; createdAt: number }[] = [];
  const shelfName = (name: string, except?: number) => {
    const trimmed = name.trim();
    if (!trimmed) throw new Error("a collection needs a name");
    if (shelves.some((s) => s.id !== except && s.name.toLowerCase() === trimmed.toLowerCase()))
      throw new Error(`there is already a collection called “${trimmed}”`);
    return trimmed;
  };
  const texts = [
    [
      [
        "The small light",
        "Every evening, Ada carried a lantern to the end of the pier. The sea collected its reflection and broke it into a hundred little stars.",
        "Tonight a paper boat waited beside the steps. Inside it lay a key, warm as if someone had held it all the way across the bay.",
      ],
      [
        "Across the water",
        "At dawn she borrowed the blue rowing boat. Each pull of the oars opened a quiet doorway in the water.",
        "On the far shore, a gardener waved. He had been waiting for someone to bring back the light.",
      ],
    ],
    [
      [
        "An ordinary discovery",
        "No one had marked the bench on any map. It stood between a bakery and a clock shop, where the morning smelled of bread and rain.",
        "Ren sat down and listened. Beneath the traffic, a sparrow was rehearsing a song with no ending.",
      ],
      [
        "The long way home",
        "Ren drew a circle where the bench should be. Then came the bridge, the cool stone steps, and the lane where the wind always rested.",
        "The map did not make the city smaller. It made room for all the things Ren had hurried past.",
      ],
      [
        "A place to begin",
        "The next morning, a stranger asked for directions. Ren folded the map and offered to walk beside her.",
        "They took the long way. There was plenty of time.",
      ],
    ],
  ];
  const chapters: ChapterView[][] = texts.map((book) =>
    book.map((parts, c) => ({
      index: c,
      title: parts[0],
      blocks: parts.map((text, b) => ({
        kind: b === 0 ? "heading" : "paragraph",
        level: b === 0 ? 2 : null,
        segments: [
          {
            id: `${c}:${b}:0`,
            words: text.split(/\s+/),
            speakable: true,
            note: false,
            notes:
              c === 0 && b === 1
                ? [{ afterWord: 2, label: "1", text: "A short original note shown in the browser preview." }]
                : [],
          },
        ],
      })),
      notNarrated: null,
    })),
  );
  const descriptors: EngineDescriptor[] = [
    {
      kind: "chatterbox",
      name: "Chatterbox TTS Server",
      adapterVersion: 1,
      languages: ["en", "fr", "de"],
      wordTimings: false,
      maxChars: 300,
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
    launch: null,
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
    };
  const bookmarks: Bookmark[] = [];
  let nextBookmark = 1;
  const listened = new Map<string, number>([
    ["book-0", 5_400_000],
    ["book-1", 1_200_000],
  ]);
  let appSettings: AppSettings = {
    metadataLookup: true,
    cacheLimitBytes: null,
    aligner: null,
    librarySort: "recent",
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
  };
  /** The browser for articles, as the core finds it (core/browsing.rs). `?browser=off|missing|ready`:
   *  off (the default) finds Chromium but does not run it; missing finds none; ready finds one. */
  const systemChromium = new URLSearchParams(search).get("browser") !== "missing";
  let browserDownloaded = false;
  let browserDownload: BrowserDownloadView | null = null;
  let cancelBrowser = false;
  const browserDownloadHandlers = new Set<(d: BrowserDownloadView) => void>();
  const BROWSER_VERSION = "157.0.8103.78";
  const BROWSER_BYTES = 203_266_906;
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
    if (appSettings.browser.use === "off") throw new Error("The browser is off. Turn it on in Settings › Sites.");
  };
  const tellBrowserDownload = (change: Partial<BrowserDownloadView>) => {
    browserDownload = { ...browserDownload!, ...change };
    browserDownloadHandlers.forEach((h) => h({ ...browserDownload! }));
  };
  /** Minutes read silently, per book. */
  const readMs = new Map<string, number>([["book-1", 1_800_000]]);
  const found: Candidate[] = [
    {
      id: "h-tide",
      status: "new",
      title: "Notes on the Tide",
      authors: ["Ines Morrow"],
      language: "en",
      isbn: "9780000000101",
      coverUrl: null,
      paths: ["~/Downloads/notes-on-the-tide.epub", "~/Documents/Books/tide.epub"],
      size: 2_400_000,
      error: null,
      suggestion: null,
      audiobook: ["~/Downloads/Notes on the Tide (Unabridged).m4b"],
    },
    {
      id: "h-glass",
      status: "new",
      title: "The Glass Meridian",
      authors: ["Oren Vale"],
      language: "en",
      isbn: null,
      coverUrl: null,
      paths: ["~/Downloads/glass-meridian.epub"],
      size: 1_100_000,
      error: null,
      suggestion: null,
      audiobook: [],
    },
    {
      id: "book-0",
      status: "inLibrary",
      title: "The Lantern Keeper",
      authors: ["Mira Vale"],
      language: "en",
      isbn: null,
      coverUrl: null,
      paths: ["~/Documents/lantern.epub"],
      size: 900_000,
      error: null,
      suggestion: null,
      audiobook: ["~/Audiobooks/Mira Vale - The Lantern Keeper"],
    },
    {
      id: "path:~/Documents/broken.epub",
      status: "unreadable",
      title: "broken",
      authors: [],
      language: null,
      isbn: null,
      coverUrl: null,
      paths: ["~/Documents/broken.epub"],
      size: 10,
      error: "not a readable EPUB",
      suggestion: null,
      audiobook: [],
    },
  ];
  /** What a scan found; like the app, a book that has a recording is not offered another. */
  const listed = () => structuredClone(found).map((c) => (recordings.has(c.id) ? { ...c, audiobook: [] } : c));
  const query = new URLSearchParams(search);
  const mode = query.get("theme");
  let preferences: Preferences = {
    appearance: {
      mode: mode === "light" || mode === "dark" ? mode : "auto",
      theme: "default",
      textSize: 2,
      readerMode: query.get("mode") === "lines" ? "lines" : "page",
      zoom: Number(query.get("zoom")) || 1,
    },
    themes: [
      { name: "dusk", light: { accent: "#B4472B" }, dark: { accent: "#FFB27A", ground: "#120E1C" }, problems: [] },
    ],
    configDir: "~/.config/aoide",
    problems: [],
  };
  /** Recordings paired with books, like the app (core/recording.rs): one per book, each with an
   *  explicit sync stage. `?recording=syncing|ready|review|missing|model|mismatch|failed` gives
   *  book-0 a recording in that stage. */
  const recordings = new Map<string, RecordingView>();
  const syncHandlers = new Set<(r: RecordingView) => void>();
  const downloadHandlers = new Set<(d: ModelDownloadView) => void>();
  const problemHandlers = new Set<(p: ProblemView) => void>();
  const downloaded = new Set<string>(["base.en"]);
  const hour = 3_600_000;
  /** What the core decides about a recording (core/recording.rs): what is synced so far can be
   *  listened to, and it narrates the book once chosen. */
  const decide = (r: Omit<RecordingView, "listenable" | "narrates">): RecordingView => {
    const listenable =
      r.stage === "ready" ||
      (["waiting", "transcribing", "paused", "matching"].includes(r.stage) && r.transcribedMs > 0);
    return { ...r, listenable, narrates: listenable && settingsOf(r.bookId).narration === "recording" };
  };
  /** The recording the core synced (its fixture), as it stands in `stage`. */
  const recordingIn = (bookId: string, stage: SyncStage): Omit<RecordingView, "listenable" | "narrates"> => {
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
  const emitSync = (undecided: Omit<RecordingView, "listenable" | "narrates">) => {
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
  const kept: Record<string, AudiobookMatch["candidates"]> = {
    "book-1": [
      {
        paths: ["~/Audiobooks/Ellis Reed - A Map of Quiet Places"],
        exact: true,
        folder: true,
        bytes: 1_288_490_189,
        files: Array.from(
          { length: 15 },
          (_, i) =>
            `~/Audiobooks/Ellis Reed - A Map of Quiet Places/${String(i + 1).padStart(2, "0")} - A Map of Quiet Places.m4b`,
        ),
      },
      {
        paths: ["~/Audiobooks/A Map of Quiet Places (Unabridged).m4b"],
        exact: true,
        folder: false,
        bytes: 625_999_872,
        files: ["~/Audiobooks/A Map of Quiet Places (Unabridged).m4b"],
      },
    ],
    "book-3": [
      {
        paths: ["~/Audiobooks/Unfinished Atlas 1.mp3", "~/Audiobooks/Unfinished Atlas 2.mp3"],
        exact: false,
        folder: false,
        bytes: 429_916_160,
        files: ["~/Audiobooks/Unfinished Atlas 1.mp3", "~/Audiobooks/Unfinished Atlas 2.mp3"],
      },
    ],
  };
  const emitMatches = () => matchHandlers.forEach((h) => h(structuredClone(audiobookMatches)));
  const details = new Map<string, BookDetails>();
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
  let state = structuredClone(fixturePlayer) as PlayerState;
  let timer: ReturnType<typeof setInterval> | undefined;
  const emitPlayer = (next: PlayerState) => {
    state = { ...next };
    playerHandlers.forEach((h) => h({ ...state }));
  };
  const emitGeneration = (next: GenerationState) => generationHandlers.forEach((h) => h({ ...next }));
  const changed = () => libraryHandlers.forEach((h) => h());
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
    // New and unread, as the core's Home has it: first in Up next and Recently added.
    home.upNextIds = [id, ...home.upNextIds.filter((other) => other !== id)];
    home.recentIds = [id, ...home.recentIds.filter((other) => other !== id)];
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
    if (!article) throw new Error("Not an article.");
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
   *  preview (`feeds.json`, times relative to now). `?feeds=empty` has none; `?feeds=failing`
   *  adds one that stopped answering. Every entry opens as the preview's article (book-6),
   *  which stays off the library's lists (on the feed shelf) until it is saved. */
  const feedHandlers = new Set<() => void>();
  const feedsChanged = () => feedHandlers.forEach((h) => h());
  const feedsParam = query.get("feeds");
  const loaded = Date.now();
  const failingFeeds = new Set(
    (fixtureFeeds.feeds as FeedView[]).filter((f) => f.stage.kind === "failing").map((f) => f.id),
  );
  let feeds: FeedView[] =
    feedsParam === "empty"
      ? []
      : (structuredClone(fixtureFeeds.feeds) as FeedView[])
          .filter((f) => feedsParam === "failing" || !failingFeeds.has(f.id))
          .map((f) => ({
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
    if (!entry) throw new Error("No such entry.");
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
      home.upNextIds = home.upNextIds.filter((other) => other !== id);
      home.recentIds = home.recentIds.filter((other) => other !== id);
    }
    entry.bookId = id;
    return id;
  };
  /** Whole-book preparation, like the app (core/preparation.rs): each queued book has an explicit
   *  stage; the mock plans a book as soon as it is queued (books from `queueWaitingForTest` wait). */
  type PrepareEvent = "planned" | "pause" | "resume" | "promote" | "demote";
  const nextStage = (stage: PrepareState, event: PrepareEvent): PrepareState =>
    stage === "waiting" && event === "planned"
      ? "queued"
      : (stage === "waiting" || stage === "queued" || stage === "preparing") && event === "pause"
        ? "paused"
        : stage === "paused" && event === "resume"
          ? "waiting"
          : stage === "queued" && event === "promote"
            ? "preparing"
            : stage === "preparing" && event === "demote"
              ? "queued"
              : stage;
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
      if (b?.prepare) b.prepare = { ...b.prepare, position, state: entry.stage, chapters: entry.chapters };
    });
    changed();
  }
  const unused = { files: 3, bytes: 12 * 1024 ** 2 };
  const getBook = (id: string) => {
    const b = books.find((b) => b.id === id);
    if (!b || b.status.kind !== "present") throw new Error("Book is unavailable.");
    return b;
  };
  const getChapter = (id: string, chapter: number) => {
    getBook(id);
    const book = chapters[Number(id.split("-")[1])],
      result = book?.[chapter];
    if (!result) throw new Error("Chapter not found.");
    const view = structuredClone(result);
    // A recording narrating the book lacks its last chapter, to show how that looks.
    if (recordings.get(id)?.narrates && recordings.get(id)?.stage === "ready") {
      view.notNarrated =
        book.length > 1 && chapter === book.length - 1
          ? view.blocks
              .flatMap((b) => b.segments)
              .filter((s) => s.speakable)
              .map((s) => s.id)
          : [];
    }
    return view;
  };
  const halt = () => {
    clearInterval(timer);
    timer = undefined;
  };
  /** An audiobook on its own plays on a timeline of its parts, as the core does
   *  (core/timeline.rs); its parts and their lengths come from the core's view of it. */
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
      chapterLeftMs: Math.round((part.end - position) / state.speed),
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
    b.readState = at + 1000 >= total ? "finished" : "inProgress";
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
  const api: AoideApi = {
    async logUi() {},
    async appInfo() {
      return { libraryFolder: "~/Documents/Aoide/Books" };
    },
    async listBooks() {
      return structuredClone(books.filter((b) => !feedShelf.has(b.id)));
    },
    async home() {
      return structuredClone(home);
    },
    async importFiles(paths) {
      return paths.map((path) => ({
        path,
        bookId: null,
        error: "Browser preview cannot read local paths. Add books in the desktop app.",
      }));
    },
    /** As the core does for the preview's audiobook (`audiobook.json`): added once, then the
     *  same item; a protected file is refused. */
    async addAudiobook(paths) {
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
        .map((s) => ({ ...s, books: books.filter((b) => b.collections.includes(s.id)).length }))
        .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
    },
    async createCollection(name) {
      const id = (shelves.at(-1)?.id ?? 0) + 1;
      shelves.push({ id, name: shelfName(name), createdAt: Date.now() });
      changed();
      return id;
    },
    async renameCollection(id, name) {
      const shelf = shelves.find((s) => s.id === id);
      if (!shelf) throw new Error("no such collection");
      shelf.name = shelfName(name, id);
      changed();
    },
    async deleteCollection(id) {
      const at = shelves.findIndex((s) => s.id === id);
      if (at >= 0) shelves.splice(at, 1);
      books.forEach((b) => {
        b.collections = b.collections.filter((c) => c !== id);
      });
      changed();
    },
    async setBookCollections(id, ids) {
      if (ids.some((c) => !shelves.some((s) => s.id === c))) throw new Error("no such collection");
      getBook(id).collections = shelves
        .filter((s) => ids.includes(s.id))
        .sort((a, b) => a.name.localeCompare(b.name))
        .map((s) => s.id);
      changed();
    },
    async removeBook(id) {
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
    async showInFolder(id) {
      getBook(id);
    },
    async openBook(id): Promise<BookView> {
      const b = getBook(id);
      b.lastOpenedAt = Date.now();
      return api.bookOverview(id);
    },
    async bookOverview(id): Promise<BookView> {
      const b = getBook(id),
        view = coreView(id);
      if (isArticle(id) && !fetched.has(id)) throw new Error(articles.get(id)!.error ?? "Not fetched yet.");
      if (!view) throw new Error("Book is unavailable.");
      const voice = settings.get(id)?.voiceId,
        offered = (await api.engineVoices()).map((v) => v.id);
      const voiceWarning =
        voice && !offered.includes(voice)
          ? `${voice.replace(".wav", "")} is not one of Mock engine's voices, so Emily reads this book. Choose a voice in the book's settings.`
          : null;
      return {
        ...view,
        title: b.title,
        progress: structuredClone(b.progress),
        voiceWarning,
        mode: b.mode,
        shape: b.shape,
      };
    },
    async setMode(id, mode) {
      const b = getBook(id);
      if (mode === "read" && isAudiobook(id)) throw new Error(NO_TEXT);
      if (mode === "read" && state.bookId === id && state.status === "playing") await api.pause();
      b.mode = mode;
      b.shape = mode === "read" ? "read" : "readAlong";
      settings.set(id, { ...settingsOf(id), mode });
      changed();
      return api.bookOverview(id);
    },
    async chapter(id, chapter) {
      if (isAudiobook(id)) throw new Error(NO_TEXT);
      if (isArticle(id)) {
        if (!fetched.has(id)) throw new Error(articles.get(id)!.error ?? "Not fetched yet.");
        if (chapter !== 0) throw new Error("Chapter not found.");
        return structuredClone(fixtureArticle.chapter) as unknown as ChapterView;
      }
      return getChapter(id, chapter);
    },
    async markFinished(id) {
      getBook(id).readState = "finished";
      changed();
    },
    async markUnread(id) {
      const b = getBook(id);
      b.readState = "unread";
      b.progress = null;
      changed();
    },
    async setReadingPlace(id, chapter, segmentId) {
      const b = getBook(id);
      if (state.bookId === id && state.status === "playing") return null;
      // As the core does: the place's share of the book's spoken segments.
      const all = chapters[Number(id.split("-")[1])].flatMap((c, n) =>
        c.blocks.flatMap((block) => block.segments.filter((s) => s.speakable).map((s) => `${n}/${s.id}`)),
      );
      const index = all.indexOf(`${chapter}/${segmentId}`);
      if (index < 0) throw new Error("That place is not in the book.");
      b.progress = { chapter, segmentId, fraction: all.length ? index / all.length : 0, positionMs: null };
      if (index === all.length - 1) b.readState = "finished";
      else if (b.readState === "unread") b.readState = "inProgress";
      changed();
      return structuredClone(b.progress);
    },
    async readingActivity(id) {
      getBook(id);
      readMs.set(id, (readMs.get(id) ?? 0) + 15_000);
    },
    async play(id, chapter, segmentId) {
      const book = getBook(id);
      if (isAudiobook(id)) {
        const parts = partsOf(id),
          total = parts.at(-1)?.end ?? 0;
        const saved = state.bookId === id ? state.positionMs : (book.progress?.positionMs ?? null);
        // As the core does: from the part asked for, else the saved place (from the start once
        // it was finished).
        const resume = saved !== null && saved + 1000 < total ? saved : 0;
        const at = chapter !== null ? (parts[chapter]?.start ?? 0) : resume;
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
        bookId: id,
        chapter: c,
        segmentId: segment.id,
        wordIndex: 0,
        status: "playing",
        chapterFraction: segments.indexOf(segment) / segments.length,
        error: null,
        source,
        offText: false,
        chapterLeftMs: null,
        waitingForSync: false,
        positionMs: null,
        durationMs: null,
        partPositionMs: null,
      });
      emitGeneration({ ...(structuredClone(fixtureGenerationPlaying) as GenerationState), bookId: id });
      run();
    },
    async pause() {
      halt();
      emitPlayer({ ...state, status: "paused" });
    },
    async resume() {
      if (state.bookId) {
        emitPlayer({ ...state, status: "playing" });
        run();
      }
    },
    async stop() {
      halt();
      emitPlayer({ ...state, status: "stopped", wordIndex: null });
    },
    async setSpeed(speed) {
      emitPlayer({ ...state, speed });
      if (state.status === "playing") run();
    },
    async setVolume(volume) {
      emitPlayer({ ...state, volume: Math.min(1, Math.max(0, volume)) });
    },
    async engineStatus() {
      return { ...engine };
    },
    async engineOptions() {
      return { descriptors: structuredClone(descriptors), profile: structuredClone(profile) };
    },
    async previewVoice() {
      // A 10 ms silent 16-bit mono WAV.
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
    },
    async engineVoices() {
      return ["Emily.wav", "Adrian.wav", "Olivia.wav"].map((id) => ({ id, displayName: id.replace(".wav", "") }));
    },
    async saveEngineProfile(next) {
      if (!descriptors.some((d) => d.kind === next.kind))
        throw new Error(`No adapter for engine kind \`${next.kind}\``);
      if (next.network === "loopback" && !/^https?:\/\/(127\.|localhost|\[::1\])/.test(next.baseUrl))
        throw new Error("Only loopback addresses are allowed unless home-network access is on.");
      profile = JSON.parse(JSON.stringify(next));
      engine.profileName = next.name;
    },
    async bookSettings(id) {
      return structuredClone(settingsOf(id));
    },
    async saveBookSettings(id, next) {
      getBook(id);
      settings.set(id, JSON.parse(JSON.stringify(next)));
      const recording = recordings.get(id);
      if (recording) emitSync(recording); // Whether it narrates the book may have changed.
    },
    async prepareBook(id) {
      wantEngine();
      const b = getBook(id);
      const entry = prepareEntry(id) ?? prepareQueue[prepareQueue.push({ id, stage: "waiting", chapters: null }) - 1];
      entry.chapters = null;
      entry.stage = nextStage(entry.stage, "planned");
      if (entry.stage === "queued") b.prepare = { state: "queued", done: 3, total: 120, position: 0, chapters: null };
      settlePrepare();
    },
    async setPreparePaused(id, paused) {
      const entry = prepareEntry(id);
      if (entry) entry.stage = nextStage(entry.stage, paused ? "pause" : "resume");
      if (entry?.stage === "waiting") entry.stage = nextStage(entry.stage, "planned"); // The mock plans at once.
      settlePrepare();
    },
    async prepareChapter(id, chapter) {
      const b = getBook(id);
      if (!chapters[Number(id.split("-")[1])]?.[chapter]) throw new Error("no such chapter");
      const entry = prepareEntry(id);
      if (entry) {
        if (entry.chapters && !entry.chapters.includes(chapter))
          entry.chapters = [...entry.chapters, chapter].sort((x, y) => x - y);
        settlePrepare();
        return;
      }
      const added = prepareQueue[prepareQueue.push({ id, stage: "waiting", chapters: [chapter] }) - 1];
      added.stage = nextStage(added.stage, "planned");
      b.prepare = { state: added.stage, done: 0, total: 12, position: 0, chapters: [chapter] };
      settlePrepare();
    },
    async chapterAudio(id) {
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
    async cancelPrepare(id) {
      const b = books.find((b) => b.id === id);
      if (b) b.prepare = null;
      const at = prepareQueue.findIndex((e) => e.id === id);
      if (at >= 0) prepareQueue.splice(at, 1);
      settlePrepare();
    },
    async regenerateBook(id) {
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
    async setSleepTimer(minutes, endOfChapter) {
      emitPlayer({
        ...state,
        sleep:
          minutes === null && !endOfChapter
            ? null
            : { remainingMs: minutes === null ? null : minutes * 60_000, endOfChapter },
      });
    },
    async searchBook(id, query) {
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
    async skipChapter(delta) {
      if (state.bookId && state.positionMs !== null) {
        const parts = partsOf(state.bookId),
          part = Math.min(parts.length - 1, Math.max(0, state.chapter + delta));
        await api.seek(state.bookId, parts[part].start);
        await api.resume();
      } else if (state.bookId) {
        const list = chapters[Number(state.bookId.split("-")[1])];
        const c = Math.min(list.length - 1, Math.max(0, state.chapter + delta));
        await api.play(state.bookId, c, list[c].blocks[0].segments[0].id);
      }
    },
    async seek(id, positionMs) {
      const total = partsOf(id).at(-1)?.end ?? 0,
        at = Math.min(total, Math.max(0, positionMs));
      if (state.bookId === id && state.positionMs !== null) emitPlayer(timelineState(id, at, state.status));
      keepTime(id, at);
    },
    async skip(id, deltaMs) {
      const at = state.bookId === id ? state.positionMs : (getBook(id).progress?.positionMs ?? 0);
      await api.seek(id, (at ?? 0) + deltaMs);
    },
    async bookmarks(id) {
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
    async addBookmark(id, note) {
      const book = getBook(id);
      const playing = state.bookId === id;
      if (isAudiobook(id)) {
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
      const chapter = playing ? state.chapter : (book.progress?.chapter ?? 0);
      const segmentId = (playing ? state.segmentId : book.progress?.segmentId) ?? null;
      const segment = getChapter(id, chapter)
        .blocks.flatMap((b) => b.segments)
        .find((s) => s.id === segmentId);
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
    async updateBookmarkNote(id, note) {
      const mark = bookmarks.find((b) => b.id === id);
      if (!mark) throw new Error("Bookmark not found.");
      mark.note = note;
    },
    async deleteBookmark(id) {
      const index = bookmarks.findIndex((b) => b.id === id);
      if (index >= 0) bookmarks.splice(index, 1);
    },
    async bookListening(id) {
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
    async bookDetails(id) {
      getBook(id);
      return structuredClone(
        details.get(id) ?? {
          description: "An original story used in the browser preview.",
          publisher: null,
          isbn: null,
          published: null,
          subjects: [],
          openLibraryKey: null,
        },
      );
    },
    async lookupMetadata(id, _refresh): Promise<MetadataMatch[]> {
      const book = getBook(id);
      if (!appSettings.metadataLookup) throw new Error("Online metadata lookup is turned off in Settings.");
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
    async applyMetadata(id, match) {
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
    async updateAppSettings(patch) {
      appSettings = JSON.parse(JSON.stringify({ ...appSettings, ...patch }));
      return JSON.parse(JSON.stringify(appSettings));
    },
    async preferences() {
      return structuredClone(preferences);
    },
    async saveAppearance(next) {
      preferences = { ...preferences, appearance: { ...next } };
    },
    onConfigChanged: (h) => subscribe(configHandlers, h),
    async exportBook(id) {
      getBook(id);
      const path = `~/Documents/Aoide/Exports/${books.find((b) => b.id === id)!.title}.m4b`;
      exportHandlers.forEach((h) => h({ bookId: id, stage: "done", done: 1, total: 1, path, error: null }));
      return path;
    },
    async exportReadAlong(id) {
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
    async fetchDiscoveredDetails(ids) {
      for (const id of ids) {
        const c = found.find((f) => f.id === id);
        if (c && c.status === "new")
          c.suggestion = {
            key: `/works/${id}`,
            title: c.title,
            authors: c.authors,
            published: "2018",
            publisher: "Preview Press",
            isbn: c.isbn,
            subjects: ["Fiction"],
            coverUrl: null,
          };
        detailHandlers.forEach((h) => h(id));
      }
    },
    async importDiscovered(choices) {
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
        if (choice.pairAudiobook && c.audiobook.length) await api.attachRecording(c.id, c.audiobook);
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
          await api.attachRecording(m.bookId, m.candidates[0].paths);
      unmatched = [
        {
          paths: [WINTER_RADIO],
          exact: false,
          folder: true,
          files: [`${WINTER_RADIO}/01 Part One.mp3`, `${WINTER_RADIO}/02 Part Two.mp3`],
          bytes: 61_400_000,
        },
      ];
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
    async lookForAudiobooks(ids) {
      if (!appSettings.audiobooks.onAdd) return 0;
      await api.scanForAudiobooks();
      return audiobookMatches.filter((m) => ids.includes(m.bookId) && m.stage.kind === "found").length;
    },
    async pairAudiobooks(choices) {
      const outcomes = [];
      for (const { bookId, candidate } of choices) {
        const match = audiobookMatches.find((m) => m.bookId === bookId);
        try {
          if (!match?.candidates[candidate]) throw new Error("no such audiobook match");
          await api.attachRecording(bookId, match.candidates[candidate].paths);
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
    async inspectRecording(paths): Promise<InspectionView> {
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
    async saveArticle(url) {
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
    async refetchArticle(id, source) {
      const view = articles.get(id);
      if (!view) throw new Error("Not an article.");
      if (source === "browser" && appSettings.browser.use === "off")
        throw new Error("The browser is off. Turn it on in Settings › Sites.");
      if (view.stage.kind === "queued" || view.stage.kind === "fetching") return;
      fetchArticle(id, { kind: "ready" }, source);
    },
    async article(id) {
      const view = articles.get(id);
      if (!view) throw new Error("Not an article.");
      return structuredClone(view);
    },
    async feeds() {
      return feedViews();
    },
    /** As the core does: a known address is the feed already subscribed; any other address
     *  ending in a feed's usual name subscribes to a new, empty feed; anything else is no feed. */
    async subscribe(url): Promise<SubscribeResult> {
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
        title: new URL(link).hostname,
        url: link,
        siteUrl: new URL("/", link).href,
        unread: 0,
        stage: { kind: "idle" },
        error: null,
        lastOkAt: Date.now(),
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
    async unsubscribe(feedId) {
      feeds = feeds.filter((f) => f.id !== feedId);
      entries = entries.filter((e) => e.feedId !== feedId);
      for (const id of [...feedShelf]) {
        if (entries.some((e) => e.bookId === id)) continue;
        feedShelf.delete(id);
        await api.removeBook(id);
      }
      feedsChanged();
    },
    async feedEntries(feedId, filter, before, limit) {
      const all = entryViews().filter(
        (e) =>
          (feedId === null || e.feedId === feedId) &&
          (filter === "all" || (filter === "unread" ? !e.read : e.inLibrary)),
      );
      const start = before === null ? 0 : all.findIndex((e) => e.id === before) + 1;
      return structuredClone(all.slice(start, start + Math.min(limit ?? 50, 200)));
    },
    async openFeedEntry(entryId) {
      const entry = getEntry(entryId);
      const id = entryItem(entry);
      entry.read = true;
      feedsChanged();
      return id;
    },
    async saveFeedEntry(entryId) {
      const id = entryItem(getEntry(entryId));
      await api.saveToLibrary(id);
      return id;
    },
    async saveToLibrary(id) {
      if (!feedShelf.delete(id)) return;
      setArticle(id, { inLibrary: true });
      feedsChanged();
    },
    /** As the core does: a feed address subscribes, anything else is saved as an article. */
    async addLink(url): Promise<AddedLink> {
      if (isFeedAddress(parseLink(url).href)) {
        const result = await api.subscribe(url);
        if (result.kind === "subscribed") return result;
      }
      return { kind: "saved", value: await api.saveArticle(url) };
    },
    async markEntryRead(entryId, read) {
      getEntry(entryId).read = read;
      feedsChanged();
    },
    async markAllRead(feedId) {
      for (const entry of entries) if (feedId === null || entry.feedId === feedId) entry.read = true;
      feedsChanged();
    },
    /** Feeds that answer were just checked; a failing one still fails. */
    async checkFeeds() {
      const now = Date.now();
      feeds = feeds.map((f) => (f.stage.kind === "failing" ? f : { ...f, lastOkAt: now }));
      feedsChanged();
    },
    /** The preview opens nothing. */
    async openFeedSite(feedId) {
      if (!feeds.some((f) => f.id === feedId)) throw new Error("No such feed.");
    },
    async chooseOpml() {
      return "~/subscriptions.opml";
    },
    async chooseOpmlTarget() {
      return "~/subscriptions.opml";
    },
    /** As the core does for the preview's list: one feed already subscribed (skipped), two
     *  new ones named by the list until their first check. */
    async importOpml() {
      const listed = [
        { title: "Tidewatch", url: "https://tidewatch.example/feed.xml" },
        { title: "Lantern Notes", url: "https://lanterns.example/feed.xml" },
        { title: "Quay Weekly", url: "https://quay.example/feed.xml" },
      ];
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
    async openOriginal(id) {
      if (!articles.has(id)) throw new Error("Not an article.");
    },
    async mergeAudiobook(audiobookId, bookId) {
      if (!isAudiobook(audiobookId)) throw new Error("That is not an audiobook on its own.");
      const book = getBook(bookId),
        audio = getBook(audiobookId);
      if (book.kind !== "book") throw new Error("That is not a book with text.");
      if (recordings.has(bookId)) throw new Error("That book already has a recording.");
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
    async attachRecording(bookId, paths) {
      getBook(bookId);
      const found = await api.inspectRecording(paths);
      if (found.problems.length) throw new Error(`${found.problems[0].name}: ${found.problems[0].problem}`);
      if (!found.files.length) throw new Error("No audio files were found there.");
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
    async recording(bookId) {
      const r = recordings.get(bookId);
      return r ? structuredClone(r) : null;
    },
    async recordings() {
      return [...recordings.values()].map((r) => structuredClone(r));
    },
    async syncReport(bookId): Promise<SyncReportView> {
      const r = recordings.get(bookId);
      if (!r?.summary) throw new Error("the recording is not matched yet");
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
    async syncAction(bookId, action) {
      const r = recordings.get(bookId);
      if (!r) throw new Error("the book has no recording");
      const next: Partial<Record<SyncStage, SyncStage>> =
        action === "pause"
          ? { waiting: "paused", transcribing: "paused", needsModel: "paused" }
          : action === "resume"
            ? { paused: "transcribing" }
            : action === "retry"
              ? { failed: "checking" }
              : action === "accept"
                ? { sampleMismatch: "transcribing", poorMatch: "ready" }
                : {
                    ready: "checking",
                    poorMatch: "checking",
                    sampleMismatch: "checking",
                    failed: "checking",
                    paused: "checking",
                  };
      const stage = next[r.stage];
      if (!stage) return;
      emitSync({
        ...r,
        stage,
        accepted: action === "accept" || r.accepted,
        transcribedMs: action === "resync" ? 0 : r.transcribedMs,
        summary: action === "resync" ? null : r.summary,
      });
      changed();
      if (stage === "transcribing" || stage === "checking") simulateSync(bookId);
    },
    async removeRecording(bookId) {
      recordings.delete(bookId);
      const book = books.find((b) => b.id === bookId);
      if (book) book.recording = null;
      if (settings.has(bookId)) settings.set(bookId, { ...settingsOf(bookId), narration: null });
      changed();
    },
    async setNarration(bookId, narration) {
      const recording = recordings.get(bookId);
      if (narration === "recording" && !recording?.listenable) throw new Error("the recording is not synced yet");
      await api.saveBookSettings(bookId, { ...settingsOf(bookId), narration });
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
    async downloadModel(modelId) {
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
    async chooseAudio(folder) {
      return folder ? ["~/Audiobooks/The Lantern Keeper"] : ["~/Audiobooks/the-lantern-keeper.m4b"];
    },
    async playRecordingAt(bookId, atMs) {
      if (!recordings.get(bookId)?.narrates)
        throw new Error("choose the recording as the book's narration to listen to it");
      const list = chapters[Number(bookId.split("-")[1])] ?? [];
      await api.play(bookId, Math.min(list.length - 1, Math.floor(atMs / hour)), null);
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
    async setSiteRule(host, fetch, siteData) {
      const key = siteHost(host);
      if (!key) throw new Error("No site was named.");
      sites.set(key, {
        ...(sites.get(key) ?? { host: key, session: { kind: "none" }, waitingArticle: null }),
        fetch,
        siteData,
      });
    },
    async signOutSite(host) {
      browserOff();
      const site = sites.get(siteHost(host));
      if (site && (site.session.kind === "signedIn" || site.session.kind === "checkPassed"))
        site.session = { kind: "none" };
    },
    async forgetSite(host) {
      browserOff();
      sites.delete(siteHost(host));
    },
    async startSignIn(id) {
      browserOff();
      if (signInOpen()) throw new Error("A sign-in is already open. Finish or cancel it first.");
      openSignIn(id);
      setTimeout(() => tellSignIn({ stage: { kind: "waiting" } }), 200);
      return structuredClone(signIn!);
    },
    async signIn() {
      return signIn && structuredClone(signIn);
    },
    async finishSignIn() {
      if (signIn?.stage.kind !== "waiting") throw new Error("No sign-in is open.");
      fetchSignedIn();
    },
    async cancelSignIn() {
      if (!signInOpen()) throw new Error("No sign-in is open.");
      if (signIn!.stage.kind !== "fetching") tellSignIn({ stage: { kind: "cancelled" } });
    },
    async showSignInWindow() {
      if (!signInOpen()) throw new Error("No sign-in is open.");
    },
    async staySignedIn(stay) {
      if (!signInOpen()) throw new Error("No sign-in is open.");
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
      if (appSettings.browser.use === "off") throw new Error("The browser is off. Turn it on in Settings › Sites.");
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
        throw new Error("The browser is still downloading. Cancel it first.");
      browserDownloaded = false;
      browserDownload = null;
    },
    onProblem: (h) => subscribe(problemHandlers, h),
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
    getBook(id).prepare = { state: "waiting", done: 0, total: 0, position: 0, chapters: null };
    settlePrepare();
  };
  /** Gives a book a recording in a stage (and, when ready, the recording as its narration). */
  const setRecordingForTest = (
    id: string,
    stage: SyncStage,
    narration: "speech" | "recording" | null = stage === "ready" ? "recording" : null,
  ) => {
    settings.set(id, { ...settingsOf(id), narration });
    // As the core would report it: a ready recording chosen as the narration is what plays.
    if (narration === "recording" && stage === "ready") getBook(id).audio = "recording";
    emitSync(recordingIn(id, stage));
    changed();
  };
  /** Home as the core would make it for another library (an empty one, say). */
  const setHomeForTest = (view: HomeView) => {
    home = structuredClone(view);
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
      clearInterval(syncTimer);
    },
    emitPlayer,
    emitGeneration,
    emitConfig,
    emitProblem: (p: ProblemView) => problemHandlers.forEach((h) => h({ ...p })),
    /** A status from the core; `wanted` defaults to true (the user has asked to listen). */
    emitEngine: (s: Omit<EngineStatusView, "wanted"> & { wanted?: boolean }) =>
      engineHandlers.forEach((h) => h({ wanted: true, ...s })),
  });
}
