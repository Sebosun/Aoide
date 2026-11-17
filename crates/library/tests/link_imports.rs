//! Schema v25: an import of links from another app, and its items in order.
use aoide_library::{BookId, ItemKind, Library, NewBook};

fn item(lib: &Library, id: &str) -> BookId {
    let id = BookId(id.into());
    lib.upsert_book(
        &NewBook {
            id: id.clone(),
            kind: ItemKind::Article,
            path: format!("{}.html", id.0).into(),
            title: None,
            authors: vec![],
            language: None,
            cover_path: None,
            error: None,
        },
        1,
    )
    .unwrap();
    id
}

#[test]
fn an_import_keeps_its_items_in_order_and_its_pause() {
    let lib = Library::open_in_memory().unwrap();
    assert_eq!(lib.current_link_import().unwrap(), None);
    let import = lib.start_link_import("pocket", 5).unwrap();
    let (b, a) = (item(&lib, "b"), item(&lib, "a"));
    lib.add_link_import_items(import, &[b.clone(), a.clone(), b.clone()])
        .unwrap();
    assert_eq!(
        lib.link_import_items(import).unwrap(),
        [b.clone(), a.clone()]
    );
    let current = lib.current_link_import().unwrap().unwrap();
    assert_eq!(
        (current.id, current.source.as_str(), current.paused),
        (import, "pocket", false)
    );
    lib.set_link_import_paused(import, true).unwrap();
    assert!(lib.current_link_import().unwrap().unwrap().paused);
    // A removed item leaves the import.
    lib.remove_book(&b).unwrap();
    assert_eq!(lib.link_import_items(import).unwrap(), [a]);
    lib.finish_link_import(import, 9).unwrap();
    assert_eq!(lib.current_link_import().unwrap(), None);
}

#[test]
fn a_book_is_added_to_one_collection_keeping_the_others() {
    let lib = Library::open_in_memory().unwrap();
    let book = item(&lib, "a");
    let (sea, quay) = (
        lib.create_collection("Sea", 1).unwrap(),
        lib.create_collection("Quay", 1).unwrap(),
    );
    lib.set_book_collections(&book, &[sea]).unwrap();
    lib.add_to_collection(&book, quay).unwrap();
    lib.add_to_collection(&book, quay).unwrap();
    let mut ids = lib.book_collections().unwrap().remove(&book).unwrap();
    ids.sort();
    let mut want = vec![sea, quay];
    want.sort();
    assert_eq!(ids, want);
}
