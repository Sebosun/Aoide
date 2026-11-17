//! Schema v26: the library's text, block by block, in a full-text index; searching it.
use aoide_library::{fts_query, BookId, ItemKind, Library, NewBook, PassageRow};

fn item(lib: &Library, id: &str, kind: ItemKind, added_at: i64) -> BookId {
    let id = BookId(id.into());
    lib.upsert_book(
        &NewBook {
            id: id.clone(),
            kind,
            path: format!("{}.item", id.0).into(),
            title: Some("Synthetic item".into()),
            authors: vec![],
            language: None,
            cover_path: None,
            error: None,
        },
        added_at,
    )
    .unwrap();
    id
}

fn row(chapter: u32, block: u32, at: u32, text: &str) -> PassageRow {
    // One sentence per full stop, as the core's segments would start.
    let mut segments = vec![(0, format!("{chapter}:{block}:0"))];
    let mut index = 0;
    for (i, c) in text.char_indices() {
        if c == '.' && i + 2 < text.len() {
            index += 1;
            let start = text[..i + 2].chars().count() as u32;
            segments.push((start, format!("{chapter}:{block}:{index}")));
        }
    }
    PassageRow {
        chapter,
        block,
        at,
        text: text.into(),
        segments,
    }
}

fn rows_of(lib: &Library) -> i64 {
    lib.passages_rows().unwrap()
}

#[test]
fn full_text_search_is_compiled_in() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE VIRTUAL TABLE t USING fts5(x);")
        .unwrap();
}

#[test]
fn an_indexed_book_is_found_by_its_words_and_its_place_is_given() {
    let lib = Library::open_in_memory().unwrap();
    let book = item(&lib, "book", ItemKind::Book, 1);
    lib.index_passages(
        &book,
        1,
        &[
            row(0, 0, 0, "The harbour master lights the lamps."),
            row(
                2,
                3,
                120,
                "Fog came in. The CAFÉ by the slipway kept its shutters down.",
            ),
        ],
    )
    .unwrap();
    assert_eq!(lib.indexed_version(&book).unwrap(), Some(1));
    assert!(
        lib.passages_bytes().unwrap() > 0,
        "the index's size is measured"
    );
    // Case and accents fold; the last word matches as a prefix.
    let found = lib
        .search_passages(&fts_query("cafe slip").unwrap(), 10)
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].book_id, book);
    assert_eq!(found[0].more, 0);
    let hit = &found[0].hits[0];
    assert_eq!((hit.chapter, hit.block), (2, 3));
    // Its first match is in the second sentence, 17 characters into the block.
    assert_eq!(hit.segment_id, "2:3:1");
    assert_eq!(hit.first, 120 + 17);
    let marked: Vec<String> = hit
        .marks
        .iter()
        .map(|&(from, to)| hit.snippet.chars().skip(from).take(to - from).collect())
        .collect();
    assert_eq!(marked, ["CAFÉ", "slipway"]);
    assert!(hit.snippet.contains("by the"));
    assert!(lib
        .search_passages(&fts_query("lantern").unwrap(), 10)
        .unwrap()
        .is_empty());
}

#[test]
fn items_come_best_first_with_three_passages_each_and_how_many_more() {
    let lib = Library::open_in_memory().unwrap();
    let many = item(&lib, "many", ItemKind::Book, 1);
    let once = item(&lib, "once", ItemKind::Article, 2);
    let rows: Vec<PassageRow> = (0..5)
        .map(|i| {
            row(
                0,
                i,
                i * 100,
                &format!("Paragraph {i} mentions a lantern among other words, words and words."),
            )
        })
        .collect();
    lib.index_passages(&many, 1, &rows).unwrap();
    lib.index_passages(&once, 1, &[row(0, 0, 0, "Lantern lantern lantern.")])
        .unwrap();
    let found = lib
        .search_passages(&fts_query("lantern").unwrap(), 10)
        .unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].book_id, once, "the best passage first");
    assert_eq!((found[1].hits.len(), found[1].more), (3, 2));
    // At most `items` items.
    assert_eq!(
        lib.search_passages(&fts_query("lantern").unwrap(), 1)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn indexing_again_replaces_and_removing_the_item_removes_its_text_on_both_paths() {
    let lib = Library::open_in_memory().unwrap();
    let book = item(&lib, "book", ItemKind::Book, 1);
    lib.index_passages(&book, 1, &[row(0, 0, 0, "An old lighthouse.")])
        .unwrap();
    lib.index_passages(&book, 2, &[row(0, 0, 0, "A new pier.")])
        .unwrap();
    let find = |lib: &Library, words: &str| {
        lib.search_passages(&fts_query(words).unwrap(), 10)
            .unwrap()
            .len()
    };
    assert_eq!((find(&lib, "lighthouse"), find(&lib, "pier")), (0, 1));
    assert_eq!(lib.indexed_version(&book).unwrap(), Some(2));
    lib.remove_book(&book).unwrap();
    assert_eq!((rows_of(&lib), find(&lib, "pier")), (0, 0));

    // Merging an audiobook into its book removes the audiobook's row, and so its text.
    let audio = item(&lib, "audio", ItemKind::Audiobook, 1);
    let book = item(&lib, "book", ItemKind::Book, 2);
    lib.index_passages(&audio, 1, &[row(0, 0, 0, "Spoken words.")])
        .unwrap();
    lib.merge_audiobook(&audio, &book).unwrap();
    assert_eq!((rows_of(&lib), find(&lib, "spoken")), (0, 0));
}

#[test]
fn what_to_index_is_the_library_shelf_with_text_most_recently_opened_first() {
    let lib = Library::open_in_memory().unwrap();
    let old = item(&lib, "old", ItemKind::Book, 1);
    let opened = item(&lib, "opened", ItemKind::Book, 2);
    let article = item(&lib, "article", ItemKind::Article, 3);
    item(&lib, "audio", ItemKind::Audiobook, 4);
    let done = item(&lib, "done", ItemKind::Book, 5);
    lib.set_last_opened(&opened, 100).unwrap();
    lib.index_passages(&done, 1, &[]).unwrap();
    assert_eq!(
        lib.to_index(1).unwrap(),
        [opened.clone(), article.clone(), old.clone()]
    );
    // A newer rule indexes again.
    assert_eq!(lib.to_index(2).unwrap().len(), 4);
    // An item on a feed's shelf is not indexed, and is not found.
    lib.index_passages(&article, 1, &[row(0, 0, 0, "Feed words.")])
        .unwrap();
    lib.set_shelf(&article, "feed").unwrap();
    assert!(!lib.to_index(1).unwrap().contains(&article));
    assert!(lib
        .search_passages(&fts_query("feed").unwrap(), 10)
        .unwrap()
        .is_empty());
    // Turned off: everything goes.
    lib.clear_passages().unwrap();
    assert_eq!(
        (rows_of(&lib), lib.indexed_version(&done).unwrap()),
        (0, None)
    );
}

#[test]
fn the_users_words_become_quoted_terms_with_a_prefix_on_the_last() {
    assert_eq!(
        fts_query("  harbour   lamp ").as_deref(),
        Some(r#""harbour" AND "lamp"*"#)
    );
    // Operators and quotes are text.
    assert_eq!(
        fts_query(r#"NOT say "hi" OR*"#).as_deref(),
        Some(r#""NOT" AND "say" AND """hi""" AND "OR*"*"#)
    );
    assert_eq!(fts_query("   "), None);
    assert_eq!(fts_query(" - … "), None, "no word in it");
    // The query is accepted by SQLite whatever was typed.
    let lib = Library::open_in_memory().unwrap();
    for typed in [r#"NOT say "hi" OR*"#, "a:b", "(x", "^start", "NEAR(a b)"] {
        lib.search_passages(&fts_query(typed).unwrap(), 10).unwrap();
    }
}
