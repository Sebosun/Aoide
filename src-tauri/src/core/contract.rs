//! The UI↔core contract, generated into `src/api/contract.ts`: TypeScript declarations of every
//! type the commands and events carry (from the Rust types), every event with its payload, and
//! every Tauri command in `commands/` with its arguments and result.
//!
//! The test fails when the file is out of date. Regenerate it with
//! `AOIDE_WRITE_CONTRACT=1 cargo test -p aoide contract`.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use ts_rs::{Config, TS};

use super::CoreEvent;

fn contract() -> String {
    let config = Config::new().with_large_int("number");
    let mut out = String::from(
        "// Generated from the Rust types by `AOIDE_WRITE_CONTRACT=1 cargo test -p aoide contract`.\n\
         // Do not edit: change the Rust type and regenerate.\n\n",
    );
    // Each declared type's TypeScript name by its Rust name, for the commands.
    let mut names: HashMap<String, String> = HashMap::new();
    macro_rules! declare {
        ($($ty:ty),* $(,)?) => {$(
            out.push_str("export ");
            out.push_str(&<$ty as TS>::decl(&config));
            out.push_str("\n\n");
            let rust = stringify!($ty).rsplit("::").next().unwrap_or_default().trim().to_owned();
            names.insert(rust, <$ty as TS>::ident(&config));
        )*};
    }
    declare!(
        // Library
        aoide_library::BookId,
        aoide_library::BookStatus,
        aoide_library::ItemKind,
        aoide_library::Collection,
        aoide_library::TagCount,
        super::BookProgressView,
        super::facets::AudioSource,
        super::ReadingMode,
        super::Shape,
        super::ReadState,
        super::BookSummary,
        super::facets::HomeView,
        super::ImportResult,
        super::AppInfo,
        // Errors
        super::error::CoreError,
        super::error::ProblemView,
        super::SpeechProblem,
        // Logging
        crate::logging::UiLevel,
        crate::logging::UiLog,
        // Preparation
        super::preparation::Stage,
        super::preparation::PrepareAction,
        super::preparation::PrepareView,
        // Reading
        aoide_epub::BlockKind,
        crate::book::NoteMarker,
        crate::book::SegmentView,
        crate::book::BlockView,
        crate::book::ChapterSummary,
        crate::book::ChapterView,
        crate::book::ExcludedView,
        crate::book::SearchHit,
        super::BookView,
        super::ChapterAudioView,
        // Playback and generation
        super::PlaybackStatus,
        super::SleepView,
        super::PlayerStateView,
        super::EngineState,
        super::engine_process::ServerState,
        super::engine_process::FailureKind,
        super::engine_process::StoppedKind,
        super::engine_process::ServerView,
        super::EngineStatusView,
        super::PreparingView,
        super::GenerationStateView,
        // Engines and voices
        aoide_tts::ParamKind,
        aoide_tts::ChoiceOption,
        aoide_tts::ParamValue,
        aoide_tts::ParamSpec,
        aoide_tts::EngineDescriptor,
        aoide_tts::NetworkScope,
        aoide_tts::LaunchCommand,
        aoide_tts::EngineProfile,
        aoide_tts::Voice,
        super::EngineOptions,
        super::BookSettings,
        // Bookmarks, stats, details
        aoide_library::Bookmark,
        aoide_library::DayListening,
        aoide_library::DayActivity,
        aoide_library::BookActivity,
        aoide_library::ActivityStats,
        aoide_library::BookDetails,
        aoide_metadata::Match,
        // Settings and storage
        super::settings::LibrarySort,
        super::settings::TagFilter,
        super::TaggingSettings,
        super::TaggingRun,
        super::settings::AlignerSettings,
        super::discovery::ScanSettings,
        super::settings::AppSettings,
        super::settings::AppSettingsPatch,
        super::storage::BookStorage,
        super::storage::StorageReport,
        super::storage::CleanResult,
        // Finding books
        super::discovery::CandidateStatus,
        super::discovery::Candidate,
        super::discovery::ImportChoice,
        super::discovery::ScanStage,
        super::discovery::ScanProgress,
        super::discovery::ImportProgress,
        // Finding audiobooks
        super::audiobooks::AudiobookSearchSettings,
        super::audiobooks::AudiobookSuggestion,
        super::audiobooks::MatchStage,
        super::audiobooks::AudiobookMatch,
        super::audiobooks::AudiobookChoice,
        super::audiobooks::PairOutcome,
        // Articles
        super::fetching::Reason,
        super::fetching::Stage,
        super::articles::ArticleSource,
        super::feeds::DiscoveredFeed,
        super::articles::ArticleView,
        super::articles::ArticleSummary,
        // Feeds
        super::feeds::FeedStage,
        super::feeds::FeedSettings,
        super::feeds::FeedView,
        super::feeds::SubscribeResult,
        super::feeds::AddedLink,
        super::feeds::OpmlImport,
        super::feeds::EntryFilter,
        super::feeds::EntryView,
        // The browser
        super::browsing::BrowserUse,
        super::browsing::BrowserSettings,
        // Appearance
        aoide_config::Mode,
        aoide_config::ReaderMode,
        aoide_config::Appearance,
        aoide_config::Theme,
        super::config::PreferencesView,
        // Export
        super::export::ExportStage,
        super::export::ExportProgress,
        // Recordings
        super::syncing::Stage,
        super::recording::Narration,
        super::recording::ModelSizeChoice,
        super::recording::RecognitionDevice,
        super::recording::RecognitionEngine,
        super::recording::RecognitionServer,
        super::recording::RecognitionSettings,
        super::recording::RecordingFileView,
        super::recording::RecordingProblemView,
        super::recording::InspectionView,
        super::recording::MatchSummaryView,
        super::syncing::RecordingAction,
        super::recording::RecordingView,
        super::recording::ChapterMatchView,
        super::recording::OffTextView,
        super::recording::SyncReportView,
        super::recording::SyncAction,
        super::recording::ModelView,
        super::recording::RecognitionOptions,
        super::recording::DownloadStage,
        super::recording::ModelDownloadView,
        super::browser_download::BrowserDownloadStage,
        super::browser_download::BrowserDownloadView,
        super::browsing::BrowserBinaryView,
        super::browsing::BrowserStatusView,
        aoide_config::SiteFetch,
        aoide_config::SiteData,
        super::sites::SiteSession,
        super::sites::SiteView,
        super::signin::SignInStage,
        super::signin::SignInView,
    );
    // Type aliases the commands use.
    names.insert("Params".into(), name::<aoide_tts::Params>(&config));
    // One line per `CoreEvent` variant.
    let events = [
        (CoreEvent::LIBRARY_CHANGED, "null".to_owned()),
        (
            CoreEvent::CONFIG_CHANGED,
            name::<super::config::PreferencesView>(&config),
        ),
        (
            CoreEvent::PLAYER_STATE,
            name::<super::PlayerStateView>(&config),
        ),
        (
            CoreEvent::ENGINE_STATUS,
            name::<super::EngineStatusView>(&config),
        ),
        (
            CoreEvent::GENERATION_STATE,
            name::<super::GenerationStateView>(&config),
        ),
        (
            CoreEvent::SCAN_PROGRESS,
            name::<super::discovery::ScanProgress>(&config),
        ),
        (CoreEvent::DISCOVERY_DETAILS, name::<String>(&config)),
        (
            CoreEvent::DISCOVERY_IMPORT,
            name::<super::discovery::ImportProgress>(&config),
        ),
        (
            CoreEvent::EXPORT_PROGRESS,
            name::<super::export::ExportProgress>(&config),
        ),
        (
            CoreEvent::SYNC_STATE,
            name::<super::recording::RecordingView>(&config),
        ),
        (
            CoreEvent::MODEL_DOWNLOAD,
            name::<super::recording::ModelDownloadView>(&config),
        ),
        (
            CoreEvent::AUDIOBOOK_SCAN,
            name::<super::discovery::ScanProgress>(&config),
        ),
        (
            CoreEvent::AUDIOBOOK_MATCHES,
            name::<Vec<super::audiobooks::AudiobookMatch>>(&config),
        ),
        (
            CoreEvent::PROBLEM,
            name::<super::error::ProblemView>(&config),
        ),
        (
            CoreEvent::ARTICLE_STATE,
            name::<super::articles::ArticleView>(&config),
        ),
        (CoreEvent::FEEDS_CHANGED, "null".to_owned()),
        (
            CoreEvent::SIGN_IN_STATE,
            name::<super::signin::SignInView>(&config),
        ),
        (
            CoreEvent::BROWSER_DOWNLOAD,
            name::<super::browser_download::BrowserDownloadView>(&config),
        ),
    ];
    out.push_str(
        "/** Every event Core emits, by name, with its payload. */\nexport type CoreEvents = {\n",
    );
    for (event, payload) in events {
        out.push_str(&format!("  \"{event}\": {payload},\n"));
    }
    out.push_str("};\n\n");
    out.push_str(&commands(&names));
    out
}

/// The Tauri commands in `commands/` as a TypeScript map from name to arguments and result,
/// and as the UI calls them: `CommandApi`, each command a method taking its arguments by name
/// (as one object, the one sent over IPC), and `COMMANDS`, each method's command.
fn commands(names: &HashMap<String, String>) -> String {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let parse = |path: &Path| syn::parse_file(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(src.join("commands"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    files.sort();
    files.insert(0, src.join("lib.rs"));
    let items: Vec<syn::Item> = files.iter().flat_map(|f| parse(f).items).collect();
    let mut registered = BTreeSet::new();
    let mut out = String::from(
        "/** Every command Core answers, by name, with its arguments and result. */\n\
         export type Commands = {\n",
    );
    let mut declared = BTreeSet::new();
    let mut methods = String::from(
        "\n/** Every command as the UI calls it: a method taking the command's arguments by name. */\n\
         export interface CommandApi {\n",
    );
    let mut table = String::from(
        "\n/** Each `CommandApi` method's command. */\n\
         export const COMMANDS = {\n",
    );
    for item in &items {
        let syn::Item::Fn(function) = item else {
            continue;
        };
        if function.sig.ident == "run" {
            registered.extend(handler_names(function));
        }
        let command = function.attrs.iter().any(|a| {
            let path = a.path();
            path.segments.len() == 2 && path.segments[1].ident == "command"
        });
        if !command {
            continue;
        }
        let name = function.sig.ident.to_string();
        let doc = jsdoc(&function.attrs);
        let mut args = Vec::new();

        for input in &function.sig.inputs {
            let syn::FnArg::Typed(arg) = input else {
                continue;
            };
            let syn::Pat::Ident(ident) = &*arg.pat else {
                continue;
            };
            let injected = ["CoreState", "State", "AppHandle", "Window", "WebviewWindow"];
            if injected.contains(&last_ident(&arg.ty).as_str()) {
                continue;
            }
            let ty = typescript(&arg.ty, names).unwrap_or_else(|e| panic!("{name}: {e}"));
            let arg = camel(&ident.ident.to_string());
            // An `Option` may be left out.
            let optional = if ty.ends_with(" | null") { "?" } else { "" };
            args.push(format!("{arg}{optional}: {ty}"));
        }
        let result = match &function.sig.output {
            syn::ReturnType::Default => "void".to_owned(),
            syn::ReturnType::Type(_, ty) => {
                typescript(ty, names).unwrap_or_else(|e| panic!("{name}: {e}"))
            }
        };
        let all_optional = args.iter().all(|a| a.contains("?: "));
        let param = match args.is_empty() {
            true => String::new(),
            false if all_optional => format!("args?: {{ {} }}", args.join("; ")),
            false => format!("args: {{ {} }}", args.join("; ")),
        };
        let args = if args.is_empty() {
            "Record<string, never>".to_owned()
        } else {
            format!("{{ {} }}", args.join("; "))
        };
        out.push_str(&format!(
            "{doc}  \"{name}\": {{ args: {args}; result: {result} }};\n"
        ));
        let method = camel(&name);
        methods.push_str(&format!("{doc}  {method}({param}): Promise<{result}>;\n"));
        table.push_str(&format!("  {method}: \"{name}\",\n"));
        declared.insert(name);
    }
    out.push_str("};\n");
    out.push_str(&methods);
    out.push_str("}\n");
    out.push_str(&table);
    out.push_str("} as const satisfies Record<keyof CommandApi, keyof Commands>;\n");
    assert_eq!(
        declared, registered,
        "every #[tauri::command] in commands/ is registered in generate_handler!, and only those"
    );
    out
}

/// A function's doc comment as an indented JSDoc block; empty when it has none.
fn jsdoc(attrs: &[syn::Attribute]) -> String {
    let lines: Vec<String> = attrs
        .iter()
        .filter_map(|a| match &a.meta {
            syn::Meta::NameValue(nv) if nv.path.is_ident("doc") => match &nv.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(text),
                    ..
                }) => Some(text.value().trim().to_owned()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    match lines.as_slice() {
        [] => String::new(),
        [one] => format!("  /** {one} */\n"),
        _ => format!(
            "  /**\n{}   */\n",
            lines
                .iter()
                .map(|l| format!("   * {l}\n"))
                .collect::<String>()
        ),
    }
}

/// The commands named in `run`'s `generate_handler!`.
fn handler_names(run: &syn::ItemFn) -> BTreeSet<String> {
    let text = quote_tokens(run);
    let start = text
        .find("generate_handler !")
        .expect("run registers the commands");
    let list = &text[start..];
    let list = &list[list.find('[').unwrap() + 1..list.find(']').unwrap()];
    list.split(',')
        .map(|n| n.rsplit("::").next().unwrap_or(n).trim().to_owned())
        .filter(|n| !n.is_empty())
        .collect()
}

fn quote_tokens(run: &syn::ItemFn) -> String {
    use quote::ToTokens;
    run.to_token_stream().to_string()
}

fn last_ident(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default(),
        syn::Type::Reference(reference) => last_ident(&reference.elem),
        _ => String::new(),
    }
}

/// `book_id` → `bookId`, as Tauri names command arguments.
fn camel(name: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for c in name.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// The TypeScript for a Rust type a command takes or returns.
fn typescript(ty: &syn::Type, names: &HashMap<String, String>) -> Result<String, String> {
    let generic = |segment: &syn::PathSegment, n: usize| -> Result<String, String> {
        let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
            return Err(format!("{} without type arguments", segment.ident));
        };
        let types: Vec<&syn::Type> = args
            .args
            .iter()
            .filter_map(|a| match a {
                syn::GenericArgument::Type(t) => Some(t),
                _ => None,
            })
            .collect();
        typescript(types.get(n).ok_or("missing type argument")?, names)
    };
    match ty {
        syn::Type::Reference(reference) => typescript(&reference.elem, names),
        syn::Type::Tuple(tuple) if tuple.elems.is_empty() => Ok("void".into()),
        syn::Type::Path(path) => {
            let segment = path.path.segments.last().ok_or("empty path")?;
            let ident = segment.ident.to_string();
            Ok(match ident.as_str() {
                "String" | "str" | "PathBuf" => "string".into(),
                "bool" => "boolean".into(),
                "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize"
                | "f32" | "f64" => "number".into(),
                "Result" => generic(segment, 0)?,
                "Option" => format!("{} | null", generic(segment, 0)?),
                "Vec" => {
                    let item = generic(segment, 0)?;
                    if item.contains(' ') {
                        format!("({item})[]")
                    } else {
                        format!("{item}[]")
                    }
                }
                "HashMap" | "BTreeMap" => {
                    format!("Record<{}, {}>", generic(segment, 0)?, generic(segment, 1)?)
                }
                // The raw bytes of a binary answer.
                "Response" => "ArrayBuffer".into(),
                other => names
                    .get(other)
                    .cloned()
                    .ok_or_else(|| format!("{other} is not declared in core/contract.rs"))?,
            })
        }
        _ => Err(format!("unsupported type {}", quote_type(ty))),
    }
}

fn quote_type(ty: &syn::Type) -> String {
    use quote::ToTokens;
    ty.to_token_stream().to_string()
}

fn name<T: TS>(config: &Config) -> String {
    T::name(config)
}

#[test]
fn the_typescript_contract_matches_the_rust_types() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/api/contract.ts");
    let text = contract();
    if std::env::var_os("AOIDE_WRITE_CONTRACT").is_some() {
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        current == text,
        "src/api/contract.ts is out of date: run `AOIDE_WRITE_CONTRACT=1 cargo test -p aoide contract`"
    );
}
