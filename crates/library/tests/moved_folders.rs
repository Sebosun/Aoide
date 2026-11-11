//! Stored paths follow a folder that was moved (the app's folders were renamed).
use std::path::{Path, PathBuf};

use aoide_library::{BookId, Library, NewBook};

fn add(lib: &Library, id: &str, path: &str, cover: Option<&str>) {
    lib.upsert_book(
        &NewBook {
            id: BookId(id.into()),
            path: path.into(),
            title: Some("Synthetic book".into()),
            authors: vec![],
            language: None,
            cover_path: cover.map(PathBuf::from),
            error: None,
        },
        1,
    )
    .unwrap();
}

#[test]
fn paths_under_a_moved_folder_are_rewritten_and_others_are_left_alone() {
    let lib = Library::open_in_memory().unwrap();
    add(
        &lib,
        "a",
        "/home/me/Documents/Old/Books/a.epub",
        Some("/home/me/.local/share/old.app/covers/a.jpg"),
    );
    add(&lib, "b", "/home/me/Elsewhere/b.epub", None);
    // A sibling whose name only starts the same is not inside the moved folder.
    add(&lib, "c", "/home/me/Documents/OldStuff/c.epub", None);

    let changed = lib
        .move_paths(
            Path::new("/home/me/Documents/Old"),
            Path::new("/home/me/Documents/New"),
        )
        .unwrap()
        + lib
            .move_paths(
                Path::new("/home/me/.local/share/old.app"),
                Path::new("/home/me/.local/share/new.app"),
            )
            .unwrap();
    assert_eq!(changed, 2);

    let a = lib.book(&BookId("a".into())).unwrap().unwrap();
    assert_eq!(a.path, PathBuf::from("/home/me/Documents/New/Books/a.epub"));
    assert_eq!(
        a.cover_path,
        Some(PathBuf::from("/home/me/.local/share/new.app/covers/a.jpg"))
    );
    assert_eq!(
        lib.book(&BookId("b".into())).unwrap().unwrap().path,
        PathBuf::from("/home/me/Elsewhere/b.epub")
    );
    assert_eq!(
        lib.book(&BookId("c".into())).unwrap().unwrap().path,
        PathBuf::from("/home/me/Documents/OldStuff/c.epub")
    );
}
