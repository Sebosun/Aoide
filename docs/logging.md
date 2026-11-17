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
UTC.

The speech server Aoide starts writes what it prints to `speech-server.log` in the same folder
(the run before is `speech-server.previous.log`; each stops at 10 MB), and an aligner Aoide
starts to `aligner.log`. That output may contain book text, so it is the user's to read and is
never copied into the JSON log: events name it only as `file`. `AOIDE_LOG` sets what the file keeps (EnvFilter syntax; default `warn,aoide=debug`, so
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
| Smoothness | `rg '"event":"ui.frames"' $F \| jq -r '[.ts[11:19],.command,.view,.frames,.dropped,.worst_ms,.took_ms]\|@tsv' \| tail -n 30` |
| Sync speed | `rg '"event":"sync.block"' $F \| jq -r '[.ts[11:19],.book,.rtf,.took_ms]\|@tsv' \| tail -n 20` |
| Dependency noise | `rg -v '"event":' $F \| jq -r '.target' \| sort \| uniq -c \| sort -rn \| head -20` |

Older days: replace `$F` with `$L/aoide.2026-11-14.jsonl`, or `$L/aoide.*.jsonl` for all.

## Events

One row per event name; adding an event adds its row here. Paths are under `src-tauri/src/`
unless they name a crate. Lines from the window carry `target` `aoide::ui`.

| Event | Level | Fields | Where |
|---|---|---|---|
| **App** | | | |
| `app.start` | info | `version`, `profile`, `gpu`, `os`, `smoke` | `lib.rs` `start` |
| `app.exit` | info | | `lib.rs` exit |
| `app.signal` | info, warn | `signal` (`interrupt`, `terminate`), `outcome` | `lib.rs` `exit_on_signals`: Ctrl+C or SIGTERM quits through `app.exit`, so the servers Aoide started are stopped |
| `launch.desktop_file` | info, warn | `step` (`desktop_file`, `icon`, `update-desktop-database`, `xdg-mime`), `outcome` (`ok`, `failed`), `file` (name only), `error` | `desktop_file.rs` `install`, one line per step of `aoide --install-desktop-file`; a tool that cannot be run is reported and the rest carries on |
| `launch.request` | info | `kind` (`flag`, `link`, `save`, `file`, `missing`, `unsupported`), `outcome` (`ignored`; for a book file `present`: opened from the library, nothing copied, or `imported`), `file` (a file's name only; never an address), `book` | `launch.rs` `requests`, an argument of a start (or of a second start, handed to the running copy) that asks for nothing; `core/launching.rs` `Launching::act`, a request acted on |
| `launch.save_link` | info, warn | `outcome` (`saved`, `failed`, `dropped`), `new` (it was not in the library before), `book`, `why` (`error`; when dropped `too_soon`: less than a second after the last, `too_many`: twenty held while the window starts, saved or not), `error` | `core/launching.rs` `Launching::save_link`, a page sent from the browser (`aoide://save?url=…`); never its address (the article's own events carry the book) |
| `launch.second_instance` | info, warn | `arguments` (how many), `outcome` (`early`: it came before the core had started, and was dropped) | `lib.rs` `second_start`: another start of Aoide handed its arguments to this one, whose window comes to the front |
| `launch.window` | info | `from`, `to` (`waiting`, `ready`) | `core/launching.rs` `Launching::apply`: the window took what was held for it (`take_open_requests`) and is sent items to open from then on |
| `app.panic` | error | `location`, `error` | `logging.rs` panic hook |
| `app.migrate` | info | | `lib.rs` `migrate_from_folio` |
| `app.log` | warn | `error` | `logging.rs`, the file cannot be written |
| `app.emit` | error | `name`, `error` | `core.rs`, an event the window did not get |
| `app.poisoned` | error | `error` (reported) | `core.rs` `lock`, the first lock a panic poisoned |
| `config.saved` | info | `section`, `changed` (setting names) | `core/config.rs` `Settings::update_app`, a change from the Settings window |
| `config.edited` | info | `section` (`app`), `changed` (setting names) | `core/config.rs` `Settings::apply_edits`, a hand edit to `settings.toml` that passed the checks |
| `config.rejected` | warn | `section` (`app`), `error` | `core/config.rs`, a hand-edited `[app]` that failed the checks; the settings in use stay |
| `config.save` | warn | `kind`, `error`, `repeat` (reported); `section`, `outcome` from the first-run migration | `core.rs` (player), `core/config.rs` (migration) |
| `config.problem` | warn | `error` | `core.rs`, a settings file Aoide cannot read |
| `config.watch` | warn | `error` | `core/config.rs`, watching settings files |
| `config.reload` | info | `file` (`site-rules.toml`) | `core/browsing.rs` `reload_rules`, a hand edit read again (a broken file logs `config.problem` and the old rules stay) |
| **Window** | | | |
| `ipc.error` | warn | `command`, `took_ms` (`msg` is the error) | `src/api/tauri.ts` `call`; errors of kind `cancelled` (a scan the user stopped) left out |
| `ipc.slow` | info | `command`, `took_ms` | `src/api/tauri.ts`, answers over 2 s (long jobs left out) |
| `ui.error` | error | `view` | `src/main.ts`, an error no view caught |
| `ui.selection` | info | `command` (the entry: `copy`, `highlight`, `search`, `listen`, `say-differently`, `say-again`, `bookmark`), `view` | `src/components/reader/ReaderSelectionMenu.vue`, an entry of the menu on selected text or on a sentence; never the text |
| `ui.frames` | debug | `command` (`scroll`, `open`, `view`), `view`, `frames`, `dropped` (over 24 ms), `worst_ms`, `took_ms` | `src/composables/useFrameLog.ts`: each scroll (`took_ms` and `frames` include the 300 ms it takes to count as ended), and each change of view until it has come in; an `open` counts from the click |
| **Library** | | | |
| `library.scan` | info, warn, error | `books`, `new`, `read`, `took_ms`, `outcome`, `error` | `core.rs` `rescan` |
| `tags.classify` | info / warn | `book`, `tags` (how many came back), `error`, `took_ms`, `outcome` | `core/tagging.rs` |
| `tags.classify_all` | info | `tagged`, `missed` (items given nothing or that failed), `took_ms`, `outcome` | `core/tagging.rs` |
| `library.import` | info, warn | `ok`, `failed`; per failure `file`, `error` | `core.rs` `import_files` |
| `library.watch` | warn | `error` | `core.rs`, the library folder watcher |
| `inbox.imported` | info | `book`, `file` (name only) | `core/inbox.rs` `take`, a book file in the inbox folder copied into the library; the window is told what one look added (`inbox://added`) |
| `inbox.skipped` | info | `file` (name only), `why` (`in_library`: its content is there already, `removed`: the user removed that book, `unreadable`, `failed`: it could not be added), `error` | `core/inbox.rs` `take`; a file passed over is not looked at again until it changes |
| `inbox.unavailable` | warn | `error` | `core/inbox.rs` `Inbox::lost`: the inbox folder cannot be listed or watched (gone, or on a drive taken out); it is looked for every 30 s. Never its path |
| `inbox.stage` | info | `from`, `to` (`off`, `idle`, `settling`, `importing`, `unavailable`) | `core/inbox.rs` `Inbox::apply`, where the inbox's `next` is applied |
| `discovery.scan` | info, warn | `roots`, `epubs`, `read`, `took_ms`, `outcome`, `error` | `core/discovery.rs` |
| `discovery.import` | info | `chosen`, `failed` | `core/discovery.rs` |
| `library.audiobook` | info | `book`, `outcome` (`ok`, `refused`), `error` | `core/audiobook.rs` `add_audiobook` |
| `cover.thumbnail` | info / warn | `file`, `from` and `to` (widths), `took_ms`, `outcome` (`ok`, `failed`), `error` | `core/item.rs` `shown_cover`: a cover wider than 480 px gets a thumbnail to be shown from, once, and again when the cover changes |
| `library.merge` | info | `book`, `from` (the audiobook), `outcome` (`ok`, `failed`), `error` | `core/audiobook.rs` `merge_audiobook` |
| `progress.pending` | info / warn | `book`, `outcome` (`settled` into progress once the sync reached it, `dropped` when read or played first), `error` | `core/audiobook.rs` |
| `article.save` | info | `book`, `outcome` (`ok`) | `core/articles.rs` `save` |
| `article.stage` | info | `book`, `from`, `to`, `reason` (when it needs the user) | `core/articles.rs`, where the fetch stage's `next` is applied |
| `article.fetch` | info | `book`, `outcome` (`ok`, `failed`), `took_ms` | `core/articles.rs` `fetch` |
| `article.forget` | warn | `book`, `error` (its folder could not be deleted) | `core/articles.rs` `forget` |
| `feed.subscribe` | info | `feed` (its id), `outcome` | `core/feeds.rs` `subscribe_feed` |
| `feed.unsubscribe` | info | `feed`, `orphans` (items only its entries opened, deleted with it) | `core/feeds.rs` `unsubscribe` |
| `feed.check` | info | `feed`, `outcome` (`ok`, `not_modified`, `failed`), `took_ms` | `core/feeds.rs` `check`: one per feed checked |
| `item.archived` | info | `book`, `on` (archived, or taken out), `from` (`user`; `finished`: the setting archived an article just finished) | `core/shelf.rs` `Shelf::archive`, only when it changed |
| `item.shelf` | info | `book`, `from`, `to` (`library`, `feed`) | `core/feeds.rs` `move_shelf`: an entry saved, or removed from the library while its feed lists it |
| `feed.import` | info | `added`, `skipped` (feeds) | `core/feeds.rs` `import` (OPML) |
| `feed.export` | info | `feeds` | `core/feeds.rs` `export` (OPML) |
| **Browser** (optional, off by default) | | | |
| `browser.start` | info | `visible`, `outcome` (`ok`, `failed`), `error` | `core/browsing.rs` `ChromeDriver`, a browser process started (headless to render, visible to sign in) |
| `browser.render` | info | `outcome` (`ok`, `failed`), `took_ms`, `error` | `core/browsing.rs` `render`, one page rendered for an article |
| `browser.stop` | info | `outcome` (`idle`, `off` when turned off, `removed`, `cleared`, `quit`, `failed`) | `core/browsing.rs` `idle` / `stop` |
| `browser.download` | info | `version`, `resumed_from` (bytes kept from an earlier try), `outcome` (`ok`, `cancelled`, `failed`), `took_ms`, `error` | `core/browser_download.rs` `download` |
| `browser.download.stage` | info | `from`, `to` | `core/browser_download.rs` `step`, where the download stage's `next` is applied |
| `browser.remove` | info | | `core/browser_download.rs` `remove`, the downloaded browser deleted |
| `browser.profile` | info | `outcome` (`unlocked`: a lock a crashed browser left; `cleared`: all browser data removed) | `crates/browser` `Browser::launch`; `core/browsing.rs` `clear_profile` |
| `signin.start` | info | `book` | `core/signin.rs` `begin`, a sign-in window asked for |
| `signin.stage` | info | `book`, `from`, `to` | `core/signin.rs` `step`, where the sign-in's `next` is applied |
| `signin.clear` | warn | `error` (the site could not be cleared after a sign-in without "stay signed in") | `core/signin.rs` `run` |
| `site.rule` | info | (no host: which sites are read is not logged) | `core/sites.rs` `set_site_rule`, written to site-rules.toml |
| `site.sign_out` | info | (no host: which sites are read is not logged) | `core/sites.rs` `sign_out_site`, its browser data cleared |
| `site.forget` | info | (no host: which sites are read is not logged) | `core/sites.rs` `forget_site`, its data, row and rule dropped |
| `browser.clear` | debug | `error` (an origin's storage that could not be cleared) | `crates/browser` `clear_site` |
| `browser.proxy` | debug | `error` (a proxied connection that ended badly) | `crates/browser` `proxy.rs` |
| `feed.clean` | info / warn | `removed` (entries), `orphans` (items no entry needs), `book`, `error` | `core/feeds.rs` `clean`, daily and after Check now |
| `feed.stage` | info | `feed`, `from`, `to` (`idle`, `checking`, `failing`) | `core/feeds.rs` `check`: a feed starts or stops failing |
| `article.cover` | info / warn | `book`, `outcome` (`ok`, `failed`), `media_type` (an image it would not keep) or `error` | `core/articles.rs` `keep_cover`: the page's lead image stored as the cover |
| `library.word_count` | warn | `book`, `error` | `core/shelf.rs` `load`, storing a parsed book's length |
| `reading.mode` | info | `book`, `from`, `to` | `core/reading.rs` `choose_mode`, a book switched between read and listen |
| `reading.state` | info | `book`, `from`, `to` | `core/reading.rs` `Reading::apply`, unread / in progress / finished |
| `reading.save` | warn | `kind`, `error`, `repeat` (reported) | `core/reading.rs`, storing reading time |
| `media.start` | warn | `error` | `media/mpris.rs` `spawn`, when the session bus is unavailable (no media keys) |
| `media.update` | warn | `error` | `media/mpris.rs`, telling the desktop what plays |
| `metadata.lookup` | warn | `book`, `outcome`, `error` | `core/discovery.rs` |
| `metadata.apply` | warn | `book`, `error` | `core/discovery.rs` |
| `http` | debug, warn | `service`, `op`, `found`, `took_ms`, `outcome`, `error` | `core/metadata.rs` (Open Library) |
| **Audiobooks** | | | |
| `audiobooks.scan` | info | `roots`, `only_added`, `dirs`, `files`, `audio`, `audiobooks`, `doubtful`, `dropped` (finds heard: kept as audiobooks, kept as doubtful, dropped as music or clips), `matches`, `unmatched` (whole searches), `took_ms`, `outcome` | `core/audiobooks.rs` |
| `audiobooks.pair` | info, warn | `book`, `candidate`, `outcome` (`ok`, `failed`, `skipped`: another book has it), `error` | `core/audiobooks.rs` |
| `match.stage` | info | `book`, `cause`, `from`, `to` | `core/audiobooks.rs`, each match transition |
| **Sync** (`core/recordings/`) | | | |
| `sync.stage` | info | `book`, `cause`, `from`, `to`, `took_ms` | `sync_event`, each transition |
| `sync.work` | debug | `kind`, `book` | each piece of work the job picks |
| `sync.transcriber` | info | `engine`, `model`, `gpu`, `threads`, `took_ms` | a transcriber loaded |
| `sync.removed` | info | `book` | `Recordings::remove`, a recording removed or unpaired (its files stay) |
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
| `speech.say_again` | info, warn | `book`, `segment`, `take`, `took_ms`, `outcome` (`ok`, `failed`), `error` | `core/takes.rs` `say_again`, one sentence's audio thrown away to be made again |
| `pronunciation.saved` | info | `book` or `scope` (`all`), `affected` | `core/pronunciation.rs` `save_pronunciation`; `affected` is the sentences said differently now (in the book, or for all books in the book being listened to and the prepared ones). Never the words |
| `pronunciation.removed` | info | `book` or `scope` (`all`), `affected` | `core/pronunciation.rs` `delete_pronunciation`, as `pronunciation.saved` |
| `highlight.added` | info | `book`, `chapter`, `chars`, `from` (`selection` or `spoken`) | `core/highlights.rs` `add`; `chars` is the passage's length after joining the highlights it overlaps or touches. Never the text or the note |
| `highlight.detached` | info | `book`, `chapter` | `core/highlights.rs` `list`, each time a book's highlights are listed and one's text is no longer in its chapter (an article fetched again, newer extraction rules); it is listed, not drawn. A highlight whose text moved is moved with it, without a line |
| `highlight.removed` | info | `book`, `chapter` | `core/highlights.rs` `delete_highlight` |
| `notes.exported` | info | `book`, `highlights`, `bookmarks`, `file` | `core/notes.rs`, a book's notes written as Markdown (`export_notes`, and each book of `export_all_notes`); `file` is the file's name only |
| `gen.failed` | warn | `key`, `error` (the whole error; the player shows a short message, and Settings › Speech engine › Problems lists both) | `core.rs` |
| `engine.state` | info | `engine`, `from`, `to`, `detail` | `core/engine.rs` `publish` |
| `engine.server.stage` | info (warn into `failed`, `waiting`) | `program` (`engine`, `aligner`), `from`, `to`, `cause` (the event), `attempt`, `failure` (kind) | `core/engine_process.rs` `step`, where `next` is applied |
| `engine.server.stop` | info | `program`, `why` (`idle`, `switched`, `timeout`, `asked`, `quit`, `leftover`), `outcome` (`terminated`, `killed` after the grace period), `took_ms` | `core/engine_process.rs`, a server stopped |
| `engine.server.reclaim` | warn | `program`, `pid`, `outcome` (`stopped`: a server an earlier run lost in a crash was stopped; `stale`: the pidfile named another program by now; `unwritten`: the pidfile could not be written), `error` | `core/engine_process.rs` `reclaim`, at start, and `launch` |
| `engine.server.log` | warn | `program`, `file`, `error` (the server log cannot be written) | `core/engine_process.rs` |
| `engine.voice` | warn | `engine`, `voice`, `error` | `core.rs`, a voice the engine lacks |
| `engine.session` | warn | `error` | `core.rs` |
| `narration.fallback` | warn | `error` | `core.rs` |
| `narration.speak_missing` | warn | `error` | `core/recording.rs` |
| `narration.left_out` | info | `book`, `chapters` (how many are left out now) | `core/leaving_out.rs` `set_left_out`, one line per change |
| `align.request` | debug | `key`, `took_ms`, `outcome` | `core/align.rs` |
| `align.offline` | info | `key` | `core/align.rs` |
| `align.error` | warn | `key`, `error` | `core/align.rs` |
| `align.settings` | warn | `error` | `core/align.rs` |
| `align.rules` | warn | `book`, `error` | `core/align.rs`, the pronunciations could not be read; the words are aligned as written |
| **Playback and files** | | | |
| `player.status` | info | `book`, `from`, `to`, `chapter`, `source`, `error` | `core.rs` `tick_loop` |
| `output.start` | info, error | `outcome`, `latency_ms`, `error` | `core.rs`, the audio output |
| `output.restart` | warn | | `core.rs` `keep_output_alive` |
| `playback.book_speed` | info | `book`, `on` | `core/playback.rs` `Core::set_book_speed`, when a book's own playback speed is turned on or off |
| `playback.cache` | warn | `key`, `error` | `core/playback.rs` |
| `playback.resumed` | info | `book`, `away_s`, `stepped_back` (a number of `unit`: `entries`, 0 being the sentence's start, or `ms` for an audiobook alone), `unit`, `from` (`pause`, or `saved`: the saved place's time) | `core/playback.rs` `step_back_in`, `core/timeline.rs` `step_back_on`, one line per resume that steps back |
| `playback.skipped_left_out` | info | `book`, `from`, `to` (chapter indexes; `to` is the chapter count when the book ended) | `core/playback.rs` `tick`, one line each time playback passes over chapters left out |
| `sleep.set` | info | `minutes`, `end_of_chapter` (neither: cancelled) | `core/controls.rs` `Controls::sleep`, where `next` is applied, the timer set or cancelled |
| `sleep.rang` | info | `minutes`, `end_of_chapter` | `core/controls.rs` `Controls::sleep`, the timer paused playback (or the listener paused while it faded) |
| `sleep.extended` | info | `minutes`, `end_of_chapter` | `core/controls.rs` `Controls::sleep`, playing within 3 minutes of a ring started the same timer again |
| `queue.changed` | info | `size` | `core/queue.rs` `Queue`, an item added to, moved in or taken off the queue (one taken off as it starts too) |
| `queue.handover` | info, warn | `from`, `to`, `outcome` (`waiting`: the item ended, the next plays after the gap; `cancelled`: the listener did something in the gap; `played`; `failed`, with `error`: that item could not play, the next is tried; `nothing_to_play`) | `core/queue.rs`, where `next` is applied and `Queue::hand_over` |
| `queue.passed_over` | info | `book`, `why` (`fetching`, `notFetched`, `missing`, `noVoice`) | `core/queue.rs` `Queue::hand_over`, a queued item that cannot be listened to now stays queued and the next is tried |
| `feeder.read` | warn | `error` | `core/feeder.rs` |
| `cache.evict` | info | `segments`, `books` | `core/storage.rs` `enforce_cache_limit` |
| `export.done` | info, warn | `kind`, `book`, `file`, `took_ms`, `outcome`, `error` | `core/export.rs` (`Exports`) |
| `notice.sent` | info, warn | `kind` (`prepared`, `preparationStalled`, `synced`, `syncNeedsYou`, `syncFailed`, `exported`, `exportFailed`, `importFinished`), `book`, `outcome` (`sent`, `failed`: logged once per run, as the desktop may have no notification service), `error` | `core/notices.rs` `Notices::happened`, a desktop notification sent (never its text: it names the book) |
| `proc.spawn` | info, warn, error | `program`, `pid`, `why` (`wanted`, `restart`, `idle_wake`, `retry`), `outcome` (`ok`, `address_taken`, `failed`), `error` | `core/engine_process.rs` (engine, aligner) |
| `proc.exit` | warn | `program`, `code`, `stderr_tail` (ffmpeg); for `engine` and `aligner`: `pid`, `code`, `signal`, `ran_ms`, `file` and no `stderr_tail` | `core/export.rs`, `core/read_along.rs` (ffmpeg), `core/engine_process.rs` (the reaper thread) |
| **Smoke runs** | | | |
| `smoke` | info, warn, error | `error` | `smoke.rs`; the message names the smoke library's own books |

**Reported failures.** `Core::report` (`core/error.rs` `Reporter`) logs a background failure at
warn with `kind` (the `CoreError` kind), `error` and `repeat`, and tells the user through
`core://problem` (an error toast). The same event is told once a minute at most; repeats are
logged with `repeat: true`. Besides the rows above that say "reported", these events come only
from it:

| Event | What failed | Where |
|---|---|---|
| `launch.open` | adding a book file opened from the file manager or the command line (the message names the file), or saving a page sent from the browser; one that fails while the window starts is told when it listens | `core/launching.rs` `Launching::act`, `take_open_requests` |
| `progress.save` | saving the listening position | `core.rs` `save_progress` |
| `library.save` | saving the books left out, when a book was opened, or a book's own playback speed | `core.rs`, `core/playback.rs` `Core::set_speed` |
| `listening.save` | saving listening time | `core/listening.rs` |
| `narration.save` | saving that a book plays its recording | `core/recording.rs` |
| `playback.restart` | playing on in the book listened to after its pauses changed | `core.rs` `text_laid_out_again` |
| `discovery.save` | forgetting found books no longer on disk | `core/discovery.rs` |
| `library.read`, `book.read`, `prep.read` | reading a stored value that does not parse (the default is used) | `core.rs`, `core/preparation.rs` |

`metadata.apply` and `audiobooks.pair` are also reported when saving a cover or unpairing fails.

Errors from the system (a file that cannot be read, a failed request) may name folders; the home
folder in them is written as `~`.
