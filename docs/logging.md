# Logging

Aoide writes one JSON object per line to a daily file, and a short human form of the same events
to the terminal. Read the file with the recipes below: filter first, print a few fields, and
always end with a cap (`| tail -n N`, N ≤ 100).

## Where

| Run | Folder |
|---|---|
| The app | `~/.local/share/com.aoide.audiobooks/logs/` (the app data folder) |
| A smoke run (`AOIDE_SMOKE_HOME=<home>`) | `<home>/data/logs/` |

Files are named `aoide.YYYY-MM-DD.jsonl` by the UTC date, and the last seven are kept. `ts` is
UTC. `AOIDE_LOG` sets what the file keeps (EnvFilter syntax; default `warn,aoide=debug`, so
dependencies log warnings only); the terminal shows `warn,aoide=info`.

## A line

```json
{"ts":"2026-11-15T10:42:18.123Z","level":"INFO","event":"sync.stage","msg":"Syncing","book":"4b1e9c07aa11","from":"waiting","to":"transcribing","target":"aoide_lib::core::recordings"}
```

- `ts`, `level`, `event`, `msg` lead; the event's own fields follow; `target` (the Rust module)
  ends the line. Text values are cut at 500 characters.
- `event` is a stable dotted name (`area.what`), the key to filter on; it is missing only on
  records from dependencies.
- Shared fields: `book` (the first 12 characters of the book id; `SELECT … WHERE id LIKE
  '4b1e9c07aa11%'` finds it in `library.sqlite3`), `took_ms`, `from`/`to` (a stage change),
  `outcome` (`ok`, `failed`, `cancelled`), `error`, `file` (a file name).
- Privacy: lines carry book ids and file names only. Titles, folder paths, book text,
  transcripts and settings values stay out, so a line can be pasted into an issue.

Levels: `error` — the user lost something or a feature broke; `warn` — a failure Aoide recovered
from, a fallback or a retry; `info` — a stage change, a job's start and end with its duration, a
user action that changes state; `debug` — each external request, each piece of work picked;
`trace` — per-segment loops (off unless `AOIDE_LOG` asks).

## Recipes

Set the file once (`ls -t` puts today's first):

```sh
L=~/.local/share/com.aoide.audiobooks/logs; F=$(ls -t $L/aoide.*.jsonl | head -1)
```

| Question | Command |
|---|---|
| What happened, by kind | `jq -r '[.level,.event]\|@tsv' $F \| sort \| uniq -c \| sort -rn \| head -30` |
| Problems | `rg '"level":"(WARN\|ERROR)"' $F \| jq -c '{ts,event,book,msg,error}' \| tail -n 40` |
| Since the last start | `tail -n +$(rg -n '"event":"app.start"' $F \| tail -1 \| cut -d: -f1) $F \| jq -c '{ts,level,event,book,msg}' \| tail -n 60` |
| One book | `rg '"book":"4b1e9c07' $F \| jq -r '[.ts[11:23],.event,.from,.to,.msg]\|@tsv' \| tail -n 60` |
| One area, from a time | `rg '"event":"sync\.' $F \| jq -c 'select(.ts>="2026-11-15T10:00") \| {ts,event,book,from,to,msg}' \| tail -n 50` |
| Sync speed | `rg '"event":"sync.block"' $F \| jq -r '[.ts[11:19],.book,.rtf,.took_ms]\|@tsv' \| tail -n 20` |
| Dependency noise | `rg -v '"event":' $F \| jq -r '.target' \| sort \| uniq -c \| sort -rn \| head -20` |

Older days: replace `$F` with `$L/aoide.2026-11-14.jsonl`, or `$L/aoide.*.jsonl` for all.

## Events

One row per event name; adding an event adds its row here. Paths are under `src-tauri/src/`
unless they name a crate. Lines from the window carry `target` `aoide::ui`.

| Event | Level | Fields | Where |
|---|---|---|---|
| **App** | | | |
| `app.start` | info | `version`, `profile`, `gpu`, `os`, `smoke` | `lib.rs` setup |
| `app.exit` | info | | `lib.rs` exit |
| `app.panic` | error | `location`, `error` | `logging.rs` panic hook |
| `app.migrate` | info | | `lib.rs`, moving Folio's folders |
| `app.log` | warn | `error` | `logging.rs`, the file cannot be written |
| `app.emit` | error | `name`, `error` | `core.rs`, an event the window did not get |
| `app.poisoned` | error | `error` (reported) | `core.rs` `lock`, the first lock a panic poisoned |
| `config.saved` | info | `section`, `changed` (setting names) | `core/settings.rs` `update_app_settings` |
| `config.save` | warn | `kind`, `error`, `repeat` (reported); `section`, `outcome` from the first-run migration | `core.rs` (player), `core/config.rs` (migration) |
| `config.problem` | warn | `error` | `core.rs`, a settings file Aoide cannot read |
| `config.watch` | warn | `error` | `core/config.rs`, watching settings files |
| **Window** | | | |
| `ipc.error` | warn | `command`, `took_ms` (`msg` is the error) | `src/api/tauri.ts` `call`; cancelled scans left out |
| `ipc.slow` | info | `command`, `took_ms` | `src/api/tauri.ts`, answers over 2 s (long jobs left out) |
| `ui.error` | error | `view` | `src/main.ts`, an error no view caught |
| **Library** | | | |
| `library.scan` | info, warn, error | `books`, `new`, `read`, `took_ms`, `outcome`, `error` | `core.rs` `rescan` |
| `library.import` | info, warn | `ok`, `failed`; per failure `file`, `error` | `core.rs` `import_files` |
| `library.watch` | warn | `error` | `core.rs`, the library folder watcher |
| `discovery.scan` | info, warn | `roots`, `epubs`, `read`, `took_ms`, `outcome`, `error` | `core/discovery.rs` |
| `discovery.import` | info | `chosen`, `failed` | `core/discovery.rs` |
| `library.audiobook` | info | `book`, `outcome` (`ok`, `refused`), `error` | `core/audiobook.rs` `add_audiobook` |
| `library.merge` | info | `book`, `from` (the audiobook), `outcome` (`ok`, `failed`), `error` | `core/audiobook.rs` `merge_audiobook` |
| `progress.pending` | info / warn | `book`, `outcome` (`settled` into progress once the sync reached it, `dropped` when read or played first), `error` | `core/audiobook.rs` |
| `article.save` | info | `book`, `outcome` (`ok`) | `core/articles.rs` `save` |
| `article.stage` | info | `book`, `from`, `to`, `reason` (when it needs the user) | `core/articles.rs`, where the fetch stage's `next` is applied |
| `article.fetch` | info | `book`, `outcome` (`ok`, `failed`), `took_ms` | `core/articles.rs` `fetch` |
| `article.forget` | warn | `book`, `error` (its folder could not be deleted) | `core/articles.rs` `forget` |
| `feed.subscribe` | info | `feed` (its id), `outcome` | `core/feeds.rs` `subscribe_feed` |
| `feed.unsubscribe` | info | `feed` | `core/feeds.rs` `unsubscribe` |
| `feed.check` | info | `feed`, `outcome` (`ok`, `not_modified`, `failed`), `took_ms` | `core/feeds.rs` `check`: one per feed checked |
| `item.shelf` | info | `book`, `from`, `to` (`library`, `feed`) | `core/feeds.rs` `move_shelf`: an entry saved, or removed from the library while its feed lists it |
| `feed.stage` | info | `feed`, `from`, `to` (`idle`, `checking`, `failing`) | `core/feeds.rs` `check`: a feed starts or stops failing |
| `article.cover` | info / warn | `book`, `outcome` (`ok`, `failed`), `error` | `core/articles.rs` `keep_cover`: the page's lead image stored as the cover |
| `library.word_count` | warn | `book`, `error` | `core/shelf.rs` `load`, storing a parsed book's length |
| `reading.mode` | info | `book`, `from`, `to` | `core/reading.rs` `choose_mode`, a book switched between read and listen |
| `reading.state` | info | `book`, `from`, `to` | `core/reading.rs` `Reading::apply`, unread / in progress / finished |
| `reading.save` | warn | `kind`, `error`, `repeat` (reported) | `core/reading.rs`, storing reading time |
| `metadata.lookup` | warn | `book`, `outcome`, `error` | `core/discovery.rs` |
| `metadata.apply` | warn | `book`, `error` | `core/discovery.rs` |
| `http` | debug, warn | `service`, `op`, `found`, `took_ms`, `outcome`, `error` | `core/metadata.rs` (Open Library) |
| **Audiobooks** | | | |
| `audiobooks.scan` | info | `roots`, `only_added`, `dirs`, `files`, `audio`, `matches`, `unmatched` (whole searches), `took_ms`, `outcome` | `core/audiobooks.rs` |
| `audiobooks.pair` | info, warn | `book`, `candidate`, `outcome` (`ok`, `failed`, `skipped`: another book has it), `error` | `core/audiobooks.rs` |
| `match.stage` | info | `book`, `cause`, `from`, `to` | `core/audiobooks.rs`, each match transition |
| **Sync** (`core/recordings.rs`) | | | |
| `sync.stage` | info | `book`, `cause`, `from`, `to`, `took_ms` | `sync_event`, each transition |
| `sync.work` | debug | `kind`, `book` | each piece of work the job picks |
| `sync.transcriber` | info | `engine`, `model`, `gpu`, `threads`, `took_ms` | a transcriber loaded |
| `sync.check` | info | `book`, `found`, `of`, `passed` | the quick check |
| `sync.block` | info | `book`, `from_ms`, `to_ms`, `of_ms`, `words`, `took_ms`, `rtf` | each transcribed block; `rtf` = audio time / wall time |
| `sync.abort` | info | `book`, `from_ms` | Pause or Remove during a block |
| `sync.match` | info | `book`, `words`, `matched_pct`, `review`, `took_ms` | matching words to the book |
| `sync.error` | warn | `book`, `error` | a failed step |
| `sync.runner` | warn | `kind`, `error`, `repeat` (reported) | `core/recording.rs`, the job's thread |
| `sync.follow` | warn | `book`, `error` | `core/recording.rs` |
| `model.download` | info, warn | `model`, `bytes`, `took_ms`, `outcome`, `error` | `download_model` |
| **Preparing and speech** | | | |
| `prep.stage` | info | `book`, `cause`, `from`, `to` | `core/preparation.rs` `logged`, each transition |
| `prep.plan` | warn | `book`, `error` | `core.rs` |
| `prep.save` | warn | `kind`, `error`, `repeat` (reported) | `core/preparation.rs`, the queue or a book's done mark |
| `gen.request` | debug, warn | `book`, `index`, `reason`, `chars`, `audio_ms`, `took_ms`, `outcome`, `error` | `crates/generation` `run`, each synthesis |
| `gen.backoff` | info | `to`, `streak`, `wait_ms` | `crates/generation`, backing off or recovering |
| `gen.discarded` | debug | `book`, `index` | `crates/generation` `run`, a result whose audio was deleted while it was made |
| `gen.failed` | warn | `key`, `error` | `core.rs` |
| `engine.state` | info | `engine`, `from`, `to`, `detail` | `core.rs` `emit_engine_status` |
| `engine.voice` | warn | `engine`, `voice`, `error` | `core.rs`, a voice the engine lacks |
| `engine.session` | warn | `error` | `core.rs` |
| `narration.fallback` | warn | `error` | `core.rs` |
| `narration.speak_missing` | warn | `error` | `core/recording.rs` |
| `align.request` | debug | `key`, `took_ms`, `outcome` | `core/align.rs` |
| `align.offline` | info | `key` | `core/align.rs` |
| `align.error` | warn | `key`, `error` | `core/align.rs` |
| `align.settings` | warn | `error` | `core/align.rs` |
| **Playback and files** | | | |
| `player.status` | info | `book`, `from`, `to`, `chapter`, `source`, `error` | `core.rs` `tick_loop` |
| `output.start` | info, error | `outcome`, `latency_ms`, `error` | `core.rs`, the audio output |
| `output.restart` | warn | | `core.rs` `keep_output_alive` |
| `playback.cache` | warn | `key`, `error` | `core/playback.rs` |
| `feeder.read` | warn | `error` | `core/feeder.rs` |
| `cache.evict` | info | `segments`, `books` | `core/storage.rs` `enforce_cache_limit` |
| `export.done` | info, warn | `kind`, `book`, `file`, `took_ms`, `outcome`, `error` | `lib.rs` export commands |
| `proc.spawn` | info, error | `program`, `pid`, `outcome`, `error` | `core.rs` (engine), `core/align.rs` (aligner) |
| `proc.exit` | warn | `program`, `code`, `stderr_tail` | `core/export.rs`, `core/read_along.rs` (ffmpeg) |
| **Smoke runs** | | | |
| `smoke` | info, warn, error | `error` | `smoke.rs`; the message names the smoke library's own books |

**Reported failures.** `Core::report` (`core/error.rs` `Reporter`) logs a background failure at
warn with `kind` (the `CoreError` kind), `error` and `repeat`, and tells the user through
`core://problem` (an error toast). The same event is told once a minute at most; repeats are
logged with `repeat: true`. Besides the rows above that say "reported", these events come only
from it:

| Event | What failed | Where |
|---|---|---|
| `progress.save` | saving the listening position | `core.rs` `save_progress` |
| `library.save` | saving the books left out, or when a book was opened | `core.rs` |
| `listening.save` | saving listening time | `core/listening.rs` |
| `narration.save` | saving that a book plays its recording | `core/recording.rs` |
| `discovery.save` | forgetting found books no longer on disk | `core/discovery.rs` |
| `library.read`, `book.read`, `prep.read` | reading a stored value that does not parse (the default is used) | `core.rs`, `core/preparation.rs` |

`metadata.apply` and `audiobooks.pair` are also reported when saving a cover or unpairing fails.

Errors from the system (a file that cannot be read, a failed request) may name folders; the home
folder in them is written as `~`.
