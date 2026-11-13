import type { RecordingView, SyncStage, InspectionView, ModelDownloadView, SyncReportView, PrepareState, Preferences, Candidate, ScanProgress, DiscoveryImportProgress, AoideApi, BookSummary, BookView, ChapterView, PlayerState, EngineStatusView, GenerationState, EngineDescriptor, EngineProfile, BookSettings, Bookmark, AppSettings, BookDetails, MetadataMatch, ExportProgress, SearchHit } from "./types";

export const initialPlayer = (): PlayerState => ({ bookId: null, status: "stopped", chapter: 0, segmentId: null, wordIndex: null, speed: 1, volume: 1, chapterFraction: 0, error: null, sleep: null, source: "speech", offText: false });
export const initialGeneration = (): GenerationState => ({ bookId: null, generatingSegmentId: null, bufferedAheadMs: 0, pending: 0, error: null, preparing: null });

/** The same API with every request (not the event subscriptions) taking `ms` longer. */
export function slowed<T extends AoideApi>(api: T, ms: number): T {
  if (ms <= 0) return api;
  return new Proxy(api, {
    get(target, key, receiver) {
      const value = Reflect.get(target, key, receiver);
      if (typeof value !== "function" || String(key).startsWith("on")) return value;
      return (...args: unknown[]) => new Promise(resolve => setTimeout(resolve, ms)).then(() => (value as (...a: unknown[]) => unknown).apply(target, args));
    },
  });
}

/** Original, synthetic preview books. No personal books or network requests. */
export function createMockApi() {
  const engine: EngineStatusView = { profileName: "Local Chatterbox", state: "ready", detail: "Browser preview · simulated audio" };
  const books: BookSummary[] = ["The Lantern Keeper", "A Map of Quiet Places", "The Absent Orchard", "An Unfinished Atlas", "The Torn Ledger"].map((title, i) => ({
    id: `book-${i}`, title, authors: [i === 0 ? "Mira Vale" : "Ellis Reed"], language: "en", coverUrl: null,
    status: i === 2 ? { kind: "missing" } : i === 4 ? { kind: "error", message: "This EPUB could not be parsed." } : { kind: "present" },
    progress: i === 0 ? { chapter: 0, segmentId: "0:1:0", fraction: 0.12 } : null,
    audioBytes: i === 0 ? 2457600 : 0, prepare: null, addedAt: 100 + i, lastOpenedAt: i === 0 ? 500 : null, collections: [], recording: null,
  }));
  const shelves: { id: number; name: string; createdAt: number }[] = [];
  const shelfName = (name: string, except?: number) => {
    const trimmed = name.trim();
    if (!trimmed) throw new Error("a collection needs a name");
    if (shelves.some(s => s.id !== except && s.name.toLowerCase() === trimmed.toLowerCase())) throw new Error(`there is already a collection called “${trimmed}”`);
    return trimmed;
  };
  const texts = [
    [
      ["The small light", "Every evening, Ada carried a lantern to the end of the pier. The sea collected its reflection and broke it into a hundred little stars.", "Tonight a paper boat waited beside the steps. Inside it lay a key, warm as if someone had held it all the way across the bay."],
      ["Across the water", "At dawn she borrowed the blue rowing boat. Each pull of the oars opened a quiet doorway in the water.", "On the far shore, a gardener waved. He had been waiting for someone to bring back the light."],
    ],
    [
      ["An ordinary discovery", "No one had marked the bench on any map. It stood between a bakery and a clock shop, where the morning smelled of bread and rain.", "Ren sat down and listened. Beneath the traffic, a sparrow was rehearsing a song with no ending."],
      ["The long way home", "Ren drew a circle where the bench should be. Then came the bridge, the cool stone steps, and the lane where the wind always rested.", "The map did not make the city smaller. It made room for all the things Ren had hurried past."],
      ["A place to begin", "The next morning, a stranger asked for directions. Ren folded the map and offered to walk beside her.", "They took the long way. There was plenty of time."],
    ],
  ];
  const chapters: ChapterView[][] = texts.map(book => book.map((parts, c) => ({ index: c, title: parts[0], blocks: parts.map((text, b) => ({ kind: b === 0 ? "heading" : "paragraph", level: b === 0 ? 2 : null, segments: [{ id: `${c}:${b}:0`, words: text.split(/\s+/), speakable: true, note: false, notes: c === 0 && b === 1 ? [{ afterWord: 2, label: "1", text: "A short original note shown in the browser preview." }] : [] }] })) })));
  const descriptors: EngineDescriptor[] = [
    { kind: "chatterbox", name: "Chatterbox TTS Server", adapterVersion: 1, languages: ["en", "fr", "de"], wordTimings: false, maxChars: 300, params: [
      { key: "temperature", label: "Temperature", kind: { type: "float", min: 0.05, max: 2, step: 0.05 }, default: 0.8 },
      { key: "exaggeration", label: "Exaggeration", kind: { type: "float", min: 0.25, max: 2, step: 0.05 }, default: 0.5 },
      { key: "seed", label: "Seed (0 = random)", kind: { type: "int", min: 0, max: 4294967295 }, default: 0 },
    ] },
    { kind: "openai-speech", name: "OpenAI-compatible speech server", adapterVersion: 1, languages: null, wordTimings: false, maxChars: 1000, params: [
      { key: "speed", label: "Speed", kind: { type: "float", min: 0.25, max: 4, step: 0.05 }, default: 1 },
    ] },
  ];
  let profile: EngineProfile = { id: "default", name: "Local Chatterbox", kind: "chatterbox", baseUrl: "http://127.0.0.1:8004", model: null, network: "loopback", voiceId: "Emily.wav", params: {}, launch: null };
  const settings = new Map<string, BookSettings>();
  const bookmarks: Bookmark[] = [];
  let nextBookmark = 1;
  const listened = new Map<string, number>([["book-0", 5_400_000], ["book-1", 1_200_000]]);
  let appSettings: AppSettings = { metadataLookup: true, cacheLimitBytes: null, aligner: null, librarySort: "recent", scan: { roots: [], exclude: [], includeHidden: [], onStartup: false }, resumePrepare: true, recognition: { modelSize: "base", device: "auto", threads: 0, resumeOnStart: true } };
  const found: Candidate[] = [
    { id: "h-tide", status: "new", title: "Notes on the Tide", authors: ["Ines Morrow"], language: "en", isbn: "9780000000101", coverUrl: null, paths: ["~/Downloads/notes-on-the-tide.epub", "~/Documents/Books/tide.epub"], size: 2_400_000, error: null, suggestion: null },
    { id: "h-glass", status: "new", title: "The Glass Meridian", authors: ["Oren Vale"], language: "en", isbn: null, coverUrl: null, paths: ["~/Downloads/glass-meridian.epub"], size: 1_100_000, error: null, suggestion: null },
    { id: "book-0", status: "inLibrary", title: "The Lantern Keeper", authors: ["Mira Vale"], language: "en", isbn: null, coverUrl: null, paths: ["~/Documents/lantern.epub"], size: 900_000, error: null, suggestion: null },
    { id: "path:~/Documents/broken.epub", status: "unreadable", title: "broken", authors: [], language: null, isbn: null, coverUrl: null, paths: ["~/Documents/broken.epub"], size: 10, error: "not a readable EPUB", suggestion: null },
  ];
  const query = typeof location === "undefined" ? new URLSearchParams() : new URLSearchParams(location.search);
  const mode = query.get("theme");
  let preferences: Preferences = {
    appearance: { mode: mode === "light" || mode === "dark" ? mode : "auto", theme: "default", textSize: 2, readerMode: query.get("mode") === "lines" ? "lines" : "page", zoom: Number(query.get("zoom")) || 1 },
    themes: [{ name: "dusk", light: { accent: "#B4472B" }, dark: { accent: "#FFB27A", ground: "#120E1C" }, problems: [] }],
    configDir: "~/.config/aoide",
    problems: [],
  };
  /** Recordings paired with books, like the app (core/recording.rs): one per book, each with an
   *  explicit sync stage. `?recording=syncing|ready|review|missing|model|mismatch|failed` gives
   *  book-0 a recording in that stage. */
  const recordings = new Map<string, RecordingView>();
  const syncHandlers = new Set<(r: RecordingView) => void>();
  const downloadHandlers = new Set<(d: ModelDownloadView) => void>();
  const downloaded = new Set<string>(["base.en"]);
  const hour = 3_600_000;
  const recordingIn = (bookId: string, stage: SyncStage): RecordingView => {
    const totalMs = 6 * hour + 1_234_000, matched = stage === "ready" || stage === "poorMatch";
    return {
      bookId, stage, error: stage === "failed" ? "the recording could not be read" : null,
      files: [{ path: "~/Audiobooks/the-lantern-keeper.m4b", name: "the-lantern-keeper.m4b", durationMs: totalMs, codec: "aac" }],
      totalMs, chapters: 12,
      transcribedMs: matched ? totalMs : stage === "transcribing" ? Math.round(totalMs * 0.37) : 0,
      remainingMs: stage === "transcribing" ? 14 * 60_000 : null,
      position: stage === "transcribing" || stage === "waiting" ? 0 : null,
      summary: matched ? { words: 64_210, matchedWords: stage === "ready" ? 60_400 : 38_100, timedWords: stage === "ready" ? 63_900 : 44_000 } : null,
      accepted: false, modelId: "base.en",
    };
  };
  const emitSync = (recording: RecordingView) => {
    recordings.set(recording.bookId, recording);
    const book = books.find(b => b.id === recording.bookId);
    if (book) book.recording = recording.stage;
    syncHandlers.forEach(h => h(structuredClone(recording)));
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
        emitSync(transcribedMs >= r.totalMs ? { ...r, transcribedMs, stage: "matching", remainingMs: null, position: null } : { ...r, transcribedMs, remainingMs });
      } else if (r.stage === "matching") {
        clearInterval(syncTimer);
        emitSync({ ...recordingIn(bookId, "ready"), files: r.files, totalMs: r.totalMs });
        const current = settings.get(bookId);
        if (!current?.narration) settings.set(bookId, { voiceId: null, params: {}, language: null, readNotes: false, ...current, narration: "recording" });
        changed();
      }
    }, 400);
  };
  const previewStage = ({ syncing: "transcribing", ready: "ready", review: "poorMatch", missing: "missing", model: "needsModel", mismatch: "sampleMismatch", failed: "failed" } as Record<string, SyncStage>)[query.get("recording") ?? ""];
  if (previewStage) {
    recordings.set("book-0", recordingIn("book-0", previewStage));
    books[0].recording = previewStage;
    if (previewStage === "ready") settings.set("book-0", { voiceId: null, params: {}, language: null, readNotes: false, narration: "recording" });
  }
  const configHandlers = new Set<(p: Preferences) => void>();
  const emitConfig = (next: Partial<Omit<Preferences, "appearance">> & { appearance?: Partial<Preferences["appearance"]> }) => {
    preferences = { ...preferences, ...next, appearance: { ...preferences.appearance, ...next.appearance } };
    configHandlers.forEach(h => h(structuredClone(preferences)));
  };
  let scanned = false;
  const scanHandlers = new Set<(p: ScanProgress) => void>();
  const detailHandlers = new Set<(id: string) => void>();
  const importHandlers = new Set<(p: DiscoveryImportProgress) => void>();
  const details = new Map<string, BookDetails>();
  const exportHandlers = new Set<(p: ExportProgress) => void>();
  const playerHandlers = new Set<(s: PlayerState) => void>();
  const engineHandlers = new Set<(s: EngineStatusView) => void>();
  const generationHandlers = new Set<(s: GenerationState) => void>();
  const libraryHandlers = new Set<() => void>();
  let state = initialPlayer();
  let timer: ReturnType<typeof setInterval> | undefined;
  const emitPlayer = (next: PlayerState) => { state = { ...next }; playerHandlers.forEach(h => h({ ...state })); };
  const emitGeneration = (next: GenerationState) => generationHandlers.forEach(h => h({ ...next }));
  const changed = () => libraryHandlers.forEach(h => h());
  /** Whole-book preparation, like the app (core/preparation.rs): each queued book has an explicit
   *  stage; the mock plans a book as soon as it is queued (books from `queueWaitingForTest` wait). */
  type PrepareEvent = "planned" | "pause" | "resume" | "promote" | "demote";
  const nextStage = (stage: PrepareState, event: PrepareEvent): PrepareState =>
    stage === "waiting" && event === "planned" ? "queued"
    : (stage === "waiting" || stage === "queued" || stage === "preparing") && event === "pause" ? "paused"
    : stage === "paused" && event === "resume" ? "waiting"
    : stage === "queued" && event === "promote" ? "preparing"
    : stage === "preparing" && event === "demote" ? "queued"
    : stage;
  const prepareQueue: { id: string; stage: PrepareState; chapters: number[] | null }[] = [];
  const prepareEntry = (id: string) => prepareQueue.find(e => e.id === id);
  /** The first planned book is prepared; the others wait behind it. Then publishes the stages. */
  function settlePrepare() {
    let first = true;
    for (const entry of prepareQueue) {
      if (entry.stage === "queued" && first) entry.stage = nextStage(entry.stage, "promote");
      else if (entry.stage === "preparing" && !first) entry.stage = nextStage(entry.stage, "demote");
      if (entry.stage === "preparing") first = false;
    }
    prepareQueue.forEach((entry, position) => {
      const b = books.find(b => b.id === entry.id);
      if (b?.prepare) b.prepare = { ...b.prepare, position, state: entry.stage, chapters: entry.chapters };
    });
    changed();
  }
  const unused = { files: 3, bytes: 12 * 1024 ** 2 };
  const getBook = (id: string) => { const b = books.find(b => b.id === id); if (!b || b.status.kind !== "present") throw new Error("Book is unavailable."); return b; };
  const getChapter = (id: string, chapter: number) => { getBook(id); const result = chapters[Number(id.split("-")[1])]?.[chapter]; if (!result) throw new Error("Chapter not found."); return structuredClone(result); };
  const halt = () => { clearInterval(timer); timer = undefined; };
  const tick = () => {
    if (state.status !== "playing" || !state.bookId) return;
    const chapter = getChapter(state.bookId, state.chapter);
    const segments = chapter.blocks.flatMap(b => b.segments).filter(s => s.speakable);
    let index = segments.findIndex(s => s.id === state.segmentId);
    let word = (state.wordIndex ?? -1) + 1;
    if (word >= segments[index].words.length) { index++; word = 0; }
    if (index >= segments.length) {
      const next = chapters[Number(state.bookId.split("-")[1])][state.chapter + 1];
      if (!next) { halt(); emitPlayer({ ...state, status: "stopped", chapterFraction: 1 }); return; }
      emitPlayer({ ...state, chapter: next.index, segmentId: next.blocks[0].segments[0].id, wordIndex: 0, chapterFraction: 0 });
    } else emitPlayer({ ...state, segmentId: segments[index].id, wordIndex: word, chapterFraction: index / segments.length });
    const book = getBook(state.bookId!);
    book.progress = { chapter: state.chapter, segmentId: state.segmentId, fraction: (state.chapter + state.chapterFraction) / chapters[Number(book.id.split("-")[1])].length };
    changed();
  };
  const run = () => { halt(); timer = setInterval(tick, 320 / state.speed); };
  const subscribe = async <T>(set: Set<(s: T) => void>, handler: (s: T) => void) => { set.add(handler); return () => { set.delete(handler); }; };
  const api: AoideApi = {
    async appInfo() { return { libraryFolder: "~/Documents/Aoide/Books" }; },
    async listBooks() { return structuredClone(books); },
    async importFiles(paths) { return paths.map(path => ({ path, bookId: null, error: "Browser preview cannot read local paths. Add EPUBs in the desktop app." })); },
    async rescanLibrary() { changed(); },
    async collections() {
      return shelves.map(s => ({ ...s, books: books.filter(b => b.collections.includes(s.id)).length })).sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
    },
    async createCollection(name) { const id = (shelves.at(-1)?.id ?? 0) + 1; shelves.push({ id, name: shelfName(name), createdAt: Date.now() }); changed(); return id; },
    async renameCollection(id, name) { const shelf = shelves.find(s => s.id === id); if (!shelf) throw new Error("no such collection"); shelf.name = shelfName(name, id); changed(); },
    async deleteCollection(id) { const at = shelves.findIndex(s => s.id === id); if (at >= 0) shelves.splice(at, 1); books.forEach(b => { b.collections = b.collections.filter(c => c !== id); }); changed(); },
    async setBookCollections(id, ids) {
      if (ids.some(c => !shelves.some(s => s.id === c))) throw new Error("no such collection");
      getBook(id).collections = shelves.filter(s => ids.includes(s.id)).sort((a, b) => a.name.localeCompare(b.name)).map(s => s.id); changed();
    },
    async removeBook(id) { if (state.bookId === id) await api.stop(); const index = books.findIndex(b => b.id === id); if (index >= 0) books.splice(index, 1); changed(); },
    async showInFolder(id) { getBook(id); },
    async openBook(id): Promise<BookView> { const b = getBook(id); b.lastOpenedAt = Date.now(); return api.bookOverview(id); },
    async bookOverview(id): Promise<BookView> {
      const b = getBook(id), list = chapters[Number(id.split("-")[1])];
      const wordCount = list.flatMap(c => c.blocks).flatMap(b => b.segments).reduce((n, s) => n + s.words.length, 0);
      const notes = id === "book-1";
      const voice = settings.get(id)?.voiceId, offered = (await api.engineVoices()).map(v => v.id);
      const voiceWarning = voice && !offered.includes(voice) ? `${voice.replace(".wav", "")} is not one of Mock engine's voices, so Emily reads this book. Choose a voice in the book's settings.` : null;
      return { ...structuredClone(b), chapters: list.map(({ index, title }) => ({ index, title })), warnings: notes ? ["A table in “The long way home” was read as plain text."] : [], excluded: notes ? [{ title: "Copyright", reason: "front matter" }] : [], languageWarning: null, voiceWarning, wordCount };
    },
    async chapter(id, chapter) { return getChapter(id, chapter); },
    async play(id, chapter, segmentId) {
      const book = getBook(id); const c = chapter ?? book.progress?.chapter ?? 0;
      const segments = getChapter(id, c).blocks.flatMap(b => b.segments).filter(s => s.speakable);
      const segment = segments.find(s => s.id === (segmentId ?? (chapter === null ? book.progress?.segmentId : null))) ?? segments[0];
      const source = settings.get(id)?.narration === "recording" && recordings.get(id)?.stage === "ready" ? "recording" : "speech";
      emitPlayer({ ...state, bookId: id, chapter: c, segmentId: segment.id, wordIndex: 0, status: "playing", chapterFraction: segments.indexOf(segment) / segments.length, error: null, source, offText: false });
      emitGeneration({ ...initialGeneration(), bookId: id, bufferedAheadMs: 18000 }); run();
    },
    async pause() { halt(); emitPlayer({ ...state, status: "paused" }); },
    async resume() { if (state.bookId) { emitPlayer({ ...state, status: "playing" }); run(); } },
    async stop() { halt(); emitPlayer({ ...state, status: "stopped", wordIndex: null }); },
    async setSpeed(speed) { emitPlayer({ ...state, speed }); if (state.status === "playing") run(); },
    async setVolume(volume) { emitPlayer({ ...state, volume: Math.min(1, Math.max(0, volume)) }); },
    async engineStatus() { return { ...engine }; },
    async engineOptions() { return { descriptors: structuredClone(descriptors), profile: structuredClone(profile) }; },
    async previewVoice() {
      // A 10 ms silent 16-bit mono WAV.
      const samples = 240, view = new DataView(new ArrayBuffer(44 + samples * 2));
      const text = (at: number, value: string) => [...value].forEach((c, i) => view.setUint8(at + i, c.charCodeAt(0)));
      text(0, "RIFF"); view.setUint32(4, 36 + samples * 2, true); text(8, "WAVE"); text(12, "fmt ");
      view.setUint32(16, 16, true); view.setUint16(20, 1, true); view.setUint16(22, 1, true); view.setUint32(24, 24_000, true);
      view.setUint32(28, 48_000, true); view.setUint16(32, 2, true); view.setUint16(34, 16, true); text(36, "data"); view.setUint32(40, samples * 2, true);
      return view.buffer;
    },
    async engineVoices() { return ["Emily.wav", "Adrian.wav", "Olivia.wav"].map(id => ({ id, displayName: id.replace(".wav", "") })); },
    async saveEngineProfile(next) {
      if (!descriptors.some(d => d.kind === next.kind)) throw new Error(`No adapter for engine kind \`${next.kind}\``);
      if (next.network === "loopback" && !/^https?:\/\/(127\.|localhost|\[::1\])/.test(next.baseUrl)) throw new Error("Only loopback addresses are allowed unless home-network access is on.");
      profile = JSON.parse(JSON.stringify(next)); engine.profileName = next.name;
    },
    async bookSettings(id) { return structuredClone(settings.get(id) ?? { voiceId: null, params: {}, language: null, readNotes: false, narration: null }); },
    async saveBookSettings(id, next) { getBook(id); settings.set(id, JSON.parse(JSON.stringify(next))); },
    async prepareBook(id) {
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
      if (entry) { if (entry.chapters && !entry.chapters.includes(chapter)) entry.chapters = [...entry.chapters, chapter].sort((x, y) => x - y); settlePrepare(); return; }
      const added = prepareQueue[prepareQueue.push({ id, stage: "waiting", chapters: [chapter] }) - 1];
      added.stage = nextStage(added.stage, "planned");
      b.prepare = { state: added.stage, done: 0, total: 12, position: 0, chapters: [chapter] };
      settlePrepare();
    },
    async chapterAudio(id) {
      getBook(id);
      const list = chapters[Number(id.split("-")[1])], queued = prepareEntry(id);
      return list.map((_, c) => {
        const total = 12;
        if (id === "book-0" && c === 0) return { done: total, total };
        return { done: queued && (queued.chapters ?? [c]).includes(c) ? 3 : 0, total };
      });
    },
    async cancelPrepare(id) { const b = books.find(b => b.id === id); if (b) b.prepare = null; const at = prepareQueue.findIndex(e => e.id === id); if (at >= 0) prepareQueue.splice(at, 1); settlePrepare(); },
    async regenerateBook(id) { getBook(id).audioBytes = 0; changed(); },
    async storageReport() {
      const audio = books.filter(b => b.audioBytes > 0).sort((a, b) => b.audioBytes - a.audioBytes);
      return {
        audioBytes: audio.reduce((n, b) => n + b.audioBytes, 0), coversBytes: 1_400_000, booksBytes: 3_600_000, databaseBytes: 240_000,
        unusedFiles: unused.files, unusedBytes: unused.bytes,
        books: audio.map(b => ({ bookId: b.id, bytes: b.audioBytes, segments: Math.ceil(b.audioBytes / 40_000), newestAt: 1_000, inUse: state.bookId === b.id && state.status !== "stopped" })),
      };
    },
    async cleanUnusedFiles() { const freed = { ...unused }; unused.files = 0; unused.bytes = 0; return freed; },
    async setSleepTimer(minutes, endOfChapter) { emitPlayer({ ...state, sleep: minutes === null && !endOfChapter ? null : { remainingMs: minutes === null ? null : minutes * 60_000, endOfChapter } }); },
    async searchBook(id, query) {
      getBook(id); const q = query.trim().toLowerCase(); if (q.length < 2) return [];
      const hits: SearchHit[] = [];
      chapters[Number(id.split("-")[1])].forEach(chapter => chapter.blocks.forEach(block => block.segments.forEach(segment => {
        const text = segment.words.join(" "); const at = text.toLowerCase().indexOf(q);
        if (at >= 0) hits.push({ chapter: chapter.index, chapterTitle: chapter.title, segmentId: segment.id, snippet: text, matchStart: at, matchEnd: at + q.length });
      })));
      return hits;
    },
    async skipChapter(delta) { if (state.bookId) { const list = chapters[Number(state.bookId.split("-")[1])]; const c = Math.min(list.length - 1, Math.max(0, state.chapter + delta)); await api.play(state.bookId, c, list[c].blocks[0].segments[0].id); } },
    async bookmarks(id) { return structuredClone(bookmarks.filter(b => b.bookId === id).sort((a, b) => a.chapter - b.chapter || a.charOffset - b.charOffset || a.createdAt - b.createdAt)); },
    async addBookmark(id, note) {
      const book = getBook(id); const playing = state.bookId === id;
      const chapter = playing ? state.chapter : book.progress?.chapter ?? 0; const segmentId = (playing ? state.segmentId : book.progress?.segmentId) ?? null;
      const segment = getChapter(id, chapter).blocks.flatMap(b => b.segments).find(s => s.id === segmentId);
      const mark: Bookmark = { id: nextBookmark++, bookId: id, chapter, charOffset: Number(segmentId?.split(":")[1] ?? 0) * 100, segmentId, excerpt: segment ? segment.words.slice(0, 12).join(" ") : "", note, createdAt: Math.floor(Date.now() / 1000) };
      bookmarks.push(mark); return structuredClone(mark);
    },
    async updateBookmarkNote(id, note) { const mark = bookmarks.find(b => b.id === id); if (!mark) throw new Error("Bookmark not found."); mark.note = note; },
    async deleteBookmark(id) { const index = bookmarks.findIndex(b => b.id === id); if (index >= 0) bookmarks.splice(index, 1); },
    async bookListening(id) {
      getBook(id);
      return id === "book-0" ? [{ day: "2026-11-02", ms: 1_500_000 }, { day: "2026-11-08", ms: 2_400_000 }, { day: "2026-11-10", ms: 1_500_000 }] : [];
    },
    async listeningStats() {
      const byBook = [...listened].map(([bookId, ms]) => ({ bookId, title: books.find(b => b.id === bookId)?.title ?? "Removed book", ms })).sort((a, b) => b.ms - a.ms);
      return { totalMs: byBook.reduce((t, b) => t + b.ms, 0), byBook, byDay: [{ day: "2026-11-08", ms: 2_400_000 }, { day: "2026-11-09", ms: 3_000_000 }, { day: "2026-11-10", ms: 1_200_000 }] };
    },
    async bookDetails(id) { getBook(id); return structuredClone(details.get(id) ?? { description: "An original story used in the browser preview.", publisher: null, isbn: null, published: null, subjects: [], openLibraryKey: null }); },
    async lookupMetadata(id, _refresh): Promise<MetadataMatch[]> {
      const book = getBook(id); if (!appSettings.metadataLookup) throw new Error("Online metadata lookup is turned off in Settings.");
      return [{ key: "/works/OL0001W", title: book.title, authors: book.authors, published: "2019", publisher: "Preview Press", isbn: null, subjects: ["Fiction"], coverUrl: null }];
    },
    async applyMetadata(id, match) {
      getBook(id); const next: BookDetails = { description: details.get(id)?.description ?? null, publisher: match.publisher, isbn: match.isbn, published: match.published, subjects: match.subjects, openLibraryKey: match.key };
      details.set(id, next); changed(); return structuredClone(next);
    },
    async appSettings() { return { ...appSettings }; },
    async saveAppSettings(next) { appSettings = JSON.parse(JSON.stringify(next)); },
    async preferences() { return structuredClone(preferences); },
    async saveAppearance(next) { preferences = { ...preferences, appearance: { ...next } }; },
    onConfigChanged: h => subscribe(configHandlers, h),
    async exportBook(id) {
      getBook(id); const path = `~/Documents/Aoide/Exports/${books.find(b => b.id === id)!.title}.m4b`;
      exportHandlers.forEach(h => h({ bookId: id, stage: "done", done: 1, total: 1, path, error: null })); return path;
    },
    async scanForBooks() {
      scanHandlers.forEach(h => h({ stage: "walking", dirs: 1200, files: 48000, found: 3, read: 0, toRead: 0, error: null }));
      scanned = true;
      scanHandlers.forEach(h => h({ stage: "done", dirs: 0, files: 0, found: 4, read: 2, toRead: 2, error: null }));
      return structuredClone(found);
    },
    async cancelScan() {},
    async discoveredBooks() { return scanned ? structuredClone(found) : []; },
    async fetchDiscoveredDetails(ids) {
      for (const id of ids) {
        const c = found.find(f => f.id === id);
        if (c && c.status === "new") c.suggestion = { key: `/works/${id}`, title: c.title, authors: c.authors, published: "2018", publisher: "Preview Press", isbn: c.isbn, subjects: ["Fiction"], coverUrl: null };
        detailHandlers.forEach(h => h(id));
      }
    },
    async importDiscovered(choices) {
      return choices.map((choice, done) => {
        importHandlers.forEach(h => h({ done, total: choices.length, current: found.find(f => f.id === choice.id)?.title ?? null }));
        const c = found.find(f => f.id === choice.id);
        if (!c || c.status === "unreadable") return { path: c?.paths[0] ?? "", bookId: null, error: "not a readable EPUB" };
        c.status = "inLibrary";
        books.push({ id: c.id, title: c.title, authors: c.authors, language: c.language, status: { kind: "present" }, coverUrl: null, progress: null, audioBytes: 0, prepare: null, recording: null, addedAt: 900, lastOpenedAt: null, collections: [] });
        changed();
        return { path: c.paths[0], bookId: c.id, error: null };
      });
    },
    onScanProgress: h => subscribe(scanHandlers, h),
    onDiscoveryDetails: h => subscribe(detailHandlers, h),
    onDiscoveryImport: h => subscribe(importHandlers, h),
    onExportProgress: h => subscribe(exportHandlers, h),
    onPlayerState: h => subscribe(playerHandlers, h), onEngineStatus: h => subscribe(engineHandlers, h), onGenerationState: h => subscribe(generationHandlers, h),
    async onLibraryChanged(h) { libraryHandlers.add(h); return () => { libraryHandlers.delete(h); }; },
    async inspectRecording(paths): Promise<InspectionView> {
      const audio = paths.filter(p => /\.(m4b|m4a|mp3|flac|ogg|opus|wav)$/i.test(p) || !/\.[a-z0-9]+$/i.test(p));
      const protectedFiles = paths.filter(p => /\.(aax|aa)$/i.test(p));
      const name = (p: string) => p.split("/").pop() ?? p;
      return {
        files: audio.map((path, i) => ({ path, name: name(path), durationMs: hour + i * 600_000, codec: path.endsWith(".mp3") ? "mp3" : "aac" })),
        problems: protectedFiles.map(path => ({ path, name: name(path), problem: "Protected by Audible DRM. Aoide plays only audio files that play in any player.", drm: true })),
        totalMs: audio.length * hour, chapters: audio.length * 3,
        title: audio.some(p => p.toLowerCase().includes("lantern")) ? "The Lantern Keeper" : null,
        author: audio.some(p => p.toLowerCase().includes("lantern")) ? "Mira Vale" : null,
        suggestedBook: audio.some(p => p.toLowerCase().includes("lantern")) ? "book-0" : null,
      };
    },
    async attachRecording(bookId, paths) {
      getBook(bookId);
      const found = await api.inspectRecording(paths);
      if (found.problems.length) throw new Error(`${found.problems[0].name}: ${found.problems[0].problem}`);
      if (!found.files.length) throw new Error("No audio files were found there.");
      const recording: RecordingView = { ...recordingIn(bookId, "checking"), files: found.files, totalMs: found.totalMs, chapters: found.chapters, transcribedMs: 0, remainingMs: null };
      emitSync(recording); changed(); simulateSync(bookId);
      return structuredClone(recording);
    },
    async recording(bookId) { const r = recordings.get(bookId); return r ? structuredClone(r) : null; },
    async recordings() { return [...recordings.values()].map(r => structuredClone(r)); },
    async syncReport(bookId): Promise<SyncReportView> {
      const r = recordings.get(bookId);
      if (!r?.summary) throw new Error("the recording is not matched yet");
      const list = chapters[Number(bookId.split("-")[1])] ?? [];
      return {
        summary: { ...r.summary },
        chapters: list.map((c, i) => ({ index: i, title: c.title, words: 5000, matchedWords: i === 1 && r.stage === "poorMatch" ? 400 : 4700, timedWords: 4980, startMs: i * hour, endMs: (i + 1) * hour - 2_000 })),
        offText: [
          { startMs: 0, endMs: 31_000, excerpt: "This is a recording of The Lantern Keeper, read for you by a narrator" },
          { startMs: r.totalMs - 28_000, endMs: r.totalMs, excerpt: "That was The Lantern Keeper. Thank you for listening" },
        ],
      };
    },
    async syncAction(bookId, action) {
      const r = recordings.get(bookId);
      if (!r) throw new Error("the book has no recording");
      const next: Partial<Record<SyncStage, SyncStage>> =
        action === "pause" ? { waiting: "paused", transcribing: "paused", needsModel: "paused" }
        : action === "resume" ? { paused: "transcribing" }
        : action === "retry" ? { failed: "checking" }
        : action === "accept" ? { sampleMismatch: "transcribing", poorMatch: "ready" }
        : { ready: "checking", poorMatch: "checking", sampleMismatch: "checking", failed: "checking", paused: "checking" };
      const stage = next[r.stage];
      if (!stage) return;
      emitSync({ ...r, stage, accepted: action === "accept" || r.accepted, transcribedMs: action === "resync" ? 0 : r.transcribedMs, summary: action === "resync" ? null : r.summary });
      changed();
      if (stage === "transcribing" || stage === "checking") simulateSync(bookId);
    },
    async removeRecording(bookId) {
      recordings.delete(bookId);
      const book = books.find(b => b.id === bookId);
      if (book) book.recording = null;
      const current = settings.get(bookId);
      if (current) settings.set(bookId, { ...current, narration: null });
      changed();
    },
    async setNarration(bookId, narration) {
      if (narration === "recording" && recordings.get(bookId)?.stage !== "ready") throw new Error("the recording is not synced yet");
      settings.set(bookId, { voiceId: null, params: {}, language: null, readNotes: false, ...settings.get(bookId), narration });
    },
    async recognitionOptions() {
      const models: [string, "tiny" | "base" | "small", boolean, number][] = [["tiny.en", "tiny", true, 77_704_715], ["tiny", "tiny", false, 77_691_713], ["base.en", "base", true, 147_964_211], ["base", "base", false, 147_951_465], ["small.en", "small", true, 487_614_201], ["small", "small", false, 487_601_967]];
      return {
        settings: structuredClone(appSettings.recognition), gpuBuild: false, modelsFolder: "~/.local/share/com.aoide.audiobooks/models",
        models: models.map(([id, size, englishOnly, bytes]) => ({ id, size, englishOnly, bytes, downloaded: downloaded.has(id) })),
      };
    },
    async downloadModel(modelId) {
      const total = 147_964_211;
      for (const share of [0.25, 0.6, 1]) {
        await new Promise(resolve => setTimeout(resolve, 150));
        downloadHandlers.forEach(h => h({ id: modelId, stage: "downloading", doneBytes: Math.round(total * share), totalBytes: total, error: null }));
      }
      downloaded.add(modelId);
      downloadHandlers.forEach(h => h({ id: modelId, stage: "done", doneBytes: total, totalBytes: total, error: null }));
      for (const r of recordings.values()) if (r.stage === "needsModel") { emitSync({ ...r, stage: "transcribing" }); simulateSync(r.bookId); }
    },
    onSyncState: h => subscribe(syncHandlers, h),
    onModelDownload: h => subscribe(downloadHandlers, h),
  };
  /** Test hooks: put a book at a share of its length, or give it generated audio. */
  const setProgressForTest = (id: string, fraction: number) => { getBook(id).progress = { chapter: 0, segmentId: null, fraction }; changed(); };
  const setAudioForTest = (id: string, bytes: number) => { getBook(id).audioBytes = bytes; changed(); };
  /** A book left in the queue by an earlier run that has not started again (resuming at startup is off). */
  const queueWaitingForTest = (id: string) => { prepareQueue.push({ id, stage: "waiting", chapters: null }); getBook(id).prepare = { state: "waiting", done: 0, total: 0, position: 0, chapters: null }; settlePrepare(); };
  return Object.assign(api, { setProgressForTest, setAudioForTest, queueWaitingForTest, dispose: () => { halt(); clearInterval(syncTimer); }, emitPlayer, emitGeneration, emitConfig, emitEngine: (s: EngineStatusView) => engineHandlers.forEach(h => h({ ...s })) });
}
