import type { AoideApi, SavedLink } from "@/api/types";
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

/** Listens for the core's problems and the pages it saved, then for items to open: taking what
 *  the core held while the window started also tells the books it could not open and the pages
 *  it saved meanwhile. */
export async function listenToCore(api: AoideApi) {
  const offProblems = await listenForProblems(api);
  const offSaved = await api.onLinkSaved((saved) => showSavedLink(api, saved));
  const offOpening = await listenForOpenItems(api);
  return () => {
    offProblems();
    offSaved();
    offOpening();
  };
}
