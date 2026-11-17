import type { Collection } from "@/api/types";
import { useLibraryStore } from "@/stores/library";
import { useUiStore } from "@/stores/ui";

/** Collection actions with their dialogs; failures (such as a duplicate name) become toasts. */
export function useCollections() {
  const library = useLibraryStore(),
    ui = useUiStore();
  async function attempt<T>(action: () => Promise<T>) {
    try {
      return await action();
    } catch (e) {
      ui.toast(String(e), "error");
      return null;
    }
  }
  /** Asks for a name and creates the collection. */
  async function create() {
    const name = await ui.prompt({
      title: "New collection",
      label: "Name",
      placeholder: "Night reading",
      confirmLabel: "Create",
    });
    if (!name) return null;
    return attempt(() => library.createCollection(name));
  }
  /** Puts item `id` on `collection` (a card dropped on it), saying so. */
  async function add(id: string, collection: Collection) {
    const book = library.books.find((b) => b.id === id);
    if (!book) return;
    if (book.collections.includes(collection.id)) {
      ui.toast(`Already in ${collection.name}`, "info");
      return;
    }
    const done = await attempt(async () => {
      await library.setBookCollections(id, [...book.collections, collection.id]);
      return true;
    });
    if (done) ui.toast(`Added to ${collection.name}`);
  }
  async function rename(collection: Collection) {
    const name = await ui.prompt({
      title: "Rename collection",
      label: "Name",
      value: collection.name,
      confirmLabel: "Rename",
    });
    if (name && name !== collection.name) await attempt(() => library.renameCollection(collection.id, name));
  }
  async function remove(collection: Collection) {
    const ok = await ui.confirm({
      title: `Delete the collection “${collection.name}”?`,
      body: "The books stay in your library.",
      confirmLabel: "Delete collection",
      danger: true,
    });
    if (ok) await attempt(() => library.deleteCollection(collection.id));
  }
  return {
    add,
    create,
    rename,
    remove,
  };
}
