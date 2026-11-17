import type { AoideApi } from "@/api/types";
import { listenForProblems } from "@/problems";
import { useNavigationStore } from "@/stores/navigation";

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

/** Listens for the core's problems, then for items to open: taking what the core held while
 *  the window started also tells the books it could not open meanwhile, as problems. */
export async function listenToCore(api: AoideApi) {
  const offProblems = await listenForProblems(api);
  const offOpening = await listenForOpenItems(api);
  return () => {
    offProblems();
    offOpening();
  };
}
