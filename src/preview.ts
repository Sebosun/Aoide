import { useNavigationStore, type SettingsTab, type View } from "@/stores/navigation";
import { usePlayerStore } from "@/stores/player";
import { useReaderStore } from "@/stores/reader";
import { useLibraryStore, type LibraryFilter } from "@/stores/library";
import { api } from "@/api";
import { useRecordingsStore } from "@/stores/recordings";
import { showFinished, showInboxAdded, showSavedLink } from "@/opening";
import fixtureArticle from "@/tests/fixtures/article.json";

/** Browser preview only: `?view=home|library|feeds|activity|item|reader|find|audiobooks&book=book-0&play=1500&theme=dark&mode=lines&zoom=1.25` opens a
 *  screen in a known state (used for screenshots). `play` plays for that many ms, then pauses;
 *  `tools=notes|search|details` opens book tools on that tab; `settings=engine` opens settings on that tab; `click=<selector>|<selector>` clicks those
 *  elements in turn once the screen is up (to show an open menu or dialog). `zoom=1.25` sets the
 *  interface size and `delay=800` slows every request (see api/mock and api/index).
 *  `prepare=book-1,book-0` queues those books for whole-book preparation, in that order.
 *  `leftout=0,3` leaves those chapters of `book` out of the narration.
 *  `recording=syncing|ready|review|missing|model|mismatch|failed` gives book-0 a recording in that
 *  stage (see api/mock); `pair=~/a.m4b,~/b.aax` opens the pairing dialog on those paths and
 *  `report=book-0` that book's sync report; `audiobook=1` adds the preview's audiobook on its own
 *  (book-5); `feeds=empty|failing` has no subscriptions, or one that stopped answering;
 *  `timeleft=book` shows the time left in the book rather than the chapter;
 *  `font=sans|legible|system`, `spacing=0..4`, `width=0..4`, `justify=1` and `hyphenate=1` set the
 *  reader's typography;
 *  `server=starting|running|stopped|failed` shows the speech server Aoide starts in that state (as the
 *  core made it; `settings=engine` shows it in Settings);
 *  `browser=off|missing|ready` sets the browser for articles (off, on with none found, on with
 *  one found; `settings=sites` shows it); `signin=waiting|fetching` opens a sign-in on the
 *  preview's article; `shortcuts=1` shows the list of keyboard shortcuts (the ? key);
 *  `pronounce=1` fills the pronunciation lists (book-1's and the one for all books), and
 *  `highlight=1` highlights two passages of `book`'s first chapter, the first with a note, and
 *  bookmarks a sentence of it;
 *  `select=Ren` selects that word in the chapter text once the screen is up (before any `click`);
 *  `saved=new|again` shows the toast for a page sent from the browser (new to the library, with
 *  Undo, or one the library had);
 *  `added=one|several` shows the toast for books the inbox folder added; `inbox=unavailable`
 *  chooses an inbox folder that is gone (`settings=library` shows it);
 *  `archived=book-1,book-3` archives those items (`filter=archive` shows the library's Archive);
 *  `queue=book-1,book-3` lines those items up to listen to next, in that order;
 *  `finished=1` shows the toast for an article just finished, offering Archive;
 *  `sleep=again|again-chapter` has a 15-minute (or end-of-chapter) sleep timer just rung, so the
 *  first play starts it again (the pill says "again"). */
export async function applyPreviewParams(search = location.search) {
  const query = new URLSearchParams(search);
  const view = query.get("view") as View | null,
    settings = query.get("settings") as SettingsTab | null;
  // Before any screen opens, so its lists show them.
  if (query.has("pronounce")) {
    await api.savePronunciation({ bookId: "book-1", written: "Ren", spoken: "wren", matchCase: false });
    await api.savePronunciation({ bookId: null, written: "SQL", spoken: "sequel", matchCase: true });
    await api.savePronunciation({ bookId: null, written: "Dr. Xu", spoken: "doctor shoe", matchCase: false });
  }
  if (query.has("highlight")) {
    const id = query.get("book") ?? "book-0";
    const chapter = await api.chapter({ bookId: id, chapter: 0 });
    const sentences = chapter.blocks.flatMap((b) => b.segments).filter((s) => s.speakable && s.words.length > 4);
    for (const [n, s] of sentences.slice(0, 2).entries()) {
      const last = n === 0 ? s.words.length - 1 : 3;
      await api.addHighlight({
        bookId: id,
        chapter: 0,
        start: s.starts[0],
        end: s.starts[last] + Array.from(s.words[last]).length,
      });
    }
    const [first] = await api.highlights({ bookId: id });
    if (first) await api.updateHighlightNote({ id: first.id, note: "Where the story begins." });
    if (sentences[2]) await api.addBookmark({ bookId: id, note: "", segmentId: sentences[2].id });
  }
  const leftOut = query.get("leftout");
  if (leftOut !== null)
    await api.setLeftOut({
      bookId: query.get("book") ?? "book-0",
      chapters: leftOut.split(",").filter(Boolean).map(Number),
    });
  const saved = query.get("saved");
  if (saved)
    showSavedLink(api, {
      id: fixtureArticle.view.id,
      title: saved === "new" ? new URL(fixtureArticle.article.canonicalUrl).hostname : fixtureArticle.summary.title,
      newToLibrary: saved === "new",
    });
  if (query.has("finished")) showFinished(api, { id: fixtureArticle.view.id, title: fixtureArticle.summary.title });
  const added = query.get("added");
  if (added)
    showInboxAdded(
      ["The Lantern Keeper", "A Map of Quiet Places", "The Absent Orchard"]
        .slice(0, added === "one" ? 1 : 3)
        .map((title, n) => ({ id: `book-${n}`, title })),
    );
  for (const id of query.get("archived")?.split(",").filter(Boolean) ?? []) await api.archive({ bookId: id, on: true });
  for (const id of query.get("queue")?.split(",").filter(Boolean) ?? [])
    await api.queueAdd({ bookId: id, place: "last" });
  const filter = query.get("filter");
  if (filter) {
    await useLibraryStore().start();
    useLibraryStore().filter = filter as LibraryFilter;
  }
  if (settings) useNavigationStore().openSettings(settings);
  if (query.has("shortcuts")) useNavigationStore().openShortcuts();
  for (const id of query.get("prepare")?.split(",").filter(Boolean) ?? []) await api.prepareBook({ bookId: id });
  if (query.has("audiobook")) await api.addAudiobook({ paths: ["~/Audiobooks/Winter Radio"] });
  if (query.has("shelves")) {
    // Sample collections for screenshots: `shelves=1`.
    const library = useLibraryStore();
    await library.start();
    const night = await library.createCollection("Night reading");
    await library.createCollection("Short stories");
    await library.setBookCollections("book-0", [night]);
    await library.setBookCollections("book-1", [night]);
    // The preview's article too, when it is saved (`article=ready`).
    if (library.books.some((b) => b.id === "book-6")) await library.setBookCollections("book-6", [night]);
    if (query.get("shelves") === "open") library.filter = `collection:${night}`;
  }
  const pair = query.get("pair");
  if (pair) useRecordingsStore().openPairing(pair.split(","), query.get("book"));
  const report = query.get("report");
  if (report) {
    await useRecordingsStore().start();
    useRecordingsStore().openReport(report);
  }
  if (!view) return;
  const reader = useReaderStore(),
    player = usePlayerStore(),
    navigation = useNavigationStore();
  const book = query.get("book") ?? "book-0";
  if (view === "reader" || query.has("play")) await reader.open(book);
  if (query.has("play")) {
    await player.play(book, Number(query.get("chapter") ?? 0), null);
    const ms = Number(query.get("play"));
    if (ms > 0)
      setTimeout(() => {
        void player.pause();
      }, ms);
  }
  if (view === "item") navigation.openItem(book);
  else navigation.view = view;
  const tools = query.get("tools");
  if (tools === "notes" || tools === "search" || tools === "details")
    setTimeout(() => useNavigationStore().openTools(tools), 300);
  const select = query.get("select");
  if (select) setTimeout(() => selectWord(select), 500);
  const clicks = query.get("click")?.split("|").filter(Boolean) ?? [];
  clicks.forEach((selector, i) =>
    setTimeout(() => document.querySelector<HTMLElement>(selector)?.click(), 600 + i * 300),
  );
}

/** Selects the first `word` in the chapter text, as a reader would by dragging over it. */
function selectWord(word: string) {
  const text = document.querySelector(".chapter-text");
  if (!text) return;
  const walker = document.createTreeWalker(text, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const at = node.textContent?.indexOf(word) ?? -1;
    if (at < 0) continue;
    const range = document.createRange();
    range.setStart(node, at);
    range.setEnd(node, at + word.length);
    document.getSelection()?.removeAllRanges();
    document.getSelection()?.addRange(range);
    return;
  }
}
