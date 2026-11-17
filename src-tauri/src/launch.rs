//! What a start of Aoide asks for: the command-line arguments of a cold start, or those a
//! second start hands to the running copy. [`requests`] is the rule, pure but for looking at
//! which files exist.

use std::path::{Path, PathBuf};

use url::Url;

/// What one argument asks the running Aoide to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Open this book file (an EPUB or a PDF), adding it to the library if it is new.
    OpenFile(PathBuf),
    /// Save this web page (`http(s)://…`, or `aoide://save?url=…`).
    SaveLink(Url),
    /// Nothing asked for: just show the window.
    Show,
}

/// The arguments after the program name in `argv` (as `std::env::args` and a second start
/// give it).
pub fn arguments(argv: &[String]) -> &[String] {
    argv.get(1..).unwrap_or_default()
}

/// The requests in `args`, the arguments after the program name, with relative paths resolved
/// against `cwd`. Options are never interpreted: an argument starting with `-` is ignored, so
/// nothing a web page or a file name supplies can become one.
pub fn requests(args: &[String], cwd: &Path) -> Vec<Request> {
    if args.is_empty() {
        return vec![Request::Show];
    }
    args.iter()
        .filter_map(|arg| {
            let request = request(arg, cwd);
            if let Err((kind, file)) = &request {
                tracing::info!(
                    event = "launch.request",
                    kind,
                    outcome = "ignored",
                    file = file.as_deref().unwrap_or_default(),
                    "Ignored an argument"
                );
            }
            request.ok()
        })
        .collect()
}

/// Why an argument asks for nothing: its kind, and the file's name if it names a file.
type Ignored = (&'static str, Option<String>);

fn request(arg: &str, cwd: &Path) -> Result<Request, Ignored> {
    if arg.starts_with('-') {
        return Err(("flag", None));
    }
    let lower = arg.get(..8).unwrap_or(arg).to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return web_address(arg)
            .map(Request::SaveLink)
            .ok_or(("link", None));
    }
    if lower.starts_with("aoide:") {
        return save_address(arg)
            .map(Request::SaveLink)
            .ok_or(("save", None));
    }
    let path = if lower.starts_with("file:") {
        Url::parse(arg)
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .ok_or(("file", None))?
    } else if arg.is_empty() || arg.contains("://") {
        return Err(("unsupported", None));
    } else {
        cwd.join(arg)
    };
    let name = || Some(crate::logging::file(&path).into_owned());
    if !path.is_file() {
        return Err(("missing", name()));
    }
    if !aoide_library::is_book_file(&path) {
        // Audio among them: adding an audiobook stays in the Add dialog.
        return Err(("file", name()));
    }
    Ok(Request::OpenFile(path))
}

/// `address` if it is an `http` or `https` address with a host.
fn web_address(address: &str) -> Option<Url> {
    let url = Url::parse(address).ok()?;
    (matches!(url.scheme(), "http" | "https") && url.host_str().is_some_and(|h| !h.is_empty()))
        .then_some(url)
}

/// The web address in `aoide://save?url=…`; any other `aoide:` address asks for nothing.
fn save_address(address: &str) -> Option<Url> {
    let url = Url::parse(address).ok()?;
    if url.scheme() != "aoide" || url.host_str() != Some("save") {
        return None;
    }
    let (_, inner) = url.query_pairs().find(|(key, _)| key == "url")?;
    web_address(&inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn link(address: &str) -> Request {
        Request::SaveLink(Url::parse(address).unwrap())
    }

    #[test]
    fn no_arguments_shows_the_window() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(requests(&[], dir.path()), [Request::Show]);
    }

    #[test]
    fn book_files_open_by_absolute_relative_or_file_address() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Ferry Tales")).unwrap();
        for name in ["Quay.epub", "Ferry Tales/Lamp #2.PDF"] {
            std::fs::write(dir.path().join(name), b"book").unwrap();
        }
        let quay = dir.path().join("Quay.epub");
        let lamp = dir.path().join("Ferry Tales/Lamp #2.PDF");
        let lamp_address = Url::from_file_path(&lamp).unwrap().to_string();
        assert!(lamp_address.contains("%20"), "{lamp_address}");

        let found = requests(
            &args(&[
                quay.to_str().unwrap(),
                "Quay.epub",
                "./Ferry Tales/Lamp #2.PDF",
                &lamp_address,
            ]),
            dir.path(),
        );

        assert_eq!(
            found,
            [
                Request::OpenFile(quay.clone()),
                Request::OpenFile(quay),
                Request::OpenFile(dir.path().join("./Ferry Tales/Lamp #2.PDF")),
                Request::OpenFile(lamp),
            ]
        );
    }

    #[test]
    fn web_addresses_and_save_addresses_are_links_to_save() {
        let dir = tempfile::tempdir().unwrap();
        let found = requests(
            &args(&[
                "https://example.org/quay?lamp=1",
                "HTTP://example.org/ferry",
                "aoide://save?url=https%3A%2F%2Fexample.org%2Fnets%3Fa%3D1%26b%3D2",
            ]),
            dir.path(),
        );
        assert_eq!(
            found,
            [
                link("https://example.org/quay?lamp=1"),
                link("http://example.org/ferry"),
                link("https://example.org/nets?a=1&b=2"),
            ]
        );
    }

    #[test]
    fn bad_save_addresses_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let found = requests(
            &args(&[
                "aoide://save",
                "aoide://save?url=",
                "aoide://save?url=javascript%3Aalert(1)",
                "aoide://save?url=file%3A%2F%2F%2Fetc%2Fpasswd",
                "aoide://save?url=aoide%3A%2F%2Fsave%3Furl%3Dhttps%253A%252F%252Fexample.org",
                "aoide://save?url=example.org",
                "aoide://delete?url=https%3A%2F%2Fexample.org",
                "https://",
                "ftp://example.org/quay.epub",
            ]),
            dir.path(),
        );
        assert_eq!(found, []);
    }

    #[test]
    fn flags_other_files_and_missing_files_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "--install-desktop-file",
            "-quay.epub",
            "Song.mp3",
            "Notes.txt",
        ] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        std::fs::create_dir(dir.path().join("Folder.epub")).unwrap();
        let found = requests(
            &args(&[
                "--install-desktop-file",
                "-quay.epub",
                "Song.mp3",
                "Notes.txt",
                "Folder.epub",
                "Gone.epub",
                "file:///nowhere/Gone.epub",
                "",
            ]),
            dir.path(),
        );
        assert_eq!(found, []);
    }

    #[test]
    fn the_program_name_is_not_an_argument() {
        // Both callers get `argv` with the program first and drop it: `aoide` alone shows the
        // window, it does not ask to open a file called `aoide`.
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("aoide.epub");
        std::fs::write(&program, b"x").unwrap();
        let argv = args(&[program.to_str().unwrap()]);
        assert_eq!(requests(arguments(&argv), dir.path()), [Request::Show]);
    }
}
