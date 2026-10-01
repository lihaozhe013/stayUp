use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};

use stayup_core::AppError;
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HWND, LocalFree},
        Security::{
            GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER, TokenElevation,
            TokenUser,
        },
        System::{
            Com::CoTaskMemFree,
            Registry::{
                HKEY_LOCAL_MACHINE, REG_VALUE_TYPE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
            },
            Threading::{
                GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
        UI::{
            Shell::{
                FOLDERID_LocalAppData, FOLDERID_ProgramData, FOLDERID_ProgramFilesX64,
                SHGetKnownFolderPath,
            },
            WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
        },
    },
    core::{PWSTR, w},
};

pub fn current_user_sid() -> Result<String, AppError> {
    user_sid_for_process(unsafe { GetCurrentProcessId() })
}

pub fn user_sid_for_process(process_id: u32) -> Result<String, AppError> {
    unsafe {
        let process = if process_id == GetCurrentProcessId() {
            GetCurrentProcess()
        } else {
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).map_err(|error| {
                windows_error(
                    "caller_process_unavailable",
                    "Could not verify the requesting StayUp process.",
                    error,
                )
            })?
        };
        let _process_guard = if process_id == GetCurrentProcessId() {
            None
        } else {
            Some(HandleGuard(process))
        };
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).map_err(|error| {
            windows_error(
                "identity_unavailable",
                "Could not read the current Windows user.",
                error,
            )
        })?;
        let _token_guard = HandleGuard(token);

        let mut size = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut size);
        if size == 0 || size > 64 * 1024 {
            return Err(AppError::new(
                "identity_unavailable",
                "Could not read the current Windows user.",
            ));
        }
        let mut buffer = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
        GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            size,
            &mut size,
        )
        .map_err(|error| {
            windows_error(
                "identity_unavailable",
                "Could not read the current Windows user.",
                error,
            )
        })?;
        let token_user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut string_sid = PWSTR::null();
        windows::Win32::Security::Authorization::ConvertSidToStringSidW(
            token_user.User.Sid,
            &mut string_sid,
        )
        .map_err(|error| {
            windows_error(
                "identity_unavailable",
                "Could not read the current Windows user.",
                error,
            )
        })?;
        let sid = string_sid.to_string().map_err(|error| {
            AppError::new(
                "identity_unavailable",
                "Could not read the current Windows user.",
            )
            .with_details(error.to_string())
        })?;
        let _ = LocalFree(Some(windows::Win32::Foundation::HLOCAL(
            string_sid.0.cast(),
        )));
        Ok(sid)
    }
}

pub fn is_elevated() -> Result<bool, AppError> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(|error| {
            windows_error(
                "elevation_check_failed",
                "Could not verify the helper’s Windows permissions.",
                error,
            )
        })?;
        let _token_guard = HandleGuard(token);
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0u32;
        GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
        .map_err(|error| {
            windows_error(
                "elevation_check_failed",
                "Could not verify the helper’s Windows permissions.",
                error,
            )
        })?;
        Ok(elevation.TokenIsElevated != 0)
    }
}

pub fn known_folder(folder_id: &windows::core::GUID) -> Result<PathBuf, AppError> {
    unsafe {
        let path = SHGetKnownFolderPath(folder_id, Default::default(), None).map_err(|error| {
            windows_error(
                "known_folder_unavailable",
                "A required Windows folder could not be located.",
                error,
            )
        })?;
        let value = path.to_string().map_err(|error| {
            AppError::new(
                "known_folder_unavailable",
                "A required Windows folder could not be located.",
            )
            .with_details(error.to_string())
        })?;
        CoTaskMemFree(Some(path.0.cast()));
        Ok(PathBuf::from(value))
    }
}

pub fn program_data_root() -> Result<PathBuf, AppError> {
    Ok(known_folder(&FOLDERID_ProgramData)?.join("StayUp"))
}

pub fn program_files_root() -> Result<PathBuf, AppError> {
    Ok(known_folder(&FOLDERID_ProgramFilesX64)?.join("StayUp"))
}

pub fn local_data_root() -> Result<PathBuf, AppError> {
    Ok(known_folder(&FOLDERID_LocalAppData)?.join("StayUp"))
}

pub fn ensure_winsw_framework() -> Result<(), AppError> {
    const MINIMUM_RELEASE: u32 = 394_254;
    let mut release = 0u32;
    let mut length = std::mem::size_of::<u32>() as u32;
    let mut value_type = REG_VALUE_TYPE::default();
    let result = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\Microsoft\\NET Framework Setup\\NDP\\v4\\Full"),
            w!("Release"),
            RRF_RT_REG_DWORD,
            Some(&mut value_type),
            Some((&mut release as *mut u32).cast()),
            Some(&mut length),
        )
    };
    if result == windows::Win32::Foundation::WIN32_ERROR(0) && release >= MINIMUM_RELEASE {
        return Ok(());
    }
    Err(AppError::new(
        "winsw_framework_missing",
        "WinSW needs .NET Framework 4.6.1 or newer. Enable or install that Windows component, then try again.",
    ))
}

pub fn ensure_webview_runtime() -> Result<(), String> {
    let key_paths = [
        w!("SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"),
        w!(
            "SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
        ),
    ];
    let found = key_paths.iter().any(|key_path| unsafe {
        let mut length = 0u32;
        let mut value_type = REG_VALUE_TYPE::default();
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            *key_path,
            w!("pv"),
            RRF_RT_REG_SZ,
            Some(&mut value_type),
            None,
            Some(&mut length),
        ) == windows::Win32::Foundation::WIN32_ERROR(0)
            && length > 2
    });
    if found {
        return Ok(());
    }

    unsafe {
        MessageBoxW(
            Some(HWND::default()),
            w!(
                "StayUp needs the Microsoft Edge WebView2 Runtime to open its window. Install it from:\nhttps://developer.microsoft.com/en-us/microsoft-edge/webview2/"
            ),
            w!("WebView2 Runtime required"),
            MB_OK | MB_ICONERROR,
        );
    }
    Err("The Microsoft Edge WebView2 Runtime is missing. Install it from the Microsoft WebView2 download page, then open StayUp again.".to_owned())
}

pub fn windows_path_from_wide(buffer: &[u16]) -> PathBuf {
    PathBuf::from(OsString::from_wide(
        buffer.split(|value| *value == 0).next().unwrap_or_default(),
    ))
}

pub fn windows_error(code: &str, message: &str, error: windows::core::Error) -> AppError {
    AppError::new(code, message).with_details(error.to_string())
}

struct HandleGuard(HANDLE);

impl Drop for HandleGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
