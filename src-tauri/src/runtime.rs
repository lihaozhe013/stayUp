use std::sync::Arc;

use stayup_windows::StayUpBackend;

pub fn launch() {
    let owner_sid = match stayup_windows::current_owner_sid() {
        Ok(owner_sid) => owner_sid,
        Err(error) => {
            eprintln!("{}", error.message);
            return;
        }
    };
    if let Ok(local_data) = stayup_windows_local_data() {
        crate::logging::initialize(&local_data.join("Logs"));
    }

    let backend = Arc::new(StayUpBackend::new(owner_sid));
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(backend)
        .invoke_handler(tauri::generate_handler![
            crate::commands::get_current_owner_sid,
            crate::commands::list_managed_apps,
            crate::commands::get_managed_app,
            crate::commands::validate_app_draft,
            crate::commands::create_managed_app,
            crate::commands::update_managed_app,
            crate::commands::start_app,
            crate::commands::stop_app,
            crate::commands::restart_app,
            crate::commands::set_startup,
            crate::commands::remove_managed_app,
            crate::commands::read_app_logs,
            crate::commands::open_executable_directory,
            crate::commands::open_log_directory
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("StayUp could not start: {error}");
    }
}

fn stayup_windows_local_data() -> Result<std::path::PathBuf, stayup_core::AppError> {
    stayup_windows::local_data_root()
}
