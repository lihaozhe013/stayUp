#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
mod backend;
#[cfg(windows)]
pub mod helper;
#[cfg(windows)]
mod platform;
#[cfg(windows)]
mod provisioning;
#[cfg(windows)]
mod scm;
#[cfg(windows)]
mod storage;

#[cfg(windows)]
pub use backend::StayUpBackend;

#[cfg(windows)]
pub fn current_owner_sid() -> Result<String, stayup_core::AppError> {
    platform::current_user_sid()
}

#[cfg(windows)]
pub fn local_data_root() -> Result<std::path::PathBuf, stayup_core::AppError> {
    platform::local_data_root()
}

#[cfg(windows)]
pub fn run_elevated_helper() -> i32 {
    helper::run_elevated_helper()
}

#[cfg(windows)]
pub fn run_uninstaller() -> i32 {
    helper::run_uninstaller()
}

#[cfg(windows)]
pub fn ensure_webview_runtime() -> Result<(), String> {
    platform::ensure_webview_runtime()
}
