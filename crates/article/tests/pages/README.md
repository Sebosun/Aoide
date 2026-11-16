# Crawled pages

Real web pages, kept as they were fetched, and what Aoide reads from each: the regression
tests in `tests/pages.rs` read every page and compare with what was reviewed.

```
<corpus>/<page>/
  source.html     the page as fetched (never edited)
  url             its address, when known (else it is read as http://fakehost/test/page.html)
  expected.txt    the text: `#` headings, `- ` list items, fenced preformatted, blank-line blocks
  expected.json   title, byline, site, date, language, links, words, completeness, feeds, warnings
```

## Reviewing a change

A change to `crates/article` that reads any page differently fails the test with the pages and
their first differing lines. Write the new expectations and read the diff before keeping it:

```sh
AOIDE_WRITE_PAGES=1 cargo test -p aoide-article --release --test pages
git diff crates/article/tests/pages      # every changed line is a decision
AOIDE_PAGE=nytimes cargo test -p aoide-article --test pages   # only pages whose name has it
```

`--release` takes about 2 s for all pages; a debug build about 20 s.

## Adding a page

Only public pages, nothing behind a sign-in. Fetch it through `crates/fetch` (the only network
path), keep it, then write and review its expectations:

```sh
AOIDE_TEST_ARTICLE_URL=https://example.org/an-article AOIDE_SAVE_PAGE=crawled/example-article \
  cargo test -p aoide-fetch --test live_page -- --ignored --nocapture
AOIDE_WRITE_PAGES=1 AOIDE_PAGE=example-article cargo test -p aoide-article --release --test pages
```

A page that shows a bug gets a small synthetic test beside the rule it breaks
(`tests/extract.rs`, `tests/meta.rs`, `tests/completeness.rs`) before the fix; the crawled page
then shows the fix in its diff.

## `readability/`

The 130 `source.html` files of Mozilla Readability's test pages
(<https://github.com/mozilla/readability>, `test/test-pages/`, commit `ab4027a`, 2026-07-09),
Apache-2.0 (`LICENSE.md`, `NOTICE`). Their pages went through Readability's tidy step, so some
markup is reshaped (`<p>When</p>` then the formula, on `mathjax`); several are synthetic cases
for one rule (`001`–`005`, `metadata-*`, `ol`, `remove-aria-hidden`, …). Readability's own
expected output is not copied: Aoide's was compared with it, and each difference checked in the
source. What the comparison found:

**Fixed** (each with a synthetic test):

- Lists whose items wrap their text in `<p>` read as paragraphs (`ol`, `guardian-1`,
  `firefox-nightly-blog`, `nytimes-5`). The item's first block is now the item.
- Formulas (MathML) were dropped silently, leaving "When , there are two solutions"
  (`mathjax`, `wikipedia-3`). They are read inline now, without their TeX annotation.
- Chinese and Japanese were counted as a handful of words (`qq` 22, `pixnet` 152, `youth` 27),
  so whole articles were judged `Empty`. Each character counts as a word now.
- `herald-sun-1` was judged paywalled for a `paywall` class the site puts on every `body`,
  with 795 words captured. A paywall's container counts only when the body is short.
- JSON-LD inside a CDATA section was ignored (`aclu`, `gitlab-blog`, `lazy-image-2`,
  `spiceworks`, `toc-missing`, `wikipedia-2`/`-3`), losing dates, authors and site names.
- `<meta property="og:title twitter:title">` (several keys) was not read, and a `name` beat a
  `property`, so a decoy `<meta name="author" content="FAIL">` won
  (`004-metadata-space-separated-properties`, `metadata-content-missing`).
- `SocialMediaPosting` (`tumblr`) and Parse.ly tags (`parsely-metadata`) were not read; Dublin
  Core is now read where Open Graph is silent.
- Bylines the article shows but its metadata lacks were dropped (40 pages: `nytimes-*`,
  `wapo-*`, `ars-1`, `cnet`, …). They are read now, "By" taken off, refused when they hold a
  date or a button's words (`breitbart`, `lwn-1`, `seattletimes-1`, `spiceworks`,
  `royal-road`'s "Follow Author", `yahoo-3`).
- A social title chosen over the headline the page shows (`article-author-tag`, `yahoo-2`). A
  title shown as the page's `h1`/`h2` wins now.
- Once the CDATA JSON-LD was read, `aclu`'s headings and paragraphs were swapped for its flat
  `articleBody`, 3 % longer. The metadata body now replaces the text only when the text falls
  short of it (under 60 %).

**Aoide differs on purpose:**

- Titles lose the site's name ("… - The Verge"); languages keep their region (`en-US`).
- Precedence is JSON-LD, then Open Graph, then the rest; Readability prefers Dublin Core
  (`003-metadata-preferred`). The site name is JSON-LD's `publisher` when it has one
  (`wikipedia-*` say "Wikimedia Foundation, Inc."; `aktualne` drops its tagline).
- Images, figures and tables are left out with a warning; Readability keeps them.
- Pages under 40 words are `Empty` (Readability's one-rule cases: `005-*`,
  `invalid-attributes`, `js-link-replacement`, `lazy-image-3`, `ol`, `remove-aria-hidden`).
- In a list item holding a heading and paragraphs (`mozilla-2`'s feature cards), the heading
  stands as the item's first block and what follows are paragraphs.

**Readability is wrong, Aoide right:** `liberation-1`'s byline is "AFP", marked up as the
author (`itemprop="author"`); Readability's "Sébastien Farcis" belongs to a linked story.

**Known wrong in both:** `yahoo-4`'s byline "個人" is a navigation link (`li#gnPriBylines`)
the content finder takes for a byline. `videos-2`'s lead image is the site's placeholder,
which its JSON-LD names.

**Not differences:** word splits on `pixnet` and `qq` come from adjacent inline elements
written without spaces; the text is the same.
