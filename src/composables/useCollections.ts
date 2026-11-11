import type { BookSummary, Collection } from "../api/types";
import { useLibraryStore } from "../stores/library";
import { useUiStore } from "../stores/ui";

/** Collection actions with their dialogs; failures (such as a duplicate name) become toasts. */
export function useCollections() {
  const library = useLibraryStore(), ui = useUiStore();
  async function attempt<T>(action: () => Promise<T>) {
    try { return await action(); } catch (e) { ui.toast(String(e), "error"); return null; }
  }
  /** Asks for a name and creates the collection; with a book, puts the book on it too. */
  async function create(book?: BookSummary) {
    const name = await ui.prompt({ title: "New collection", label: "Name", placeholder: "Night reading", confirmLabel: "Create" });
    if (!name) return null;
    return attempt(async () => {
      const id = await library.createCollection(name);
      if (book) await library.setBookCollections(book.id, [...book.collections, id]);
      return id;
    });
  }
  async function rename(collection: Collection) {
    const name = await ui.prompt({ title: "Rename collection", label: "Name", value: collection.name, confirmLabel: "Rename" });
    if (name && name !== collection.name) await attempt(() => library.renameCollection(collection.id, name));
  }
  async function remove(collection: Collection) {
    const ok = await ui.confirm({ title: `Delete the collection “${collection.name}”?`, body: "The books stay in your library.", confirmLabel: "Delete collection", danger: true });
    if (ok) await attempt(() => library.deleteCollection(collection.id));
  }
  async function toggle(book: BookSummary, id: number) {
    const ids = book.collections.includes(id) ? book.collections.filter(c => c !== id) : [...book.collections, id];
    await attempt(() => library.setBookCollections(book.id, ids));
  }
  return { create, rename, remove, toggle };
}
