#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if let Some(code) = aoide_lib::install_desktop_file_if_asked(&args) {
        std::process::exit(code);
    }
    aoide_lib::run();
}
