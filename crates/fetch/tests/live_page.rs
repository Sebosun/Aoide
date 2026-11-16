//! An explicit opt-in check against a real web page: fetched once over the network and
//! extracted. Never run by the default test command; nothing from the page is printed but its
//! title, site, word count and completeness.
//!
//! ```sh
//! AOIDE_TEST_ARTICLE_URL=https://example.org/some-article \
//! cargo test -p aoide-fetch --test live_page -- --ignored --nocapture
//! ```
//!
//! With `AOIDE_SAVE_PAGE=<corpus>/<name>` it also keeps the page as fetched, with its address,
//! in `crates/article/tests/pages/<corpus>/<name>/` for the crawled-page regression tests
//! (`crates/article/tests/pages/README.md` says how to review and add one).
use aoide_article::{extract, ExtractOptions};
use aoide_fetch::{FetchOptions, HttpFetcher, Via};
use url::Url;

#[tokio::test]
#[ignore = "set AOIDE_TEST_ARTICLE_URL to a public article's address"]
async fn a_real_page_is_fetched_and_extracted() {
    let url = std::env::var("AOIDE_TEST_ARTICLE_URL")
        .expect("set AOIDE_TEST_ARTICLE_URL to a public article's address");
    let url = Url::parse(&url).expect("a web address");
    let fetcher = HttpFetcher::new(FetchOptions::default()).unwrap();
    let snapshot = fetcher.fetch(&url).await.expect("the page is fetched");
    assert_eq!(snapshot.via, Via::Http);
    assert!(!snapshot.html.is_empty());

    let extracted = extract(
        &snapshot.html,
        &snapshot.final_url,
        &ExtractOptions::default(),
    );
    println!(
        "title: {:?}\nsite: {:?}\nwords: {}\ncompleteness: {:?}",
        extracted.meta.title,
        extracted.meta.site_name,
        extracted.meta.word_count,
        extracted.completeness
    );
    if let Ok(name) = std::env::var("AOIDE_SAVE_PAGE") {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../article/tests/pages")
            .join(name.trim_matches('/'));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("source.html"), &snapshot.html).unwrap();
        std::fs::write(folder.join("url"), format!("{}\n", snapshot.final_url)).unwrap();
        println!("kept in {}", folder.display());
    }
    assert_eq!(extracted.document.chapters.len(), 1);
    assert!(extracted.meta.word_count > 0, "no text was found");
}
