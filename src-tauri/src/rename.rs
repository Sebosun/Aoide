//! Aoide used to be called Folio. On the first start after the rename, Folio's folders move to
//! Aoide's names, and the paths stored in the library follow them.

use std::path::{Path, PathBuf};

/// The app identifier Folio used for its data and cache folders.
pub const OLD_IDENTIFIER: &str = "com.folio.audiobooks";
/// Folio's settings folder name (`~/.config/folio`).
pub const OLD_CONFIG: &str = "folio";
/// Folio's folder in Documents (`~/Documents/Folio/Books`).
pub const OLD_DOCUMENTS: &str = "Folio";

/// Moves each `(old, new)` folder that exists, then rewrites the library's stored paths inside
/// the moved folders. A folder already at the new place gets only the old entries it lacks;
/// nothing is overwritten. Returns a line per folder moved.
pub fn move_folders(moves: &[(PathBuf, PathBuf)], db_path: &Path) -> Vec<String> {
    let mut report = Vec::new();
    let mut moved = Vec::new();
    for (from, to) in moves {
        match move_folder(from, to) {
            Ok(true) => {
                report.push(format!("moved {} to {}", from.display(), to.display()));
                moved.push((from, to));
            }
            Ok(false) => {}
            Err(error) => report.push(format!("could not move {}: {error}", from.display())),
        }
    }
    if !moved.is_empty() && db_path.exists() {
        match aoide_library::Library::open(db_path) {
            Ok(library) => {
                for (from, to) in moved {
                    if let Err(error) = library.move_paths(from, to) {
                        report.push(format!(
                            "could not update paths under {}: {error}",
                            from.display()
                        ));
                    }
                }
            }
            Err(error) => report.push(format!(
                "could not open the library to update paths: {error}"
            )),
        }
    }
    report
}

/// Returns whether anything moved.
fn move_folder(from: &Path, to: &Path) -> std::io::Result<bool> {
    if !from.is_dir() {
        return Ok(false);
    }
    if !to.exists() {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(from, to)?;
        return Ok(true);
    }
    let mut any = false;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if !target.exists() {
            std::fs::rename(entry.path(), target)?;
            any = true;
        }
    }
    let _ = std::fs::remove_dir(from); // Only succeeds once everything moved.
    Ok(any)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn moves_old_folders_to_their_new_names() {
        let root = tempfile::tempdir().unwrap();
        let (old, new) = (
            root.path().join("config/folio"),
            root.path().join("config/aoide"),
        );
        write(&old.join("settings.toml"), "[engine]");
        let report = move_folders(
            &[(old.clone(), new.clone())],
            &root.path().join("none.sqlite3"),
        );
        assert_eq!(report.len(), 1, "{report:?}");
        assert!(!old.exists());
        assert_eq!(
            std::fs::read_to_string(new.join("settings.toml")).unwrap(),
            "[engine]"
        );
    }

    #[test]
    fn fills_a_new_folder_that_already_exists_without_overwriting_it() {
        let root = tempfile::tempdir().unwrap();
        let (old, new) = (root.path().join("old"), root.path().join("new"));
        write(&old.join("library.sqlite3"), "old library");
        write(&old.join("covers/a.jpg"), "cover");
        write(&new.join("library.sqlite3"), "kept");
        move_folders(
            &[(old.clone(), new.clone())],
            &root.path().join("none.sqlite3"),
        );
        assert_eq!(
            std::fs::read_to_string(new.join("library.sqlite3")).unwrap(),
            "kept"
        );
        assert_eq!(
            std::fs::read_to_string(new.join("covers/a.jpg")).unwrap(),
            "cover"
        );
        // What could not move stays where it was.
        assert_eq!(
            std::fs::read_to_string(old.join("library.sqlite3")).unwrap(),
            "old library"
        );
    }

    #[test]
    fn does_nothing_when_there_is_nothing_old() {
        let root = tempfile::tempdir().unwrap();
        let new = root.path().join("new");
        let report = move_folders(
            &[(root.path().join("old"), new.clone())],
            &root.path().join("none.sqlite3"),
        );
        assert!(report.is_empty());
        assert!(!new.exists());
    }

    #[test]
    fn stored_book_and_cover_paths_follow_the_moved_folders() {
        use aoide_library::{BookId, Library, NewBook};
        let root = tempfile::tempdir().unwrap();
        let (old_data, new_data) = (
            root.path().join("share/old.app"),
            root.path().join("share/new.app"),
        );
        let (old_docs, new_docs) = (
            root.path().join("Documents/Folio"),
            root.path().join("Documents/Aoide"),
        );
        write(&old_docs.join("Books/a.epub"), "epub");
        write(&old_data.join("covers/a.jpg"), "cover");
        std::fs::create_dir_all(&old_data).unwrap();
        {
            let library = Library::open(&old_data.join("library.sqlite3")).unwrap();
            library
                .upsert_book(
                    &NewBook {
                        id: BookId("a".into()),
                        path: old_docs.join("Books/a.epub"),
                        title: None,
                        authors: vec![],
                        language: None,
                        cover_path: Some(old_data.join("covers/a.jpg")),
                        error: None,
                    },
                    1,
                )
                .unwrap();
        }
        let moves = [
            (old_data.clone(), new_data.clone()),
            (old_docs.clone(), new_docs.clone()),
        ];
        move_folders(&moves, &new_data.join("library.sqlite3"));

        let library = Library::open(&new_data.join("library.sqlite3")).unwrap();
        let book = library.book(&BookId("a".into())).unwrap().unwrap();
        assert_eq!(book.path, new_docs.join("Books/a.epub"));
        assert_eq!(book.cover_path, Some(new_data.join("covers/a.jpg")));
        assert!(book.path.exists() && book.cover_path.unwrap().exists());
    }
}
