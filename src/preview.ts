import { useNavigationStore, type SettingsTab, type View } from "@/stores/navigation";
import { usePlayerStore } from "@/stores/player";
import { useReaderStore } from "@/stores/reader";
import { useLibraryStore } from "@/stores/library";
import { api } from "@/api";
import { useRecordingsStore } from "@/stores/recordings";

/** Browser preview only: `?view=home|library|feeds|activity|item|reader|find|audiobooks&book=book-0&play=1500&theme=dark&mode=lines&zoom=1.25` opens a
 *  screen in a known state (used for screenshots). `play` plays for that many ms, then pauses;
 *  `tools=voice` opens book tools on that tab; `settings=engine` opens settings on that tab; `click=<selector>|<selector>` clicks those
 *  elements in turn once the screen is up (to show an open menu or dialog). `zoom=1.25` sets the
 *  interface size and `delay=800` slows every request (see api/mock and api/index).
 *  `prepare=book-1,book-0` queues those books for whole-book preparation, in that order.
 *  `recording=syncing|ready|review|missing|model|mismatch|failed` gives book-0 a recording in that
 *  stage (see api/mock); `pair=~/a.m4b,~/b.aax` opens the pairing dialog on those paths and
 *  `report=book-0` that book's sync report; `audiobook=1` adds the preview's audiobook on its own
 *  (book-5); `feeds=empty|failing` has no subscriptions, or one that stopped answering;
 *  `browser=off|missing|ready` sets the browser for articles (off, on with none found, on with
 *  one found; `settings=sites` shows it); `signin=waiting|fetching` opens a sign-in on the
 *  preview's article */
export async function applyPreviewParams(search = location.search) {
  const query = new URLSearchParams(search);
  const view = query.get("view") as View | null,
    settings = query.get("settings") as SettingsTab | null;
  if (settings) useNavigationStore().openSettings(settings);
  for (const id of query.get("prepare")?.split(",").filter(Boolean) ?? []) await api.prepareBook(id);
  if (query.has("audiobook")) await api.addAudiobook(["~/Audiobooks/Winter Radio"]);
  if (query.has("shelves")) {
    // Sample collections for screenshots: `shelves=1`.
    const library = useLibraryStore();
    await library.start();
    const night = await library.createCollection("Night reading");
    await library.createCollection("Short stories");
    await library.setBookCollections("book-0", [night]);
    await library.setBookCollections("book-1", [night]);
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
  if (tools)
    setTimeout(() => {
      document.querySelector<HTMLElement>('[aria-label="Player"] [aria-label="Book tools"]')?.click();
      setTimeout(() => document.querySelector<HTMLElement>(`#book-tools-tab-${tools}`)?.click(), 100);
    }, 300);
  const clicks = query.get("click")?.split("|").filter(Boolean) ?? [];
  clicks.forEach((selector, i) =>
    setTimeout(() => document.querySelector<HTMLElement>(selector)?.click(), 600 + i * 300),
  );
}
