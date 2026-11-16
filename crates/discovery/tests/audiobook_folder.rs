//! Suggesting audiobooks from a chosen folder whose names come close to a book's, against
//! temporary folders.
use std::fs;
use std::path::{Path, PathBuf};

use aoide_discovery::AudiobookFolder;

/// A file of `bytes` that takes no room (sparse): sizes are all a search reads.
fn sized(root: &Path, relative: &str, bytes: u64) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::File::create(&path).unwrap().set_len(bytes).unwrap();
    path
}

const MIB: u64 = 1024 * 1024;

/// An audio file the size of an audiobook's.
fn touch(root: &Path, relative: &str) -> PathBuf {
    sized(root, relative, 8 * MIB)
}

/// The suggestions for a book, each as its paths relative to `root`.
fn suggested(root: &Path, epub: &str, title: Option<&str>, authors: &[&str]) -> Vec<Vec<String>> {
    let authors: Vec<String> = authors.iter().map(|a| a.to_string()).collect();
    AudiobookFolder::read(root)
        .suggest(Path::new(epub), title, &authors)
        .iter()
        .map(|s| {
            s.paths
                .iter()
                .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().into_owned())
                .collect()
        })
        .collect()
}

#[test]
fn a_folder_named_like_the_book_is_suggested() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, "Wren Hollis - The Salt Road (Unabridged)/01.mp3");
    touch(root, "Wren Hollis - The Salt Road (Unabridged)/02.mp3");
    touch(root, "Glass Harbour/01.mp3");
    assert_eq!(
        suggested(
            root,
            "/books/salt_road.epub",
            Some("The Salt Road"),
            &["Wren Hollis"]
        ),
        vec![vec!["Wren Hollis - The Salt Road (Unabridged)"]]
    );
}

#[test]
fn close_names_are_suggested_closest_first_and_at_most_three() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, "The Salt Road Returns/01.mp3");
    touch(root, "Salt Road Returns Again/01.mp3");
    touch(root, "Salt Road Returns Again Home/01.mp3");
    touch(root, "Salt Road Returns Again Home Soon/01.mp3");
    touch(root, "Road Trip Songs/01.mp3");
    assert_eq!(
        suggested(root, "/books/salt-road.epub", Some("The Salt Road"), &[]),
        vec![
            vec!["The Salt Road Returns"],
            vec!["Salt Road Returns Again"],
            vec!["Salt Road Returns Again Home"]
        ]
    );
}

#[test]
fn names_with_exactly_the_books_words_leave_out_the_close_ones() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, "The Salt Road/01.mp3");
    touch(root, "Salt Road/01.mp3");
    touch(root, "The Salt Road Returns/01.mp3");
    let authors: &[&str] = &[];
    assert_eq!(
        suggested(
            root,
            "/books/salt-road.epub",
            Some("The Salt Road"),
            authors
        ),
        vec![vec!["Salt Road"], vec!["The Salt Road"]]
    );
}

#[test]
fn a_misspelt_long_word_still_comes_close() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, "The Lanturn Keeper.m4b");
    assert_eq!(
        suggested(root, "/books/x.epub", Some("The Lantern Keeper"), &[]),
        vec![vec!["The Lanturn Keeper.m4b"]]
    );
}

#[test]
fn files_sharing_a_name_among_other_books_are_one_audiobook() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, "Salt Road - Part 1.mp3");
    touch(root, "Salt Road - Part 2.mp3");
    touch(root, "Glass Harbour.m4b");
    touch(root, "Salt Road.epub");
    assert_eq!(
        suggested(root, "/books/x.epub", Some("The Salt Road"), &[]),
        vec![vec!["Salt Road - Part 1.mp3", "Salt Road - Part 2.mp3"]]
    );
}

#[test]
fn a_folder_of_discs_is_one_audiobook() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, "Wren Hollis/The Salt Road/CD1/01.mp3");
    touch(root, "Wren Hollis/The Salt Road/CD2/01.mp3");
    assert_eq!(
        suggested(
            root,
            "/books/x.epub",
            Some("The Salt Road"),
            &["Wren Hollis"]
        ),
        vec![vec!["Wren Hollis/The Salt Road"]]
    );
}

#[test]
fn the_authors_folder_breaks_a_tie() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, "Ada Moss/The Salt Road/01.mp3");
    touch(root, "Wren Hollis/The Salt Road/01.mp3");
    assert_eq!(
        suggested(
            root,
            "/books/x.epub",
            Some("The Salt Road"),
            &["Wren Hollis"]
        ),
        vec![
            vec!["Wren Hollis/The Salt Road"],
            vec!["Ada Moss/The Salt Road"]
        ]
    );
}

#[test]
fn a_missing_folder_suggests_nothing() {
    assert!(
        AudiobookFolder::read(Path::new("/nonexistent/aoide-audiobooks"))
            .suggest(Path::new("/books/x.epub"), Some("The Salt Road"), &[])
            .is_empty()
    );
}

#[test]
fn sharing_only_a_series_name_or_small_words_is_not_close() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, "Ember Watch, Book 2 - Lampmaker.m4b");
    touch(root, "Lessons/005. Sqlite Out of the Box.mp4");
    touch(root, "The Lantern Fair.m4b");
    touch(root, "Lessons/006. Refactoring Migrations and Methods.mp4");
    for (epub, title) in [
        ("/books/x.epub", "Method and Madness"),
        ("/books/Ember Watch 12 - Tide Oath.epub", "Tide Oath"),
        ("/books/silent.epub", "Out of the Silent Harbour"),
        ("/books/x.epub", "The Lantern Keeper"),
    ] {
        assert_eq!(
            suggested(root, epub, Some(title), &[]),
            Vec::<Vec<String>>::new(),
            "{title}"
        );
    }
}

#[test]
fn several_folders_are_searched_skipping_hidden_ones_and_extra_folders_inside_a_book() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, "Music/The Salt Road/01.mp3");
    touch(root, "Music/The Salt Road/covers/front.jpg");
    touch(root, "Spoken/Glass Harbour/01.mp3");
    touch(root, "Spoken/.trash/Glass Harbour Two/01.mp3");
    let options = aoide_discovery::ScanOptions {
        roots: vec![root.join("Music"), root.join("Spoken")],
        ..Default::default()
    };
    let folder = AudiobookFolder::scan(
        &options,
        &std::sync::atomic::AtomicBool::new(false),
        &|_| {},
    );
    let paths = |title: &str| -> Vec<Vec<PathBuf>> {
        folder
            .suggest(Path::new("/books/x.epub"), Some(title), &[])
            .into_iter()
            .map(|s| s.paths)
            .collect()
    };
    assert_eq!(
        paths("The Salt Road"),
        vec![vec![root.join("Music/The Salt Road")]]
    );
    assert_eq!(
        paths("Glass Harbour"),
        vec![vec![root.join("Spoken/Glass Harbour")]]
    );
}

#[test]
fn a_suggestion_holds_every_audio_file_of_its_audiobook_and_their_size() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for (file, bytes) in [
        ("The Salt Road/CD1/01 - The Salt Road.m4b", 3 * MIB),
        ("The Salt Road/CD1/02 - The Salt Road.m4b", 4 * MIB),
        ("The Salt Road/CD2/01 - The Salt Road.m4b", 5 * MIB),
        ("The Salt Road/CD2/cover.jpg", 100),
    ] {
        sized(root, file, bytes);
    }
    let found =
        AudiobookFolder::read(root).suggest(Path::new("/books/x.epub"), Some("The Salt Road"), &[]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].paths, vec![root.join("The Salt Road")]);
    let files: Vec<String> = found[0]
        .files
        .iter()
        .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        files,
        vec![
            "The Salt Road/CD1/01 - The Salt Road.m4b",
            "The Salt Road/CD1/02 - The Salt Road.m4b",
            "The Salt Road/CD2/01 - The Salt Road.m4b",
        ]
    );
    assert_eq!(found[0].bytes, 12 * MIB);
}

#[test]
fn among_several_books_an_audiobook_goes_only_to_the_books_it_comes_closest_to() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, "The Salt Road.m4b");
    touch(root, "Glass Harbour/01.mp3");
    let authors: Vec<String> = Vec::new();
    let books = [
        (
            Path::new("/books/a.epub"),
            Some("The Salt Road"),
            &authors[..],
        ),
        (
            Path::new("/books/b.epub"),
            Some("The Salt Road Home"),
            &authors[..],
        ),
        (
            Path::new("/books/c.epub"),
            Some("The Salt Road"),
            &authors[..],
        ),
        (
            Path::new("/books/d.epub"),
            Some("Glass Harbour"),
            &authors[..],
        ),
    ];
    let names: Vec<Vec<String>> = AudiobookFolder::read(root)
        .suggest_each(books)
        .iter()
        .map(|found| {
            found
                .iter()
                .map(|s| {
                    s.paths[0]
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        })
        .collect();
    assert_eq!(
        names,
        vec![
            vec!["The Salt Road.m4b".to_owned()],
            vec![],
            vec!["The Salt Road.m4b".to_owned()],
            vec!["Glass Harbour".to_owned()],
        ],
        "a closer book takes it from a merely close one; equally close books both keep it"
    );
    assert_eq!(
        suggested(root, "/books/b.epub", Some("The Salt Road Home"), &[]),
        vec![vec!["The Salt Road.m4b".to_owned()]],
        "alone, the close book is offered it"
    );
}

#[test]
fn every_audiobook_found_is_listed_once_by_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, "Glass Harbour/01.mp3");
    touch(root, "Glass Harbour/02.mp3");
    // A folder of one book's audio is that book, however many files it holds.
    touch(root, "Loose/Night Ferry.m4b");
    let all: Vec<(Vec<String>, usize)> = AudiobookFolder::read(root)
        .all()
        .iter()
        .map(|s| {
            let paths = s
                .paths
                .iter()
                .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().into_owned())
                .collect();
            (paths, s.files.len())
        })
        .collect();
    assert_eq!(
        all,
        [
            (vec!["Glass Harbour".to_string()], 2),
            (vec!["Loose".to_string()], 1)
        ]
    );
}

#[test]
fn short_sounds_are_not_audiobooks_but_many_short_chapters_are_one() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // A game's or an app's sounds: a few kilobytes each.
    sized(root, ".local/share/Some Game/sfx/door.ogg", 4 * 1024);
    sized(root, "Games/Some Game/sfx/door.ogg", 4 * 1024);
    sized(root, "Games/Some Game/sfx/step.ogg", 4 * 1024);
    sized(root, "Notifications/Ding Dong.mp3", 60 * 1024);
    // A book split into many short chapters counts by its whole size.
    for chapter in 1..=40 {
        sized(
            root,
            &format!("The Night Ferry/{chapter:02}.mp3"),
            200 * 1024,
        );
    }
    let all: Vec<PathBuf> = AudiobookFolder::read(root)
        .all()
        .into_iter()
        .flat_map(|s| s.paths)
        .collect();
    assert_eq!(all, vec![root.join("The Night Ferry")]);
}
