use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::ffi::OsStrExt,
    path::Path,
    sync::Mutex,
};

use stayup_core::{
    AppAction, AppDraft, AppError, AppOverview, LogResponse, LogStream, ManagedApp, RunnerBackend,
    ServiceStatus, validation::validate_app,
};
use uuid::Uuid;
use windows::{
    Win32::UI::Shell::ShellExecuteW,
    core::{PCWSTR, w},
};

use crate::{
    helper::{self, HelperOperation},
    scm, storage,
};

const MAX_LOG_BYTES: usize = 1024 * 1024;
const MAX_LOG_LINES: usize = 2_000;

pub struct StayUpBackend {
    owner_sid: String,
    mutation_lock: Mutex<()>,
}

impl StayUpBackend {
    pub fn new(owner_sid: String) -> Self {
        Self {
            owner_sid,
            mutation_lock: Mutex::new(()),
        }
    }

    pub fn owner_sid(&self) -> &str {
        &self.owner_sid
    }

    pub fn list(&self) -> Result<Vec<AppOverview>, AppError> {
        self.list_owned(&self.owner_sid)
    }

    pub fn app(&self, app_id: &str) -> Result<ManagedApp, AppError> {
        let app_id = Uuid::parse_str(app_id).map_err(|_| {
            AppError::new(
                "invalid_app_id",
                "The selected app could not be identified.",
            )
        })?;
        storage::load(&self.owner_sid, app_id)
    }

    pub fn validate_draft(&self, owner_sid: &str, draft: AppDraft) -> Result<(), AppError> {
        self.check_owner(owner_sid)?;
        validate_app(&draft.into_app(owner_sid.to_owned()), true)
    }

    pub fn logs(&self, app_id: &str, stream: LogStream) -> Result<LogResponse, AppError> {
        let app = self.app(app_id)?;
        let paths = storage::paths(&self.owner_sid, app.id)?;
        let log_path = if stream == LogStream::Diagnostic {
            diagnostic_path(&self.owner_sid, app.id)?
        } else {
            let name = if stream == LogStream::Stdout {
                "wrapper.out.log"
            } else {
                "wrapper.err.log"
            };
            paths.log_directory.join(name)
        };
        read_log_tail(&log_path)
    }

    pub fn open_executable_directory(&self, app_id: &str) -> Result<(), AppError> {
        let app = self.app(app_id)?;
        let directory = Path::new(&app.process.executable).parent().ok_or_else(|| {
            AppError::new(
                "executable_directory_missing",
                "The program folder could not be located.",
            )
        })?;
        open_folder(directory)
    }

    pub fn open_log_directory(&self, app_id: &str) -> Result<(), AppError> {
        let app = self.app(app_id)?;
        let paths = storage::paths(&self.owner_sid, app.id)?;
        open_folder(&paths.log_directory)
    }

    pub fn update_startup(&self, app_id: &str, enabled: bool) -> Result<AppOverview, AppError> {
        let _guard = self.mutation_lock.lock().map_err(|_| {
            AppError::new(
                "operation_busy",
                "StayUp could not lock this app for an update.",
            )
        })?;
        let app_id = parse_app_id(app_id)?;
        helper::elevated(
            &self.owner_sid,
            HelperOperation::SetStartup { app_id, enabled },
        )?;
        let updated = storage::load(&self.owner_sid, app_id)?;
        record_diagnostic(
            &self.owner_sid,
            app_id,
            if enabled {
                "Automatic startup enabled."
            } else {
                "Automatic startup disabled."
            },
        );
        Ok(scm::query_status(&updated))
    }
}

impl RunnerBackend for StayUpBackend {
    fn list_owned(&self, owner_sid: &str) -> Result<Vec<AppOverview>, AppError> {
        if owner_sid != self.owner_sid {
            return Err(AppError::new(
                "owner_mismatch",
                "This app belongs to a different Windows user.",
            ));
        }
        let mut result = Vec::new();
        for app in storage::load_owned(owner_sid)? {
            let paths = storage::paths(owner_sid, app.id)?;
            match scm::ensure_registered_service(&app, &paths.wrapper) {
                Ok(()) => result.push(scm::query_status(&app)),
                Err(error) if error.code == "service_missing" => {
                    result.push(scm::query_status(&app))
                }
                Err(error) => result.push(AppOverview {
                    app,
                    status: ServiceStatus::NeedsAttention,
                    status_detail: Some(error.message),
                }),
            }
        }
        Ok(result)
    }

    fn create(
        &self,
        owner_sid: &str,
        draft: AppDraft,
        start_now: bool,
    ) -> Result<AppOverview, AppError> {
        self.check_owner(owner_sid)?;
        let _guard = self.mutation_lock.lock().map_err(|_| {
            AppError::new(
                "operation_busy",
                "StayUp could not lock the app list for an update.",
            )
        })?;
        let app_id = helper::elevated(owner_sid, HelperOperation::Create { draft, start_now })?
            .ok_or_else(|| {
                AppError::new(
                    "create_result_missing",
                    "The app was created, but its identifier was not returned.",
                )
            })?;
        let app = storage::load(owner_sid, app_id)?;
        let overview = scm::query_status(&app);
        if start_now && overview.status != ServiceStatus::Running {
            record_diagnostic(
                owner_sid,
                app_id,
                "App was created, but did not reach a running state. Check its logs or try starting it again.",
            );
        } else {
            record_diagnostic(
                owner_sid,
                app_id,
                if start_now {
                    "App created and started."
                } else {
                    "App created."
                },
            );
        }
        Ok(overview)
    }

    fn update(
        &self,
        owner_sid: &str,
        app_id: Uuid,
        expected_revision: u64,
        draft: AppDraft,
    ) -> Result<AppOverview, AppError> {
        self.check_owner(owner_sid)?;
        let _guard = self.mutation_lock.lock().map_err(|_| {
            AppError::new(
                "operation_busy",
                "StayUp could not lock this app for an update.",
            )
        })?;
        helper::elevated(
            owner_sid,
            HelperOperation::Update {
                app_id,
                expected_revision,
                draft,
            },
        )?;
        let app = storage::load(owner_sid, app_id)?;
        let overview = scm::query_status(&app);
        record_diagnostic(
            owner_sid,
            app_id,
            if matches!(
                overview.status,
                ServiceStatus::Stopped | ServiceStatus::NeedsAttention
            ) {
                "App settings were saved, but the app is not running. Check its logs or try starting it again."
            } else {
                "App settings updated."
            },
        );
        Ok(overview)
    }

    fn act(
        &self,
        owner_sid: &str,
        app_id: Uuid,
        action: AppAction,
    ) -> Result<AppOverview, AppError> {
        self.check_owner(owner_sid)?;
        let app = storage::load(owner_sid, app_id)?;
        let paths = storage::paths(owner_sid, app_id)?;
        scm::ensure_registered_service(&app, &paths.wrapper)?;
        let overview = scm::act(&app, action)?;
        record_diagnostic(
            owner_sid,
            app_id,
            match action {
                AppAction::Start => "App started.",
                AppAction::Stop => "App stopped.",
                AppAction::Restart => "App restarted.",
            },
        );
        Ok(overview)
    }

    fn set_startup(
        &self,
        owner_sid: &str,
        app_id: Uuid,
        enabled: bool,
    ) -> Result<AppOverview, AppError> {
        self.check_owner(owner_sid)?;
        self.update_startup(&app_id.to_string(), enabled)
    }

    fn remove(&self, owner_sid: &str, app_id: Uuid, remove_logs: bool) -> Result<(), AppError> {
        self.check_owner(owner_sid)?;
        let _guard = self.mutation_lock.lock().map_err(|_| {
            AppError::new(
                "operation_busy",
                "StayUp could not lock this app for removal.",
            )
        })?;
        let app = storage::load(owner_sid, app_id)?;
        helper::elevated(
            owner_sid,
            HelperOperation::Remove {
                app_id,
                remove_logs,
            },
        )?;
        if remove_logs {
            if let Ok(path) = diagnostic_path(owner_sid, app_id) {
                let _ = fs::remove_file(path);
            }
        } else {
            record_diagnostic(
                owner_sid,
                app_id,
                &format!("{} was removed. Program logs were kept.", app.name),
            );
        }
        Ok(())
    }
}

impl StayUpBackend {
    fn check_owner(&self, owner_sid: &str) -> Result<(), AppError> {
        if owner_sid == self.owner_sid {
            Ok(())
        } else {
            Err(AppError::new(
                "owner_mismatch",
                "This app belongs to a different Windows user.",
            ))
        }
    }
}

fn parse_app_id(value: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(value).map_err(|_| {
        AppError::new(
            "invalid_app_id",
            "The selected app could not be identified.",
        )
    })
}

fn read_log_tail(path: &Path) -> Result<LogResponse, AppError> {
    if !path.exists() {
        return Ok(LogResponse {
            content: String::new(),
            truncated: false,
            byte_limit: MAX_LOG_BYTES as u32,
        });
    }
    let mut file = File::open(path).map_err(|error| {
        AppError::new("log_read_failed", "Could not open this app’s log.")
            .with_details(error.to_string())
    })?;
    let total = file
        .metadata()
        .map_err(|error| {
            AppError::new("log_read_failed", "Could not inspect this app’s log.")
                .with_details(error.to_string())
        })?
        .len();
    let start = total.saturating_sub(MAX_LOG_BYTES as u64);
    file.seek(SeekFrom::Start(start)).map_err(|error| {
        AppError::new("log_read_failed", "Could not read this app’s log.")
            .with_details(error.to_string())
    })?;
    let mut bytes = Vec::with_capacity((total - start) as usize);
    file.take(MAX_LOG_BYTES as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            AppError::new("log_read_failed", "Could not read this app’s log.")
                .with_details(error.to_string())
        })?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = text.lines().collect();
    let line_truncated = lines.len() > MAX_LOG_LINES;
    if line_truncated {
        lines.drain(..lines.len() - MAX_LOG_LINES);
    }
    Ok(LogResponse {
        content: lines.join("\n"),
        truncated: start > 0 || line_truncated,
        byte_limit: MAX_LOG_BYTES as u32,
    })
}

fn diagnostic_path(owner_sid: &str, app_id: Uuid) -> Result<std::path::PathBuf, AppError> {
    Ok(crate::platform::local_data_root()?
        .join("Logs")
        .join(owner_sid)
        .join(format!("{app_id}.stayup.log")))
}

fn record_diagnostic(owner_sid: &str, app_id: Uuid, message: &str) {
    let Ok(path) = diagnostic_path(owner_sid, app_id) else {
        return;
    };
    let Some(directory) = path.parent() else {
        return;
    };
    if fs::create_dir_all(directory).is_err() {
        return;
    }
    if fs::metadata(&path)
        .map(|metadata| metadata.len() > 5 * 1024 * 1024)
        .unwrap_or(false)
    {
        let _ = fs::remove_file(&path);
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{timestamp} {message}");
    }
}

fn open_folder(path: &Path) -> Result<(), AppError> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let result = ShellExecuteW(
            Some(windows::Win32::Foundation::HWND::default()),
            w!("open"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        );
        if result.0 as usize <= 32 {
            return Err(AppError::new(
                "folder_open_failed",
                "Windows could not open this folder.",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_tail_is_bounded_by_bytes_and_lines() {
        let directory = std::env::temp_dir().join(format!("stayup-log-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("wrapper.out.log");
        fs::write(&path, "line\n".repeat(700_000)).unwrap();

        let result = read_log_tail(&path).unwrap();

        assert!(result.truncated);
        assert!(result.content.len() <= MAX_LOG_BYTES);
        assert!(result.content.lines().count() <= MAX_LOG_LINES);
        let _ = fs::remove_dir_all(directory);
    }
}
