use std::sync::Arc;

use stayup_core::{
    AppAction, AppDraft, AppError, AppOverview, LogResponse, LogStream, RunnerBackend,
};
use stayup_windows::StayUpBackend;
use tauri::State;

#[tauri::command]
pub fn get_current_owner_sid(backend: State<'_, Arc<StayUpBackend>>) -> String {
    backend.owner_sid().to_owned()
}

#[tauri::command]
pub fn list_managed_apps(
    backend: State<'_, Arc<StayUpBackend>>,
) -> Result<Vec<AppOverview>, AppError> {
    backend.list()
}

#[tauri::command]
pub fn get_managed_app(
    app_id: String,
    backend: State<'_, Arc<StayUpBackend>>,
) -> Result<stayup_core::ManagedApp, AppError> {
    backend.app(&app_id)
}

#[tauri::command]
pub fn validate_app_draft(
    draft: AppDraft,
    backend: State<'_, Arc<StayUpBackend>>,
) -> Result<(), AppError> {
    backend.validate_draft(backend.owner_sid(), draft)
}

#[tauri::command]
pub fn create_managed_app(
    draft: AppDraft,
    start_now: bool,
    backend: State<'_, Arc<StayUpBackend>>,
) -> Result<AppOverview, AppError> {
    backend.create(backend.owner_sid(), draft, start_now)
}

#[tauri::command]
pub fn update_managed_app(
    app_id: String,
    expected_revision: u64,
    draft: AppDraft,
    backend: State<'_, Arc<StayUpBackend>>,
) -> Result<AppOverview, AppError> {
    backend.update(
        backend.owner_sid(),
        parse_id(&app_id)?,
        expected_revision,
        draft,
    )
}

#[tauri::command]
pub fn start_app(
    app_id: String,
    backend: State<'_, Arc<StayUpBackend>>,
) -> Result<AppOverview, AppError> {
    backend.act(backend.owner_sid(), parse_id(&app_id)?, AppAction::Start)
}

#[tauri::command]
pub fn stop_app(
    app_id: String,
    backend: State<'_, Arc<StayUpBackend>>,
) -> Result<AppOverview, AppError> {
    backend.act(backend.owner_sid(), parse_id(&app_id)?, AppAction::Stop)
}

#[tauri::command]
pub fn restart_app(
    app_id: String,
    backend: State<'_, Arc<StayUpBackend>>,
) -> Result<AppOverview, AppError> {
    backend.act(backend.owner_sid(), parse_id(&app_id)?, AppAction::Restart)
}

#[tauri::command]
pub fn set_startup(
    app_id: String,
    enabled: bool,
    backend: State<'_, Arc<StayUpBackend>>,
) -> Result<AppOverview, AppError> {
    backend.set_startup(backend.owner_sid(), parse_id(&app_id)?, enabled)
}

#[tauri::command]
pub fn remove_managed_app(
    app_id: String,
    remove_logs: bool,
    backend: State<'_, Arc<StayUpBackend>>,
) -> Result<(), AppError> {
    backend.remove(backend.owner_sid(), parse_id(&app_id)?, remove_logs)
}

#[tauri::command]
pub fn read_app_logs(
    app_id: String,
    stream: LogStream,
    backend: State<'_, Arc<StayUpBackend>>,
) -> Result<LogResponse, AppError> {
    backend.logs(&app_id, stream)
}

#[tauri::command]
pub fn open_executable_directory(
    app_id: String,
    backend: State<'_, Arc<StayUpBackend>>,
) -> Result<(), AppError> {
    backend.open_executable_directory(&app_id)
}

#[tauri::command]
pub fn open_log_directory(
    app_id: String,
    backend: State<'_, Arc<StayUpBackend>>,
) -> Result<(), AppError> {
    backend.open_log_directory(&app_id)
}

fn parse_id(value: &str) -> Result<uuid::Uuid, AppError> {
    uuid::Uuid::parse_str(value).map_err(|_| {
        AppError::new(
            "invalid_app_id",
            "The selected app could not be identified.",
        )
    })
}
