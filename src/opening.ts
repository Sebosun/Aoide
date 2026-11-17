import type { AoideApi, InboxBook, SavedLink } from "@/api/types";
import { listenForProblems } from "@/problems";
import { useNavigationStore } from "@/stores/navigation";
import { useUiStore } from "@/stores/ui";

/** Opens the item the core asks for ("item://open": a desktop notification about it was
 *  clicked, or a book was opened with Aoide from the file manager), closing the Add dialog.
 *  Once listening, it takes what the core held while the window started, and opens the latest. */
export async function listenForOpenItems(api: AoideApi) {
  const open = (id: string) => {
    const navigation = useNavigationStore();
    navigation.closeAdd();
    navigation.openItem(id);
  };
  const off = await api.onOpenItem(open);
  try {
    const latest = (await api.takeOpenRequests()).at(-1);
    if (latest) open(latest);
  } catch {
    // Already logged by the API; the listener stands.
  }
  return off;
}

/** Names a page sent from the browser that the core saved; Undo, offered only when the core
 *  says it was new to the library, removes it (an item a feed lists goes back to the feed). */
export function showSavedLink(api: AoideApi, saved: SavedLink) {
  const ui = useUiStore();
  if (!saved.newToLibrary) {
    ui.toast(`“${saved.title}” is in the library already`);
    return;
  }
  ui.toast(`Saved “${saved.title}”`, "info", undefined, {
    label: "Undo",
    run: async () => {
      try {
        await api.removeBook({ bookId: saved.id });
      } catch (e) {
        ui.toast(String(e), "error");
      }
    },
  });
}

/** Names the books the core added from the inbox folder in one look; nothing opens. */
export function showInboxAdded(books: InboxBook[]) {
  const quoted = books.map((book) => `“${book.title}”`);
  const ui = useUiStore();
  if (quoted.length === 1) return ui.toast(`Added ${quoted[0]} from the inbox folder`);
  const names = quoted.length <= 3 ? `: ${quoted.slice(0, -1).join(", ")} and ${quoted.at(-1)}` : "";
  ui.toast(`Added ${quoted.length} books from the inbox folder${names}`);
}

/** Listens for the core's problems and the pages it saved, then for items to open: taking what
 *  the core held while the window started also tells the books it could not open and the pages
 *  it saved meanwhile. */
export async function listenToCore(api: AoideApi) {
  const offProblems = await listenForProblems(api);
  const offSaved = await api.onLinkSaved((saved) => showSavedLink(api, saved));
  const offInbox = await api.onInboxAdded(showInboxAdded);
  const offOpening = await listenForOpenItems(api);
  return () => {
    offProblems();
    offSaved();
    offInbox();
    offOpening();
  };
}
