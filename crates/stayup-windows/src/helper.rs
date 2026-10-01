use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle},
    path::Path,
};

use serde::{Deserialize, Serialize};
use stayup_core::{AppDraft, AppError};
use uuid::Uuid;
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HLOCAL},
        Security::{
            Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SECURITY_ATTRIBUTES,
        },
        Storage::FileSystem::FILE_FLAG_FIRST_PIPE_INSTANCE,
        System::{
            Pipes::{
                ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId,
                GetNamedPipeServerProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_TYPE_BYTE, PIPE_WAIT,
            },
            Threading::{GetCurrentProcessId, GetProcessId, WaitForSingleObject},
        },
        UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
    },
    core::{PCWSTR, w},
};

use crate::{
    platform::{self, windows_error},
    provisioning,
};

const MAX_HELPER_MESSAGE: usize = 4 * 1024 * 1024;
const HELPER_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase")]
pub enum HelperOperation {
    Create {
        draft: AppDraft,
        start_now: bool,
    },
    Update {
        app_id: Uuid,
        expected_revision: u64,
        draft: AppDraft,
    },
    SetStartup {
        app_id: Uuid,
        enabled: bool,
    },
    Remove {
        app_id: Uuid,
        remove_logs: bool,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HelperRequest {
    protocol_version: u32,
    session_nonce: String,
    owner_sid: String,
    operation: HelperOperation,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HelperResponse {
    app_id: Option<Uuid>,
    error: Option<AppError>,
}

pub fn elevated(owner_sid: &str, operation: HelperOperation) -> Result<Option<Uuid>, AppError> {
    let nonce = Uuid::new_v4().simple().to_string();
    let pipe_name = format!(r"\\.\pipe\stayup-{nonce}");
    let pipe_wide = wide(&pipe_name);
    let security_string = format!("D:P(A;;GA;;;{owner_sid})(A;;GA;;;SY)(A;;GA;;;BA)");
    let security_wide = wide(&security_string);

    unsafe {
        let mut security_descriptor = windows::Win32::Security::PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(security_wide.as_ptr()),
            1,
            &mut security_descriptor,
            None,
        )
        .map_err(|error| {
            windows_error(
                "secure_channel_failed",
                "Could not create a secure administrator connection.",
                error,
            )
        })?;
        let _descriptor_guard = LocalDescriptorGuard(security_descriptor);
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security_descriptor.0,
            bInheritHandle: false.into(),
        };
        let pipe = CreateNamedPipeW(
            PCWSTR(pipe_wide.as_ptr()),
            windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(
                windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX.0
                    | FILE_FLAG_FIRST_PIPE_INSTANCE.0,
            ),
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            64 * 1024,
            64 * 1024,
            30_000,
            Some(&attributes),
        );
        if pipe.0 as isize == -1 {
            return Err(AppError::new(
                "secure_channel_failed",
                "Could not open a secure administrator connection.",
            ));
        }
        let pipe_guard = RawHandleGuard(pipe);
        let current_exe = std::env::current_exe().map_err(|error| {
            AppError::new(
                "helper_unavailable",
                "StayUp could not locate its administrator helper.",
            )
            .with_details(error.to_string())
        })?;
        validate_installed_path(&current_exe)?;

        let parent_pid = GetCurrentProcessId();
        let arguments = format!("--stayup-elevated-helper \"{pipe_name}\" {parent_pid} {nonce}");
        let arguments_wide = wide(&arguments);
        let executable_wide = wide(&current_exe.to_string_lossy());
        let mut execute = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: w!("runas"),
            lpFile: PCWSTR(executable_wide.as_ptr()),
            lpParameters: PCWSTR(arguments_wide.as_ptr()),
            nShow: 0,
            ..Default::default()
        };
        if let Err(error) = ShellExecuteExW(&mut execute) {
            let code = if error.code().0 as u32 == 0x8007_04C7 {
                "elevation_cancelled"
            } else {
                "elevation_failed"
            };
            let message = if code == "elevation_cancelled" {
                "The administrator request was cancelled. No changes were made."
            } else {
                "Windows could not start the administrator helper."
            };
            return Err(windows_error(code, message, error));
        }
        let process_guard = RawHandleGuard(execute.hProcess);
        if execute.hProcess.0.is_null() {
            return Err(AppError::new(
                "helper_unavailable",
                "The administrator helper did not start.",
            ));
        }

        if let Err(error) = ConnectNamedPipe(pipe, None)
            && error.code().0 as u32 != 535
        {
            return Err(windows_error(
                "secure_channel_failed",
                "The administrator helper could not connect securely.",
                error,
            ));
        }
        let mut client_pid = 0u32;
        GetNamedPipeClientProcessId(pipe, &mut client_pid).map_err(|error| {
            windows_error(
                "secure_channel_failed",
                "Could not verify the administrator helper process.",
                error,
            )
        })?;
        if client_pid != GetProcessId(execute.hProcess) {
            return Err(AppError::new(
                "secure_channel_rejected",
                "The administrator connection came from an unexpected process.",
            ));
        }

        let mut channel = File::from_raw_handle(pipe.0);
        pipe_guard.disarm();
        let request = HelperRequest {
            protocol_version: HELPER_PROTOCOL_VERSION,
            session_nonce: nonce,
            owner_sid: owner_sid.to_owned(),
            operation,
        };
        write_frame(&mut channel, &request)?;
        let response: HelperResponse = read_frame(&mut channel)?;
        let _ = WaitForSingleObject(execute.hProcess, 30_000);
        drop(process_guard);
        match response.error {
            Some(error) => Err(error),
            None => Ok(response.app_id),
        }
    }
}

pub fn run_elevated_helper() -> i32 {
    match receive_and_apply() {
        Ok((response, mut channel)) => {
            if let Err(error) = write_frame(&mut channel, &response) {
                eprintln!("{error}");
                1
            } else if response.error.is_some() {
                1
            } else {
                0
            }
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

pub fn run_uninstaller() -> i32 {
    match (|| {
        ensure_elevated_install()?;
        provisioning::remove_all_managed_services()
    })() {
        Ok(()) => 0,
        Err(error) => {
            let text = format!(
                "{}\n\n{}\n\nStayUp has not been removed. Resolve this app’s service issue and try uninstalling again.",
                error.message,
                error.details.unwrap_or_default()
            );
            let text_wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                    Some(windows::Win32::Foundation::HWND::default()),
                    PCWSTR(text_wide.as_ptr()),
                    w!("StayUp could not finish uninstalling"),
                    windows::Win32::UI::WindowsAndMessaging::MB_OK
                        | windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
                );
            }
            1
        }
    }
}

fn receive_and_apply() -> Result<(HelperResponse, File), AppError> {
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.len() != 5 {
        return Err(AppError::new(
            "helper_request_invalid",
            "The administrator helper request is incomplete.",
        ));
    }
    let pipe_name = &arguments[2];
    let parent_pid = arguments[3].parse::<u32>().map_err(|_| {
        AppError::new(
            "helper_request_invalid",
            "The administrator helper request is invalid.",
        )
    })?;
    let nonce = &arguments[4];
    if !nonce.chars().all(|character| character.is_ascii_hexdigit()) || nonce.len() != 32 {
        return Err(AppError::new(
            "helper_request_invalid",
            "The administrator helper request is invalid.",
        ));
    }
    ensure_elevated_install()?;

    let mut pipe_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(pipe_name)
        .map_err(|error| {
            AppError::new(
                "secure_channel_failed",
                "The administrator helper could not connect to StayUp.",
            )
            .with_details(error.to_string())
        })?;
    let mut server_pid = 0u32;
    unsafe {
        GetNamedPipeServerProcessId(HANDLE(pipe_file.as_raw_handle().cast()), &mut server_pid)
            .map_err(|error| {
                windows_error(
                    "secure_channel_failed",
                    "Could not verify the StayUp window process.",
                    error,
                )
            })?;
    }
    if server_pid != parent_pid {
        return Err(AppError::new(
            "secure_channel_rejected",
            "The administrator request did not come from the expected StayUp window.",
        ));
    }
    let caller_sid = platform::user_sid_for_process(server_pid)?;
    let request: HelperRequest = read_frame(&mut pipe_file)?;
    if request.protocol_version != HELPER_PROTOCOL_VERSION
        || request.session_nonce != *nonce
        || request.owner_sid != caller_sid
    {
        return Err(AppError::new(
            "secure_channel_rejected",
            "The administrator request failed its identity check.",
        ));
    }
    let response = match provisioning::apply_request(&request.owner_sid, request.operation) {
        Ok(app_id) => Ok(HelperResponse {
            app_id,
            error: None,
        }),
        Err(error) => Ok(HelperResponse {
            app_id: None,
            error: Some(error),
        }),
    }?;
    Ok((response, pipe_file))
}

fn validate_installed_path(executable: &Path) -> Result<(), AppError> {
    let installed_root = platform::program_files_root()?;
    let expected = installed_root.canonicalize().map_err(|error| {
        AppError::new(
            "helper_unavailable",
            "Install StayUp before managing background apps.",
        )
        .with_details(error.to_string())
    })?;
    let actual = executable
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .ok_or_else(|| {
            AppError::new(
                "helper_unavailable",
                "StayUp must be installed in Program Files to manage background apps.",
            )
        })?;
    if expected != actual {
        return Err(AppError::new(
            "helper_unavailable",
            "Install StayUp in Program Files before managing background apps.",
        ));
    }
    Ok(())
}

fn ensure_elevated_install() -> Result<(), AppError> {
    if !platform::is_elevated()? {
        return Err(AppError::new(
            "helper_not_elevated",
            "Windows did not grant the administrator permissions required for this operation.",
        ));
    }
    let executable = std::env::current_exe().map_err(|error| {
        AppError::new(
            "helper_unavailable",
            "StayUp could not locate its installation folder.",
        )
        .with_details(error.to_string())
    })?;
    validate_installed_path(&executable)
}

fn write_frame<T: Serialize>(channel: &mut impl Write, value: &T) -> Result<(), AppError> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        AppError::new(
            "secure_channel_failed",
            "Could not encode the administrator request.",
        )
        .with_details(error.to_string())
    })?;
    if bytes.len() > MAX_HELPER_MESSAGE {
        return Err(AppError::new(
            "helper_request_too_large",
            "The administrator request is too large.",
        ));
    }
    channel
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .and_then(|()| channel.write_all(&bytes))
        .and_then(|()| channel.flush())
        .map_err(|error| {
            AppError::new(
                "secure_channel_failed",
                "Could not send the administrator request.",
            )
            .with_details(error.to_string())
        })
}

fn read_frame<T: for<'de> Deserialize<'de>>(channel: &mut impl Read) -> Result<T, AppError> {
    let mut length = [0u8; 4];
    channel.read_exact(&mut length).map_err(|error| {
        AppError::new(
            "secure_channel_failed",
            "Could not read the administrator operation result.",
        )
        .with_details(error.to_string())
    })?;
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > MAX_HELPER_MESSAGE {
        return Err(AppError::new(
            "helper_request_invalid",
            "The administrator request has an invalid size.",
        ));
    }
    let mut bytes = vec![0u8; length];
    channel.read_exact(&mut bytes).map_err(|error| {
        AppError::new(
            "secure_channel_failed",
            "Could not read the administrator operation result.",
        )
        .with_details(error.to_string())
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        AppError::new(
            "helper_request_invalid",
            "The administrator request could not be decoded.",
        )
        .with_details(error.to_string())
    })
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

struct RawHandleGuard(HANDLE);

impl RawHandleGuard {
    fn disarm(self) {
        std::mem::forget(self);
    }
}

impl Drop for RawHandleGuard {
    fn drop(&mut self) {
        if !self.0.0.is_null() && self.0.0 as isize != -1 {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

struct LocalDescriptorGuard(windows::Win32::Security::PSECURITY_DESCRIPTOR);

impl Drop for LocalDescriptorGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::LocalFree(Some(HLOCAL(self.0.0.cast())));
        }
    }
}
