# Local Chatterbox client

`aoide-chatterbox` is an async Rust client for **devnen/Chatterbox-TTS-Server**. It is independent of the EPUB parser, Tauri, Vue, filesystem storage, and generation queues. Use it from a Tokio runtime.

## Public seam

```rust
let client = ChatterboxClient::new(ClientOptions::default())?;
let voices = client.voices().await?;

// Choose an actual filename from `voices`; there is no implicit fallback voice.
let mut request = SpeechRequest::new("A short chapter of original text.", "Emily.wav");
request.settings.seed = Some(0);
request.settings.language = Some("en".into());
let audio = client.generate(&request).await?;

// Read-only WAV bytes and metadata, ready for a later storage/playback layer.
let bytes = audio.as_bytes();
let metadata = audio.metadata();
```

Import these types from `aoide_chatterbox`. `AudioMetadata` exposes sample rate, channels, bits per sample, frame count (per channel), and PCM/float encoding. Duration in seconds is `frames / sample_rate` using floating-point division.

### Wire contract

- `GET /get_predefined_voices`: JSON array of `{ "filename", "display_name" }`. Returns `Voice { id, display_name }` in server order. Empty lists are valid. Invalid/duplicate IDs and blank names are rejected.
- `POST /tts`: sends text unchanged with `voice_mode: "predefined"`, `predefined_voice_id`, `output_format: "wav"`, `split_text: true`, `chunk_size`, and `stream: false`. Optional seed/language fields are omitted unless supplied.
- One generation call sends one chapter in one HTTP request. **Chatterbox** performs its internal text chunking and audio stitching. Default target chunk size is 240 characters; configurable from 50–500 to match the server schema.
- Both operations require HTTP 200 and the expected content type. Accepted/pending responses, redirects, and partial responses do not count as a completed chapter.
- WAV responses are checked for complete RIFF/chunk lengths, a single format/data chunk, nonempty complete frames, and decodable PCM or finite float samples. Full sample validation runs on a blocking worker, not the async I/O thread. No decoded sample buffer is retained.

The client does not use an LLM, rewrite text, select/switch models, change server configuration, upload voices, or silently choose a different voice. It currently supports predefined voices only. Language support, active engine, generation defaults, and speech quality are server responsibilities; voice listing does **not** prove the model is loaded. A fixed seed does not guarantee byte-identical GPU output.

## Locality, limits, and failures

Defaults:

| Setting | Default |
| --- | --- |
| Server | `http://127.0.0.1:8004` |
| Connection deadline | 2 seconds |
| Voice discovery HTTP deadline | 5 seconds |
| Chapter generation HTTP deadline | 30 minutes |
| Input text | 1 MiB UTF-8 bytes |
| Audio response | 256 MiB |
| Voice-list response | 1 MiB |
| Error details read | 8 KiB |

All deadlines and byte budgets must be positive. HTTP deadlines include reading the body, but not subsequent CPU-side WAV validation. Limits cover declared response lengths and streamed/chunked bodies. An oversized error body is truncated; the HTTP status is retained with a generic message if its JSON cannot be parsed safely.

Only HTTP(S) loopback origins are permitted. `localhost` is pinned to `127.0.0.1`; numeric loopback IPv4/IPv6 is supported. LAN/cloud hosts, URL credentials, paths, query strings, and fragments are rejected. Environment proxies are bypassed; redirects and automatic retries are disabled. HTTPS uses normal certificate verification; there is no insecure-certificate override or authentication support in this pass.

Errors distinguish invalid configuration/input, transport failures, timeouts, HTTP failures, oversized bodies, malformed protocol responses, invalid audio, and internal validation-worker failure. Pydantic error arrays retain messages but not echoed `input` fields; HTML/raw error bodies are not surfaced. Server-provided detail strings are bounded but may still contain server-supplied content. The client itself does not log chapter text.

**Do not automatically retry failed generation.** The server may still be working after a timeout or dropped client future; there is no cancellation acknowledgement or idempotency protocol here. This crate writes no files, but the server may save audio/logs according to its own configuration.

WAV validation establishes structural audio validity, not that every word was spoken correctly or that the recording sounds good. No listening-quality claim is made by these tests.

## Tests

From the repository root:

```sh
npm run test:chatterbox
npm run check:chatterbox
npm test  # parser + client
```

The default tests use ephemeral loopback HTTP servers and tiny original WAV/JSON fixtures. They exercise the public client over actual HTTP; no GPU, real Chatterbox process, cloud service, or private EPUB text is used. Behaviors were added in failing-test → minimal-implementation cycles.

### Explicit real-server smoke tests

Both live tests passed on 2026-11-10 against upstream commit `915ae28…` running `chatterbox-turbo` on CUDA (RTX 4070 Ti SUPER). Run them with `--test-threads=1`: the server does not answer other requests while generating, so a concurrent voice-list call exceeds its 5-second deadline. Start a compatible server separately, then opt in:

```sh
# Read-only voice discovery; does not establish model readiness.
AOIDE_CHATTERBOX_URL=http://127.0.0.1:8004 \
cargo test --manifest-path crates/chatterbox-client/Cargo.toml \
  --test live_server lists_real_server_voices -- --ignored

# Uses the local model/GPU for one short original sentence.
# Replace Emily.wav with a predefined voice installed on that server.
AOIDE_CHATTERBOX_VOICE=Emily.wav \
cargo test --manifest-path crates/chatterbox-client/Cargo.toml \
  --test live_server generates_a_short_original_sentence -- --ignored --test-threads=1
```

The client does not install, restore, or launch the server. Queueing, parsed-chapter orchestration, audio caching/persistence, progress/resume, Tauri commands, and Vue wiring remain separate work.

## Contract evidence

The preserved local configuration identifies port 8004 and Chatterbox TTS Server-style settings, and the server is installed locally at that commit. The adapter's contract was checked against upstream commit `915ae289340e10c6047f27f47e22eae9bf350c32`, rather than inferred from the generic Chatterbox model name:

- [`models.py`](https://github.com/devnen/Chatterbox-TTS-Server/blob/915ae289340e10c6047f27f47e22eae9bf350c32/models.py): `CustomTTSRequest` fields, chunk-size bounds, and streaming flag.
- [`server.py`](https://github.com/devnen/Chatterbox-TTS-Server/blob/915ae289340e10c6047f27f47e22eae9bf350c32/server.py): predefined-voice route, `/tts` request handling, model-not-loaded error, and binary WAV response.
- [`utils.py`](https://github.com/devnen/Chatterbox-TTS-Server/blob/915ae289340e10c6047f27f47e22eae9bf350c32/utils.py): predefined filename/display-name schema.

The opt-in live smoke tests confirm compatibility with the local install.
