//! Suggesting audiobooks from a chosen folder whose names come close to a book's, against
//! temporary folders.
use std::fs;
use std::path::{Path, PathBuf};

use aoide_discovery::{AudioFacts, AudiobookFolder, Verdict};

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

/// Each audiobook found, as its paths relative to `root`.
fn listed(folder: &AudiobookFolder, root: &Path) -> Vec<Vec<String>> {
    folder
        .all()
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
fn numbered_files_in_a_folder_are_one_audiobook_however_their_titles_differ() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // A lecture series: a shared word, then a number and each lecture's own title.
    touch(root, "Gospel of Mark/Mark 00a Intro to the Text.m4a");
    touch(root, "Gospel of Mark/Mark 00b Pronunciation.m4a");
    touch(root, "Gospel of Mark/Mark 16  Latin.m4a");
    // A course: the number first.
    touch(root, "Latin 101/01 Pronouncing Classical Latin.mp3");
    touch(root, "Latin 101/02 Third-Conjugation Verbs.mp3");
    touch(root, "Latin 101/36 Next Steps.mp3");
    // Chapters with an introduction before them.
    touch(root, "Ice Caves/01 Introduction.mp3");
    touch(root, "Ice Caves/02 Chapter 1.mp3");
    touch(root, "Ice Caves/03 Chapter 2.mp3");
    // An author's books, whole and unnumbered, stay apart; numbered ones too when each is
    // the size of a book.
    touch(root, "Ada Penn/The Glass Harbour.m4b");
    touch(root, "Ada Penn/Night Ferry.m4b");
    sized(root, "Salt Series/01 The Salt Road.m4b", 400 * MIB);
    sized(root, "Salt Series/02 The Salt Sea.m4b", 400 * MIB);
    assert_eq!(
        listed(&AudiobookFolder::read(root), root),
        [
            vec!["Ada Penn/Night Ferry.m4b"],
            vec!["Ada Penn/The Glass Harbour.m4b"],
            vec!["Gospel of Mark"],
            vec!["Ice Caves"],
            vec!["Latin 101"],
            vec!["Salt Series/01 The Salt Road.m4b"],
            vec!["Salt Series/02 The Salt Sea.m4b"],
        ]
    );
}

#[test]
fn videos_and_protected_music_are_not_audiobooks() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, "Coding Course/01 Routing.mp4");
    touch(root, "Coding Course/02 Views.mp4");
    touch(root, "Store/Song.m4p");
    touch(root, "Old/Song.wma");
    touch(root, "Shows/Episode 1.MP4");
    assert!(AudiobookFolder::read(root).all().is_empty());
}

/// What the headers of a scripted file say.
fn heard(minutes: u64, channels: u32, genre: Option<&str>, album: Option<&str>) -> AudioFacts {
    AudioFacts {
        duration_ms: Some(minutes * 60_000),
        channels,
        album: album.map(str::to_owned),
        artist: Some("Ada Penn".into()),
        genre: genre.map(str::to_owned),
    }
}

#[test]
fn what_the_files_say_tells_audiobooks_from_music_and_clips() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // A music album: ten four-minute stereo tracks at 320 kbps, its genre Rock.
    for track in 1..=10 {
        sized(
            root,
            &format!("Harbour Lights/{track:02} Song.mp3"),
            9 * MIB,
        );
    }
    // A soundtrack, its tracks untagged.
    sized(root, "Night Show/Soundtrack/Big Dumb Ape.mp3", 4 * MIB);
    sized(root, "Night Show/Soundtrack/Blue Moon.mp3", 4 * MIB);
    // A clip, three minutes long.
    sized(root, "Clips/Speech Excerpt.mp3", 4 * MIB);
    // A long stereo MP3 at 64 kbps: a memoir.
    sized(root, "My Own Time - A Memoir.mp3", 170 * MIB);
    // An M4B, whatever else it says.
    sized(root, "The Salt Road.m4b", 60 * MIB);
    // Fifty minutes of stereo at 128 kbps, nothing else known: may be either.
    sized(root, "Talks/Evening Talk.mp3", 46 * MIB);
    let mut folder = AudiobookFolder::read(root);
    let report = folder.classify(&|path: &Path| {
        let name = path.file_name()?.to_string_lossy().into_owned();
        Some(match name.as_str() {
            n if n.ends_with("Song.mp3") => heard(4, 2, Some("Rock"), Some("Harbour Lights")),
            "Big Dumb Ape.mp3" | "Blue Moon.mp3" => heard(2, 2, None, None),
            "Speech Excerpt.mp3" => heard(3, 1, None, None),
            "My Own Time - A Memoir.mp3" => heard(370, 2, None, Some("My Own Time")),
            "The Salt Road.m4b" => heard(400, 2, None, Some("The Salt Road")),
            "Evening Talk.mp3" => heard(50, 2, None, None),
            _ => return None,
        })
    });
    let verdicts: Vec<(Vec<String>, Verdict)> = folder
        .all()
        .iter()
        .map(|s| {
            let paths = s
                .paths
                .iter()
                .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().into_owned())
                .collect();
            (paths, s.verdict)
        })
        .collect();
    assert_eq!(
        verdicts,
        [
            (
                vec!["My Own Time - A Memoir.mp3".into()],
                Verdict::Audiobook
            ),
            (vec!["Talks".into()], Verdict::Doubtful),
            (vec!["The Salt Road.m4b".into()], Verdict::Audiobook),
        ]
    );
    assert_eq!(
        (report.audiobooks, report.doubtful, report.dropped),
        (2, 1, 4),
        "the album, both soundtrack tracks and the clip are dropped"
    );
    let memoir = folder.all().into_iter().next().unwrap();
    assert_eq!(memoir.duration_ms, Some(370 * 60_000));
    assert_eq!(memoir.author.as_deref(), Some("Ada Penn"));
}

#[test]
fn a_spoken_genre_or_a_word_like_unabridged_says_audiobook() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    sized(root, "Stories/Harbour Tale.mp3", 46 * MIB);
    sized(root, "Night Ferry (Unabridged).mp3", 46 * MIB);
    let mut folder = AudiobookFolder::read(root);
    folder.classify(&|path: &Path| {
        let spoken = path.ends_with("Harbour Tale.mp3");
        Some(heard(50, 2, spoken.then_some("Audiobook"), None))
    });
    assert!(
        folder.all().iter().all(|s| s.verdict == Verdict::Audiobook),
        "{:?}",
        folder.all()
    );
}

#[test]
fn files_of_one_album_in_a_folder_are_one_audiobook() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    sized(root, "Downloads/Salt Road Opening.mp3", 30 * MIB);
    sized(root, "Downloads/Harbour Chapter.mp3", 30 * MIB);
    sized(root, "Downloads/Unrelated Talk.mp3", 30 * MIB);
    let mut folder = AudiobookFolder::read(root);
    folder.classify(&|path: &Path| {
        let album = (!path.ends_with("Unrelated Talk.mp3")).then_some("The Salt Road");
        Some(heard(60, 1, None, album))
    });
    assert_eq!(
        listed(&folder, root),
        [
            vec![
                "Downloads/Harbour Chapter.mp3",
                "Downloads/Salt Road Opening.mp3"
            ],
            vec!["Downloads/Unrelated Talk.mp3"],
        ]
    );
    // The album's name is one it goes by.
    let found = folder.suggest(Path::new("/books/x.epub"), Some("The Salt Road"), &[]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].files.len(), 2);
}

#[test]
fn a_genre_that_is_really_a_title_says_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for lecture in 1..=4 {
        sized(
            root,
            &format!("Latin Course/{lecture:02} Lecture.mp3"),
            60 * MIB,
        );
    }
    let mut folder = AudiobookFolder::read(root);
    let report = folder.classify(&|_: &Path| {
        // "Classical" is a music genre, but this is the course's title put in the genre tag.
        Some(heard(
            30,
            2,
            Some("Latin 101: Learning a Classical Language"),
            None,
        ))
    });
    assert_eq!((report.dropped, folder.all().len()), (0, 1));
}
