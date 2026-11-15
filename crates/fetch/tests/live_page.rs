//! An explicit opt-in check against a real web page: fetched once over the network and
//! extracted. Never run by the default test command; nothing from the page is printed but its
//! title, site, word count and completeness.
//!
//! ```sh
//! AOIDE_TEST_ARTICLE_URL=https://example.org/some-article \
//! cargo test -p aoide-fetch --test live_page -- --ignored --nocapture
//! ```
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
    assert_eq!(extracted.document.chapters.len(), 1);
    assert!(extracted.meta.word_count > 0, "no text was found");
}
