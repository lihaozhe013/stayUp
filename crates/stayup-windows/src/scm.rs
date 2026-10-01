use std::{
    ffi::OsStr,
    os::windows::ffi::OsStrExt,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use stayup_core::{AppError, AppOverview, ManagedApp, ServiceStatus};
use windows::{
    Win32::System::Services::{
        ChangeServiceConfig2W, ChangeServiceConfigW, CloseServiceHandle, ControlService,
        OpenSCManagerW, OpenServiceW, QueryServiceConfigW, QueryServiceStatusEx, SC_ACTION,
        SC_ACTION_NONE, SC_ACTION_RESTART, SC_HANDLE, SC_MANAGER_CONNECT, SC_STATUS_PROCESS_INFO,
        SERVICE_AUTO_START, SERVICE_CHANGE_CONFIG, SERVICE_CONFIG_FAILURE_ACTIONS,
        SERVICE_CONFIG_SERVICE_SID_INFO, SERVICE_DEMAND_START, SERVICE_FAILURE_ACTIONSW,
        SERVICE_NO_CHANGE, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_SID_INFO,
        SERVICE_SID_TYPE_UNRESTRICTED, SERVICE_START, SERVICE_START_PENDING, SERVICE_STATUS,
        SERVICE_STATUS_PROCESS, SERVICE_STOP, SERVICE_STOP_PENDING, SERVICE_STOPPED, StartServiceW,
    },
    core::{PCWSTR, PWSTR},
};

use crate::platform::windows_error;

pub fn query_status(app: &ManagedApp) -> AppOverview {
    let status = match service_status(&app.service_id()) {
        Ok(status) => status,
        Err(error) if error.code == "service_missing" => ServiceStatus::Missing,
        Err(_) => ServiceStatus::Unknown,
    };
    let status_detail = match status {
        ServiceStatus::Missing => Some(
            "The Windows service is no longer installed. Edit or remove this entry to repair it."
                .to_owned(),
        ),
        ServiceStatus::Unknown => {
            Some("Windows could not report this app’s service status.".to_owned())
        }
        _ => None,
    };
    AppOverview {
        app: app.clone(),
        status,
        status_detail,
    }
}

pub fn service_status(service_id: &str) -> Result<ServiceStatus, AppError> {
    let service = open_service(service_id, SERVICE_QUERY_STATUS)?;
    Ok(query_state(service.0)?.0)
}

pub fn act(app: &ManagedApp, action: stayup_core::AppAction) -> Result<AppOverview, AppError> {
    let service = open_service(
        &app.service_id(),
        SERVICE_QUERY_STATUS | SERVICE_START | SERVICE_STOP,
    )?;
    match action {
        stayup_core::AppAction::Start => start_service(service.0)?,
        stayup_core::AppAction::Stop => stop_service(service.0, app.advanced.stop_timeout_seconds)?,
        stayup_core::AppAction::Restart => {
            stop_service(service.0, app.advanced.stop_timeout_seconds)?;
            start_service(service.0)?;
        }
    }
    Ok(query_status(app))
}

pub fn set_startup_and_recovery(app: &ManagedApp) -> Result<(), AppError> {
    let service = open_service(&app.service_id(), SERVICE_CHANGE_CONFIG)?;
    let display_name = wide(app.name.trim());
    unsafe {
        ChangeServiceConfigW(
            service.0,
            windows::Win32::System::Services::ENUM_SERVICE_TYPE(SERVICE_NO_CHANGE),
            if app.startup_enabled {
                SERVICE_AUTO_START
            } else {
                SERVICE_DEMAND_START
            },
            windows::Win32::System::Services::SERVICE_ERROR(SERVICE_NO_CHANGE),
            PCWSTR::null(),
            PCWSTR::null(),
            None,
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR(display_name.as_ptr()),
        )
        .map_err(|error| {
            windows_error(
                "service_configuration_failed",
                "Could not update the Windows startup setting.",
                error,
            )
        })?;

        let actions = if matches!(app.restart.policy, stayup_core::RestartPolicy::OnFailure) {
            [
                SC_ACTION {
                    Type: SC_ACTION_RESTART,
                    Delay: app.restart.delay_seconds.saturating_mul(1000),
                },
                SC_ACTION {
                    Type: SC_ACTION_RESTART,
                    Delay: app.restart.delay_seconds.saturating_mul(1000),
                },
            ]
        } else {
            [
                SC_ACTION {
                    Type: SC_ACTION_NONE,
                    Delay: 0,
                },
                SC_ACTION {
                    Type: SC_ACTION_NONE,
                    Delay: 0,
                },
            ]
        };
        let failure_actions = SERVICE_FAILURE_ACTIONSW {
            dwResetPeriod: 86_400,
            lpRebootMsg: PWSTR::null(),
            lpCommand: PWSTR::null(),
            cActions: actions.len() as u32,
            lpsaActions: actions.as_ptr().cast_mut(),
        };
        ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_FAILURE_ACTIONS,
            Some((&failure_actions as *const SERVICE_FAILURE_ACTIONSW).cast()),
        )
        .map_err(|error| {
            windows_error(
                "service_configuration_failed",
                "Could not update the restart behavior.",
                error,
            )
        })?;

        let service_sid = SERVICE_SID_INFO {
            dwServiceSidType: SERVICE_SID_TYPE_UNRESTRICTED,
        };
        ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_SERVICE_SID_INFO,
            Some((&service_sid as *const SERVICE_SID_INFO).cast()),
        )
        .map_err(|error| {
            windows_error(
                "service_configuration_failed",
                "Could not enable the app’s low-privilege service identity.",
                error,
            )
        })?;
    }
    Ok(())
}

pub fn ensure_registered_service(
    app: &ManagedApp,
    expected_wrapper: &Path,
) -> Result<(), AppError> {
    let service = open_service(&app.service_id(), SERVICE_QUERY_CONFIG)?;
    let (actual_path, actual_account) = unsafe {
        let mut needed = 0u32;
        let _ = QueryServiceConfigW(service.0, None, 0, &mut needed);
        if needed == 0 || needed > 64 * 1024 {
            return Err(AppError::new(
                "service_configuration_unavailable",
                "Could not verify the installed service.",
            ));
        }
        let mut words = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        QueryServiceConfigW(
            service.0,
            Some(words.as_mut_ptr().cast()),
            needed,
            &mut needed,
        )
        .map_err(|error| {
            windows_error(
                "service_configuration_unavailable",
                "Could not verify the installed service.",
                error,
            )
        })?;
        let configuration = &*words
            .as_ptr()
            .cast::<windows::Win32::System::Services::QUERY_SERVICE_CONFIGW>();
        (
            configuration
                .lpBinaryPathName
                .to_string()
                .unwrap_or_default(),
            configuration
                .lpServiceStartName
                .to_string()
                .unwrap_or_default(),
        )
    };
    let expected = expected_wrapper.to_string_lossy();
    if !normalize_command_path(&actual_path)
        .eq_ignore_ascii_case(&normalize_command_path(&expected))
    {
        return Err(AppError::new(
            "service_ownership_mismatch",
            "This service’s runtime path does not match the StayUp entry.",
        ));
    }
    if !actual_account.eq_ignore_ascii_case("NT AUTHORITY\\LocalService") {
        return Err(AppError::new(
            "service_account_mismatch",
            "This service is not running under StayUp’s restricted Windows account.",
        ));
    }
    Ok(())
}

pub fn start_service(service: SC_HANDLE) -> Result<(), AppError> {
    unsafe {
        StartServiceW(service, None).map_err(|error| {
            windows_error(
                "service_start_failed",
                "Windows could not start this app.",
                error,
            )
        })?;
    }
    wait_for_state(service, ServiceStatus::Running, Duration::from_secs(60))
}

pub fn stop_service(service: SC_HANDLE, timeout_seconds: u32) -> Result<(), AppError> {
    let (current, _) = query_state(service)?;
    if current == ServiceStatus::Stopped {
        return Ok(());
    }
    unsafe {
        let mut status = SERVICE_STATUS::default();
        ControlService(service, SERVICE_STOP, &mut status).map_err(|error| {
            windows_error(
                "service_stop_failed",
                "Windows could not stop this app.",
                error,
            )
        })?;
    }
    wait_for_state(
        service,
        ServiceStatus::Stopped,
        Duration::from_secs(timeout_seconds.max(5) as u64 + 15),
    )
}

fn wait_for_state(
    service: SC_HANDLE,
    target: ServiceStatus,
    timeout: Duration,
) -> Result<(), AppError> {
    let deadline = Instant::now() + timeout;
    loop {
        let (state, _) = query_state(service)?;
        if state == target {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(AppError::new(
                "service_operation_timeout",
                "The app did not reach the expected state in time.",
            ));
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn query_state(service: SC_HANDLE) -> Result<(ServiceStatus, u32), AppError> {
    unsafe {
        let mut words = vec![
            0usize;
            std::mem::size_of::<SERVICE_STATUS_PROCESS>()
                .div_ceil(std::mem::size_of::<usize>())
        ];
        let mut needed = 0u32;
        let bytes = std::slice::from_raw_parts_mut(
            words.as_mut_ptr().cast::<u8>(),
            std::mem::size_of::<SERVICE_STATUS_PROCESS>(),
        );
        QueryServiceStatusEx(service, SC_STATUS_PROCESS_INFO, Some(bytes), &mut needed).map_err(
            |error| {
                windows_error(
                    "service_status_failed",
                    "Could not query this app’s Windows service.",
                    error,
                )
            },
        )?;
        let status = &*words.as_ptr().cast::<SERVICE_STATUS_PROCESS>();
        let state = if status.dwCurrentState == SERVICE_START_PENDING {
            ServiceStatus::StartPending
        } else if status.dwCurrentState == SERVICE_STOP_PENDING {
            ServiceStatus::StopPending
        } else if status.dwCurrentState == SERVICE_STOPPED {
            ServiceStatus::Stopped
        } else if status.dwCurrentState.0 == 4 {
            ServiceStatus::Running
        } else {
            ServiceStatus::Unknown
        };
        Ok((state, status.dwWin32ExitCode))
    }
}

fn open_service(service_id: &str, access: u32) -> Result<ServiceGuard, AppError> {
    let service_name = wide(service_id);
    unsafe {
        let manager = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT).map_err(
            |error| {
                windows_error(
                    "service_manager_unavailable",
                    "Windows Service Control Manager could not be opened.",
                    error,
                )
            },
        )?;
        let manager_guard = ServiceGuard(manager);
        let service = OpenServiceW(manager_guard.0, PCWSTR(service_name.as_ptr()), access)
            .map_err(|error| {
                if error.code().0 as u32 == 1060 {
                    AppError::new("service_missing", "The Windows service is not installed.")
                } else {
                    windows_error(
                        "service_access_denied",
                        "Windows denied access to this app’s service.",
                        error,
                    )
                }
            })?;
        drop(manager_guard);
        Ok(ServiceGuard(service))
    }
}

fn normalize_command_path(command: &str) -> String {
    let trimmed = command.trim();
    let without_quotes = trimmed
        .strip_prefix('"')
        .and_then(|value| value.split_once('"').map(|(path, _)| path))
        .unwrap_or(trimmed);
    without_quotes.trim().trim_matches('"').replace('/', "\\")
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

struct ServiceGuard(SC_HANDLE);

impl Drop for ServiceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseServiceHandle(self.0);
        }
    }
}
