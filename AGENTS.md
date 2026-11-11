# Project conventions

- Vue: use BEM class names (`block__element--modifier`) and `<style scoped>` wherever possible. Keep shared resets and theme tokens global.
- TDD: agree the public seam, then run one failing behavior test before the smallest implementation that passes it. Repeat in vertical slices; test public results rather than private helpers.
- EPUB parsing belongs in `crates/epub-parser`, independent of Tauri, UI, network, and speech generation. Run its Cargo tests without building the desktop app.
- Chatterbox protocol code belongs in `crates/chatterbox-client`. Test the public client over loopback HTTP with synthetic responses; real-server/GPU synthesis tests are explicit opt-ins. Keep production requests loopback-only, with proxies, redirects, and automatic retries disabled.
- Use tiny synthetic EPUB fixtures with original text in committed tests. Personal EPUBs stay outside Git; inspect only metadata, navigation, and bounded excerpts when establishing local regression expectations.
- Model anything with states as an explicit enum changed by one transition function over named events (as in `src-tauri/src/core/preparation.rs`); views render that stage.
- `src/api/contract.ts` is generated from the Rust types and `CoreEvent`. After changing a type that crosses IPC, run `AOIDE_WRITE_CONTRACT=1 cargo test -p aoide contract`.
- Extraction is deterministic. Preserve uncertain content and expose warnings; report excluded sections and their reasons. Never modify source books.
