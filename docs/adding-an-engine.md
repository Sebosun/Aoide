# Adding a TTS engine

Aoide talks to speech engines through one Rust trait, `aoide_tts::SpeechEngine`
([`crates/tts/src/lib.rs`](../crates/tts/src/lib.rs)). Everything else (segmenting, scheduling,
caching, playback, the settings screen) is engine-independent.

## Do you need an adapter at all?

If your server implements the OpenAI-style speech API, you do not need to write any code:

- `POST /v1/audio/speech` with `{"model", "input", "voice", "response_format": "wav", "speed"}`
  returning a WAV file, and
- `GET /v1/audio/voices` returning `{"voices": [...]}` or a bare list of voice names or
  `{id, name}` objects.

Kokoro-FastAPI, openedai-speech and Chatterbox-TTS-Server's `/v1` routes all qualify. In
**Settings → Speech engine**, choose *OpenAI-compatible speech server*, enter the server address
(for example `http://127.0.0.1:8880`) and, if the server hosts several models, the model name.
The adapter is [`crates/openai-speech`](../crates/openai-speech/src/lib.rs).

Write an adapter when the engine has its own protocol or exposes settings worth showing in the
UI (as [`crates/chatterbox-client`](../crates/chatterbox-client/src/engine.rs) does).

## The contract

```rust
pub trait SpeechEngine: Send + Sync {
    fn descriptor(&self) -> &EngineDescriptor;
    fn status(&self) -> BoxFuture<'_, EngineStatus>;
    fn voices(&self) -> BoxFuture<'_, Result<Vec<Voice>, EngineError>>;
    fn synthesize<'a>(&'a self, request: &'a SynthesisRequest)
        -> BoxFuture<'a, Result<Synthesis, EngineError>>;
}
```

### `descriptor()`

A static description that the app uses to build its UI and cache keys:

| Field | Meaning |
| --- | --- |
| `kind` | Registry key stored in engine profiles, e.g. `"my-engine"`. Never change it once released. |
| `name` | Shown in the engine picker. |
| `adapter_version` | Part of every cache key. Bump it when the same input would now produce different audio, so old audio is not reused. |
| `languages` | Primary language subtags you support (`["en", "fr"]`), or `None` for any. Books in other languages get a warning but can still be generated, and no language is sent. |
| `word_timings` | `true` if `Synthesis::word_timings` is filled with real per-word times. Otherwise Aoide estimates them. |
| `max_chars` | Longest text one request should carry. Aoide sends single sentences or small groups (about 250 characters). |
| `params` | Settings shown as sliders, number fields, menus or checkboxes: `ParamSpec { key, label, kind, default }` with `ParamKind::{Float, Int, Choice, Bool}`. |

`SynthesisRequest::params` always holds a complete, validated set: Aoide calls `resolve_params`
with your descriptor before every request, applying the profile's values and then the book's
overrides.

### `status()`

Must not fail. Return `Offline` when nothing is listening (connection refused), `Loading` when
the server answers but its model is not ready, `Ready(Some(model description))` when it can
synthesize, and `Error(message)` otherwise. Aoide polls this every few seconds and, if the
profile has a launch command, starts the server when it reports `Offline`.

### `voices()`

The voices to offer in menus. `Voice { id, display_name }`; the `id` is sent back unchanged in
`SynthesisRequest::voice_id`.

### `synthesize()`

Turn one request into mono PCM: `Synthesis { sample_rate, samples: Vec<f32>, word_timings }`.
If your engine returns WAV, `aoide_tts::decode_wav` does the conversion (downmixing stereo).
Aoide measures the returned audio itself, encodes it as Ogg Opus and caches it under
`cache_key(descriptor, model, request)`.

Map failures onto `EngineError`: `Unavailable` for connection problems (Aoide backs off and
retries later), `Timeout`, `Http { status, message }` with a short readable message (never echo
HTML pages), `InvalidInput` for requests you reject before sending, and `InvalidResponse` /
`InvalidAudio` for replies you cannot use.

## Rules every adapter follows

- **Local only.** Validate the profile's address with `aoide_tts::validate_endpoint(url,
  profile.network)`. By default only loopback addresses pass; the user can allow home-network
  addresses per profile, and the UI warns them that book text leaves the machine. No cloud
  services or API keys.
- **No proxies, no redirects, no automatic retries.** With reqwest:
  `.no_proxy().redirect(Policy::none()).retry(reqwest::retry::never())`. Aoide's worker decides
  when to retry.
- **Bound everything.** Set connect and request timeouts, and cap response sizes while
  streaming the body.
- **Send the text as given.** Aoide has already removed footnote markers and skipped segments
  with nothing to say. Do not rewrite, split or normalise text in the adapter.

## Registering it

1. Create a crate under `crates/` (the workspace picks up `crates/*` automatically) that depends
   on `aoide-tts`.
2. Expose a factory and a `register` function:

   ```rust
   pub const KIND: &str = "my-engine";

   pub fn register(registry: &mut aoide_tts::Registry) {
       registry.register(descriptor(), create);
   }

   fn create(profile: &EngineProfile) -> Result<Arc<dyn SpeechEngine>, EngineError> {
       Ok(Arc::new(MyEngine::new(&profile.base_url, profile.network, profile.model.clone())?))
   }
   ```

   `Registry::create` validates the profile's address before calling your factory.
3. Add the crate to `src-tauri/Cargo.toml` and call `my_engine::register(&mut registry)` in
   `registry()` in [`src-tauri/src/core.rs`](../src-tauri/src/core.rs). The engine then appears
   in **Settings → Speech engine** with its settings drawn from the descriptor.

## Testing it

Test the adapter's public API against a fake server on `127.0.0.1:0` that returns synthetic
responses. Copy the helper in
[`crates/openai-speech/tests/support/mod.rs`](../crates/openai-speech/tests/support/mod.rs).
Enable the shared conformance checks with
`aoide-tts = { path = "../tts", features = ["testing"] }` in `[dev-dependencies]`:

```rust
use aoide_tts::testing::{assert_valid_descriptor, assert_valid_synthesis};

#[test]
fn descriptor_is_valid() {
    assert_valid_descriptor(&my_engine::descriptor());
}

#[tokio::test]
async fn synthesis_is_valid() {
    let server = Server::start(Response::wav(support::wav())).await;
    let engine = MyEngine::new(&server.url, NetworkScope::Loopback, None).unwrap();
    let request = /* a SynthesisRequest with resolve_params(&descriptor(), &Params::new()) */;
    let synthesis = engine.synthesize(&request).await.unwrap();
    assert_valid_synthesis(&request, &synthesis);
}
```

At a minimum, cover: the request body your server receives, every voice-list shape you accept,
`status()` for ready, offline (a closed port) and error replies, HTTP error mapping, oversized
responses, and that the registry rejects a LAN address under `NetworkScope::Loopback`.
[`crates/openai-speech/tests/engine.rs`](../crates/openai-speech/tests/engine.rs) is a complete
example. Tests against a real engine (GPU, downloaded models) must be opt-in, as in
`crates/chatterbox-client/tests/live_server.rs`.
