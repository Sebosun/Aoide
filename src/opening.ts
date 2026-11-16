import type { AoideApi } from "@/api/types";
import { useNavigationStore } from "@/stores/navigation";

/** Opens the item the core asks for ("item://open": a desktop notification about it was
 *  clicked), closing the Add dialog. */
export function listenForOpenItems(api: AoideApi) {
  return api.onOpenItem((id) => {
    const navigation = useNavigationStore();
    navigation.closeAdd();
    navigation.openItem(id);
  });
}
