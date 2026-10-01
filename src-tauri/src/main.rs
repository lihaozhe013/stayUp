#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--stayup-elevated-helper") {
        std::process::exit(stayup_windows::helper::run_elevated_helper());
    }
    if std::env::args().nth(1).as_deref() == Some("--stayup-uninstall-managed") {
        std::process::exit(stayup_windows::run_uninstaller());
    }
    if let Err(message) = stayup_windows::ensure_webview_runtime() {
        eprintln!("{message}");
        std::process::exit(1);
    }
    stayup_lib::run();
}
