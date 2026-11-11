import { useNavigationStore, type SettingsTab, type View } from "./stores/navigation";
import { usePlayerStore } from "./stores/player";
import { useReaderStore } from "./stores/reader";
import { useLibraryStore } from "./stores/library";
import { api } from "./api";

/** Browser preview only: `?view=reader&book=book-0&play=1500&theme=dark&mode=lines&zoom=1.25` opens a
 *  screen in a known state (used for screenshots). `play` plays for that many ms, then pauses;
 *  `tools=voice` opens book tools on that tab; `settings=engine` opens settings on that tab; `click=<selector>|<selector>` clicks those
 *  elements in turn once the screen is up (to show an open menu or dialog). `zoom=1.25` sets the
 *  interface size and `delay=800` slows every request (see api/mock and api/index).
 *  `prepare=book-1,book-0` queues those books for whole-book preparation, in that order. */
export async function applyPreviewParams(search = location.search) {
  const query = new URLSearchParams(search);
  const view = query.get("view") as View | null, settings = query.get("settings") as SettingsTab | null;
  if (settings) useNavigationStore().openSettings(settings);
  for (const id of query.get("prepare")?.split(",").filter(Boolean) ?? []) await api.prepareBook(id);
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
  if (!view) return;
  const reader = useReaderStore(), player = usePlayerStore(), navigation = useNavigationStore();
  const book = query.get("book") ?? "book-0";
  if (view === "reader" || query.has("play")) await reader.open(book);
  if (query.has("play")) {
    await player.play(book, Number(query.get("chapter") ?? 0), null);
    const ms = Number(query.get("play"));
    if (ms > 0) setTimeout(() => { void player.pause(); }, ms);
  }
  if (view === "book") navigation.openBook(book);
  else navigation.view = view;
  const tools = query.get("tools");
  if (tools) setTimeout(() => {
    document.querySelector<HTMLElement>('[aria-label="Player"] [aria-label="Book tools"]')?.click();
    setTimeout(() => document.querySelector<HTMLElement>(`#book-tools-tab-${tools}`)?.click(), 100);
  }, 300);
  const clicks = query.get("click")?.split("|").filter(Boolean) ?? [];
  clicks.forEach((selector, i) => setTimeout(() => document.querySelector<HTMLElement>(selector)?.click(), 600 + i * 300));
}
