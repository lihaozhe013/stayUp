use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    str::FromStr,
};

use stayup_core::{AppError, MAX_APP_CONFIG_BYTES, ManagedApp};
use uuid::Uuid;

use crate::platform::program_data_root;

const MAX_CONFIGURATION_BYTES: u64 = MAX_APP_CONFIG_BYTES as u64;

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub configuration: PathBuf,
    pub runtime_directory: PathBuf,
    pub wrapper: PathBuf,
    pub xml: PathBuf,
    pub log_directory: PathBuf,
}

pub fn paths(owner_sid: &str, app_id: Uuid) -> Result<AppPaths, AppError> {
    validate_sid(owner_sid)?;
    let root = program_data_root()?;
    let managed_root = root.join("Managed");
    let runtime_root = root.join("Runtimes");
    let logs_root = root.join("Logs");
    let owner = Path::new(owner_sid);
    let id = app_id.to_string();
    let settings_directory = managed_root.join(owner).join(&id);
    let runtime_directory = runtime_root.join(owner).join(&id);
    let log_directory = logs_root.join(owner).join(&id);
    Ok(AppPaths {
        configuration: settings_directory.join("app.json"),
        runtime_directory: runtime_directory.clone(),
        wrapper: runtime_directory.join("wrapper.exe"),
        xml: runtime_directory.join("wrapper.xml"),
        log_directory,
    })
}

pub fn owner_directory(owner_sid: &str) -> Result<PathBuf, AppError> {
    validate_sid(owner_sid)?;
    Ok(program_data_root()?.join("Managed").join(owner_sid))
}

pub fn load_owned(owner_sid: &str) -> Result<Vec<ManagedApp>, AppError> {
    let owner_directory = owner_directory(owner_sid)?;
    reject_reparse_components(&program_data_root()?, &owner_directory)?;
    if !owner_directory.exists() {
        return Ok(Vec::new());
    }

    let mut apps = Vec::new();
    for entry in fs::read_dir(&owner_directory).map_err(io_error(
        "configuration_read_failed",
        "Could not read the managed app list.",
    ))? {
        let entry = entry.map_err(io_error(
            "configuration_read_failed",
            "Could not read the managed app list.",
        ))?;
        if !entry
            .file_type()
            .map_err(io_error(
                "configuration_read_failed",
                "Could not inspect a managed app entry.",
            ))?
            .is_dir()
        {
            continue;
        }
        reject_reparse_components(&owner_directory, &entry.path())?;
        let Some(id) = entry
            .file_name()
            .to_str()
            .and_then(|value| Uuid::from_str(value).ok())
        else {
            continue;
        };
        let configuration = entry.path().join("app.json");
        if !configuration.exists() {
            continue;
        }
        reject_reparse_components(&entry.path(), &configuration)?;
        let metadata = fs::metadata(&configuration).map_err(io_error(
            "configuration_read_failed",
            "Could not read a managed app configuration.",
        ))?;
        if metadata.len() > MAX_CONFIGURATION_BYTES {
            return Err(AppError::new(
                "configuration_invalid",
                "A managed app configuration is too large.",
            ));
        }
        let app: ManagedApp =
            serde_json::from_reader(File::open(&configuration).map_err(io_error(
                "configuration_read_failed",
                "Could not read a managed app configuration.",
            ))?)
            .map_err(|error| {
                AppError::new(
                    "configuration_invalid",
                    "A managed app configuration could not be read.",
                )
                .with_details(error.to_string())
            })?;
        if app.id != id || app.owner_sid != owner_sid {
            return Err(AppError::new(
                "configuration_ownership_mismatch",
                "A managed app entry failed its ownership check.",
            ));
        }
        apps.push(app);
    }
    apps.sort_by_key(|app| app.name.to_lowercase());
    Ok(apps)
}

pub fn load(owner_sid: &str, app_id: Uuid) -> Result<ManagedApp, AppError> {
    let paths = paths(owner_sid, app_id)?;
    reject_reparse_components(&program_data_root()?, &paths.configuration)?;
    let metadata = fs::metadata(&paths.configuration).map_err(io_error(
        "configuration_missing",
        "The managed app configuration could not be found.",
    ))?;
    if metadata.len() > MAX_CONFIGURATION_BYTES {
        return Err(AppError::new(
            "configuration_invalid",
            "The managed app configuration is too large.",
        ));
    }
    let app: ManagedApp =
        serde_json::from_reader(File::open(&paths.configuration).map_err(io_error(
            "configuration_read_failed",
            "Could not read the managed app configuration.",
        ))?)
        .map_err(|error| {
            AppError::new(
                "configuration_invalid",
                "The managed app configuration could not be read.",
            )
            .with_details(error.to_string())
        })?;
    if app.id != app_id || app.owner_sid != owner_sid {
        return Err(AppError::new(
            "configuration_ownership_mismatch",
            "The managed app failed its ownership check.",
        ));
    }
    Ok(app)
}

pub fn write(app: &ManagedApp) -> Result<(), AppError> {
    let paths = paths(&app.owner_sid, app.id)?;
    reject_reparse_components(&program_data_root()?, &paths.runtime_directory)?;
    reject_reparse_components(&program_data_root()?, &paths.configuration)?;
    let parent = paths.configuration.parent().ok_or_else(|| {
        AppError::new(
            "configuration_path_invalid",
            "The managed app location is invalid.",
        )
    })?;
    reject_reparse_components(&program_data_root()?, parent)?;
    fs::create_dir_all(parent).map_err(io_error(
        "configuration_write_failed",
        "Could not create the managed app configuration folder.",
    ))?;

    let temporary = parent.join(format!("app.{}.tmp", Uuid::new_v4().simple()));
    let encoded = serde_json::to_vec_pretty(app).map_err(|error| {
        AppError::new(
            "configuration_write_failed",
            "Could not prepare the managed app configuration.",
        )
        .with_details(error.to_string())
    })?;
    if encoded.len() as u64 > MAX_CONFIGURATION_BYTES {
        return Err(AppError::new(
            "configuration_too_large",
            "The managed app settings are too large to store safely.",
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(io_error(
            "configuration_write_failed",
            "Could not create a temporary managed app configuration.",
        ))?;
    if let Err(error) = file.write_all(&encoded).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(AppError::new(
            "configuration_write_failed",
            "Could not save the managed app configuration.",
        )
        .with_details(error.to_string()));
    }
    drop(file);
    if let Err(error) = replace_file(&temporary, &paths.configuration) {
        let _ = fs::remove_file(&temporary);
        return Err(AppError::new(
            "configuration_write_failed",
            "Could not save the managed app configuration.",
        )
        .with_details(error.to_string()));
    }
    Ok(())
}

pub fn remove_configuration(owner_sid: &str, app_id: Uuid) -> Result<(), AppError> {
    let paths = paths(owner_sid, app_id)?;
    reject_reparse_components(&program_data_root()?, &paths.configuration)?;
    if paths.configuration.exists() {
        fs::remove_file(&paths.configuration).map_err(io_error(
            "configuration_remove_failed",
            "Could not remove the managed app configuration.",
        ))?;
    }
    if let Some(directory) = paths.configuration.parent() {
        let _ = fs::remove_dir(directory);
    }
    Ok(())
}

pub fn reject_reparse_components(root: &Path, target: &Path) -> Result<(), AppError> {
    let root = root
        .canonicalize()
        .or_else(|error| {
            if !root.exists() && error.kind() == std::io::ErrorKind::NotFound {
                Ok(root.to_path_buf())
            } else {
                Err(error)
            }
        })
        .map_err(io_error(
            "managed_path_unavailable",
            "A protected StayUp folder could not be accessed.",
        ))?;
    if !target.starts_with(&root) {
        return Err(AppError::new(
            "managed_path_escape",
            "The requested path is outside the protected StayUp folder.",
        ));
    }

    let mut current = root.clone();
    for component in target
        .strip_prefix(&root)
        .map_err(|_| {
            AppError::new(
                "managed_path_escape",
                "The requested path is outside the protected StayUp folder.",
            )
        })?
        .components()
    {
        current.push(component);
        if !current.exists() {
            continue;
        }
        let metadata = fs::symlink_metadata(&current).map_err(io_error(
            "managed_path_unavailable",
            "A protected StayUp folder could not be accessed.",
        ))?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(AppError::new(
                    "managed_path_reparse_point",
                    "A protected StayUp path contains a symbolic link or reparse point.",
                ));
            }
        }
        #[cfg(not(windows))]
        if metadata.file_type().is_symlink() {
            return Err(AppError::new(
                "managed_path_reparse_point",
                "A protected StayUp path contains a symbolic link.",
            ));
        }
    }
    Ok(())
}

pub fn validate_sid(sid: &str) -> Result<(), AppError> {
    if !sid.starts_with("S-1-")
        || sid.len() > 184
        || !sid
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'S' || byte == b'-')
    {
        return Err(AppError::new("invalid_owner", "The app owner is invalid."));
    }
    Ok(())
}

fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use windows::{
        Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        },
        core::PCWSTR,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(|error| std::io::Error::other(error.to_string()))
    }
}

fn io_error(code: &'static str, message: &'static str) -> impl FnOnce(std::io::Error) -> AppError {
    move |error| AppError::new(code, message).with_details(error.to_string())
}
