mod support;

use aoide_recording::{inspect, inspect_as_listed, DrmKind, Problem, RecordingChapter};
use support::{fixture, mp4};

#[test]
fn a_folder_is_ordered_by_track_tags_rather_than_names() {
    let found = inspect(&[fixture("set")]);
    let names: Vec<_> = found
        .files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(names, ["b-first.mp3", "a-second.mp3"]);
    assert_eq!(found.title.as_deref(), Some("The Lantern Keeper"));
    assert_eq!(found.author.as_deref(), Some("A. Writer"));
    assert!(found.problems.is_empty());
}

#[test]
fn files_without_track_tags_follow_natural_name_order() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["Part 10.wav", "Part 2.wav", "part 1.wav"] {
        support::sine(&dir.path().join(name), 8_000, 440.0, 0.1);
    }
    let found = inspect(&[dir.path().to_owned()]);
    let names: Vec<_> = found
        .files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(names, ["part 1.wav", "Part 2.wav", "Part 10.wav"]);
}

#[test]
fn files_listed_by_hand_keep_their_order_and_chapters_follow_it() {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = ["part 1.wav", "part 2.wav", "part 3.wav"]
        .iter()
        .map(|name| {
            let path = dir.path().join(name);
            support::sine(&path, 8_000, 440.0, 0.5);
            path
        })
        .collect();
    let listed = [paths[2].clone(), paths[0].clone(), paths[1].clone()];
    let found = inspect_as_listed(&listed);
    let names: Vec<_> = found
        .files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(names, ["part 3.wav", "part 1.wav", "part 2.wav"]);
    let starts: Vec<_> = found
        .chapters
        .iter()
        .map(|c| (c.title.as_str(), c.start_ms))
        .collect();
    assert_eq!(starts, [("part 3", 0), ("part 1", 500), ("part 2", 1_000)]);
}

#[test]
fn lengths_are_counted_from_the_audio() {
    let found = inspect(&[
        fixture("set"),
        fixture("book.m4b"),
        fixture("tone.flac"),
        fixture("tone.ogg"),
    ]);
    let expected = [
        ("b-first.mp3", "mp3", 1_000),
        ("a-second.mp3", "mp3", 500),
        ("book.m4b", "aac", 2_000),
        ("tone.flac", "flac", 750),
        ("tone.ogg", "vorbis", 500),
    ];
    assert_eq!(found.files.len(), expected.len());
    for (name, codec, want_ms) in expected {
        let file = found
            .files
            .iter()
            .find(|f| f.path.file_name().unwrap() == name)
            .unwrap_or_else(|| panic!("{name} missing"));
        assert_eq!(file.codec, codec);
        let ms = file.duration_us() / 1_000;
        // Lossy encoders may keep up to a frame of priming they don't declare.
        assert!(
            ms.abs_diff(want_ms) <= 50,
            "{name}: {ms} ms, expected {want_ms}"
        );
    }
    let flac = found.files.iter().find(|f| f.codec == "flac").unwrap();
    assert_eq!(flac.frames, 36_000);
    assert_eq!((flac.sample_rate, flac.channels), (48_000, 1));
}

#[test]
fn chapters_come_from_an_m4b_or_else_one_per_file() {
    let book = inspect(&[fixture("book.m4b")]);
    assert_eq!(
        book.chapters,
        [
            RecordingChapter {
                title: "Opening".into(),
                start_ms: 0
            },
            RecordingChapter {
                title: "Closing".into(),
                start_ms: 1_000
            },
        ]
    );
    let set = inspect(&[fixture("set")]);
    let titles: Vec<_> = set
        .chapters
        .iter()
        .map(|c| (c.title.as_str(), c.start_ms))
        .collect();
    assert_eq!(titles.len(), 2);
    assert_eq!(titles[0], ("Part One", 0));
    assert_eq!(titles[1].0, "Part Two");
    assert!(titles[1].1.abs_diff(1_000) <= 50);
}

#[test]
fn drm_protected_files_are_refused_with_their_kind() {
    let dir = tempfile::tempdir().unwrap();
    let cases = [
        ("audible.aax", mp4(b"aax ", b"aavd"), DrmKind::Audible),
        ("audible.aaxc", mp4(b"aaxc", b"mp4a"), DrmKind::Audible),
        ("store.m4b", mp4(b"M4B ", b"drms"), DrmKind::FairPlay),
        ("cenc.m4a", mp4(b"M4A ", b"enca"), DrmKind::Encrypted),
    ];
    let mut paths = Vec::new();
    for (name, bytes, _) in &cases {
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        paths.push(path);
    }
    let old = dir.path().join("old.aa");
    let mut aa = vec![0u8; 64];
    aa[4..8].copy_from_slice(&0x5790_7536u32.to_be_bytes());
    std::fs::write(&old, aa).unwrap();
    paths.push(old.clone());

    let found = inspect(&paths);
    assert!(found.files.is_empty());
    for (path, kind) in paths
        .iter()
        .zip(cases.iter().map(|c| c.2).chain([DrmKind::Audible]))
    {
        let problem = found
            .problems
            .iter()
            .find(|p| &p.path == path)
            .expect("listed");
        assert_eq!(problem.problem, Problem::Drm(kind), "{path:?}");
    }
}

#[test]
fn files_it_cannot_play_are_listed_with_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join("notes.mp3");
    std::fs::write(&fake, "not audio at all").unwrap();
    let found = inspect(&[fixture("tone.opus"), fake.clone(), fixture("tone.flac")]);
    assert_eq!(found.files.len(), 1);
    let reason = |path: &std::path::Path| {
        found
            .problems
            .iter()
            .find(|p| p.path == path)
            .map(|p| p.problem.clone())
    };
    assert!(
        matches!(reason(&fixture("tone.opus")), Some(Problem::Unsupported(m)) if m.contains("opus"))
    );
    assert!(matches!(reason(&fake), Some(Problem::Unreadable(_))));
}

#[test]
fn only_audio_files_are_taken_from_a_folder() {
    let dir = tempfile::tempdir().unwrap();
    support::sine(&dir.path().join("01.wav"), 8_000, 440.0, 0.1);
    std::fs::write(dir.path().join("cover.jpg"), [0xFF, 0xD8]).unwrap();
    std::fs::write(dir.path().join("info.txt"), "hello").unwrap();
    std::fs::create_dir(dir.path().join("CD2")).unwrap();
    support::sine(&dir.path().join("CD2/01.wav"), 8_000, 440.0, 0.1);
    let found = inspect(&[dir.path().to_owned()]);
    assert_eq!(found.files.len(), 2);
    assert!(found.problems.is_empty());
}

/// `mp3` with its ID3 tag replaced by an ID3v2.3 tag holding an album and a front cover.
fn with_cover(mp3: &[u8], picture: &[u8]) -> Vec<u8> {
    let audio = if mp3.starts_with(b"ID3") {
        let size = mp3[6..10]
            .iter()
            .fold(0usize, |n, b| (n << 7) | usize::from(*b));
        &mp3[10 + size..]
    } else {
        mp3
    };
    let frame = |id: &[u8], body: &[u8]| {
        let mut f = id.to_vec();
        f.extend_from_slice(&(body.len() as u32).to_be_bytes());
        f.extend_from_slice(&[0, 0]);
        f.extend_from_slice(body);
        f
    };
    let mut apic = vec![0];
    apic.extend_from_slice(b"image/png\0");
    apic.push(3); // front cover
    apic.push(0); // no description
    apic.extend_from_slice(picture);
    let mut frames = frame(b"TALB", b"\0Harbour Notes");
    frames.extend(frame(b"APIC", &apic));
    let size = frames.len();
    let mut tag = b"ID3\x03\x00\x00".to_vec();
    tag.extend((0..4).rev().map(|i| ((size >> (7 * i)) & 0x7f) as u8));
    tag.extend(frames);
    tag.extend_from_slice(audio);
    tag
}

#[test]
fn an_embedded_front_cover_is_found() {
    let dir = tempfile::tempdir().unwrap();
    let mp3 = std::fs::read(fixture("set/b-first.mp3")).unwrap();
    let picture = b"\x89PNG\r\n\x1a\nnot really a picture";
    let path = dir.path().join("notes.mp3");
    std::fs::write(&path, with_cover(&mp3, picture)).unwrap();
    let found = inspect(&[path]);
    assert!(found.problems.is_empty(), "{:?}", found.problems);
    assert_eq!(found.title.as_deref(), Some("Harbour Notes"));
    let cover = found.cover.expect("the cover");
    assert_eq!(
        (cover.media_type.as_str(), cover.data.as_slice()),
        ("image/png", &picture[..])
    );
    assert_eq!(inspect(&[fixture("tone.flac")]).cover, None);
}

#[test]
fn a_glance_reads_length_channels_and_tags_without_decoding() {
    use aoide_recording::glance;
    let mp3 = glance(&fixture("set").join("b-first.mp3")).expect("an mp3 is heard");
    assert_eq!(mp3.channels, 1);
    assert_eq!(mp3.tags.album.as_deref(), Some("The Lantern Keeper"));
    assert_eq!(mp3.tags.artist.as_deref(), Some("A. Writer"));
    for (name, want_ms, channels) in [
        ("set/b-first.mp3", 1_000, 1),
        ("book.m4b", 2_000, 1),
        ("tone.flac", 750, 1),
        ("tone.ogg", 500, 2),
    ] {
        let heard = glance(&fixture(name)).unwrap_or_else(|| panic!("{name} not heard"));
        let ms = heard
            .duration_ms
            .unwrap_or_else(|| panic!("{name}: no length"));
        // A glance may estimate from the first packets; within a tenth is enough to judge by.
        assert!(
            ms.abs_diff(want_ms) <= want_ms / 10 + 60,
            "{name}: {ms} ms, expected {want_ms}"
        );
        assert_eq!(heard.channels, channels, "{name}");
    }
    assert_eq!(
        glance(&fixture("book.m4b")).unwrap().tags.artist.as_deref(),
        Some("A. Writer")
    );
    assert!(glance(&fixture("README.md")).is_none(), "not audio");
}
