//! Finding a book's audiobook beside its EPUB, against temporary folders.
use std::fs;
use std::path::{Path, PathBuf};

use aoide_discovery::audiobook_beside;

fn touch(root: &Path, relative: &str) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"x").unwrap();
    path
}

/// What was found, relative to `root`.
fn found(root: &Path, epub: &Path, title: Option<&str>, authors: &[&str]) -> Vec<String> {
    let authors: Vec<String> = authors.iter().map(|a| a.to_string()).collect();
    audiobook_beside(epub, title, &authors)
        .iter()
        .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn an_audio_file_named_like_the_book_is_found() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let epub = touch(root, "Books/The Salt Road.epub");
    touch(root, "Books/The Salt Road.m4b");
    touch(root, "Books/Other Story.m4b");
    touch(root, "Books/Other Story.epub");
    assert_eq!(
        found(root, &epub, None, &[]),
        vec!["Books/The Salt Road.m4b"]
    );
}

#[test]
fn names_match_as_words_without_author_numbers_brackets_or_noise() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let epub = touch(root, "Books/salt_road_v2.epub");
    touch(
        root,
        "Books/Wren Hollis - The Salt Road (Unabridged) - Part 01.mp3",
    );
    touch(
        root,
        "Books/Wren Hollis - The Salt Road (Unabridged) - Part 02.mp3",
    );
    touch(root, "Books/The Salt Road Returns.mp3");
    assert_eq!(
        found(root, &epub, Some("The Salt Road"), &["Wren Hollis"]),
        vec![
            "Books/Wren Hollis - The Salt Road (Unabridged) - Part 01.mp3",
            "Books/Wren Hollis - The Salt Road (Unabridged) - Part 02.mp3",
        ]
    );
}

#[test]
fn a_folder_named_like_the_book_beside_the_epub_is_found() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let epub = touch(root, "Books/The Salt Road.epub");
    touch(root, "Books/The Salt Road Returns/01.mp3");
    touch(root, "Books/The Salt Road [Audiobook]/CD1/01.mp3");
    touch(root, "Books/The Salt Road (notes)/notes.txt");
    assert_eq!(
        found(root, &epub, None, &[]),
        vec!["Books/The Salt Road [Audiobook]"]
    );
}

#[test]
fn the_epubs_own_folder_counts_when_it_names_the_book_and_holds_no_other_epub() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let epub = touch(root, "Wren Hollis - The Salt Road/book.epub");
    touch(root, "Wren Hollis - The Salt Road/01 Opening.mp3");
    touch(root, "Wren Hollis - The Salt Road/02 Harbour.mp3");
    assert_eq!(
        found(root, &epub, Some("The Salt Road"), &["Wren Hollis"]),
        vec!["Wren Hollis - The Salt Road"]
    );

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let shared = touch(root, "The Salt Road/a.epub");
    touch(root, "The Salt Road/b.epub");
    touch(root, "The Salt Road/01.mp3");
    assert!(found(root, &shared, Some("The Salt Road"), &[]).is_empty());
}

#[test]
fn a_folder_named_like_the_book_beside_the_epubs_folder_is_found() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let epub = touch(root, "Salt Road/ebook/The Salt Road.epub");
    touch(root, "Salt Road/The Salt Road - audio/01.m4a");
    assert_eq!(
        found(root, &epub, Some("The Salt Road"), &[]),
        vec!["Salt Road/The Salt Road - audio"]
    );
}

#[test]
fn nothing_is_found_when_no_name_matches() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let epub = touch(root, "Downloads/The Salt Road.epub");
    touch(root, "Downloads/song.mp3");
    touch(root, "Downloads/Podcasts/episode.mp3");
    touch(root, "Wren Hollis/01.mp3");
    assert!(found(root, &epub, Some("The Salt Road"), &["Wren Hollis"]).is_empty());
}

#[test]
fn audio_named_like_the_book_is_found_in_folders_beside_the_epubs_folder() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let epub = touch(
        root,
        "Downloads/Salt_Road-Wren_Hollis/The Salt Road - Wren Hollis.epub",
    );
    touch(
        root,
        "Downloads/Wren Hollis - The Salt Road Notes from a Long Walk/The Salt Road.m4b",
    );
    touch(
        root,
        "Downloads/Wren Hollis - The Salt Road Notes from a Long Walk/cover.jpg",
    );
    assert_eq!(
        found(root, &epub, Some("The Salt Road"), &["Wren Hollis"]),
        vec!["Downloads/Wren Hollis - The Salt Road Notes from a Long Walk/The Salt Road.m4b"]
    );
}

#[test]
fn a_series_kept_apart_from_its_epubs_finds_each_books_audio() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let second = touch(
        root,
        "Shelf/LanternEpubs/Lantern's Keepers 02 - Tidewright - Ines Varo.epub",
    );
    let first = touch(
        root,
        "Shelf/LanternEpubs/Lantern's Keepers 01 - First Light - Ines Varo.epub",
    );
    touch(
        root,
        "Shelf/Ines Varo - Lanterns Keeper, Book 2 - Tidewright.m4b",
    );
    touch(
        root,
        "Shelf/Ines Varo - Lanterns Keeper, Book 3 - Saltmarsh.m4b",
    );
    touch(
        root,
        "Shelf/Ines Varo - First Light Lantern Keeper Saga, Book 1/01 - First Light.m4b",
    );
    touch(
        root,
        "Shelf/Ines Varo - First Light Lantern Keeper Saga, Book 1/02 - First Light.m4b",
    );
    assert_eq!(
        found(root, &second, None, &["Ines Varo"]),
        vec!["Shelf/Ines Varo - Lanterns Keeper, Book 2 - Tidewright.m4b"]
    );
    assert_eq!(
        found(root, &first, Some("First Light"), &["Ines Varo"]),
        vec![
            "Shelf/Ines Varo - First Light Lantern Keeper Saga, Book 1/01 - First Light.m4b",
            "Shelf/Ines Varo - First Light Lantern Keeper Saga, Book 1/02 - First Light.m4b",
        ]
    );
}
