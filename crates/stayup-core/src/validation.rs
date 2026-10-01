use std::collections::BTreeSet;
use std::path::Path;

use crate::{AppError, MAX_APP_CONFIG_BYTES, ManagedApp};

const MAX_NAME_LENGTH: usize = 80;
const MAX_ARGUMENTS_LENGTH: usize = 16_384;
const MAX_ENVIRONMENT_VALUE_LENGTH: usize = 32_768;
const MAX_ENVIRONMENT_VARIABLES: usize = 64;

pub fn validate_app(app: &ManagedApp, check_files: bool) -> Result<(), AppError> {
    let name = app.name.trim();
    if name.is_empty() || name.len() > MAX_NAME_LENGTH {
        return Err(AppError::new(
            "invalid_name",
            "Enter a name that is between 1 and 80 characters long.",
        ));
    }
    if !is_xml_text(name) {
        return Err(AppError::new(
            "invalid_name",
            "The app name contains characters that cannot be used in a service configuration.",
        ));
    }
    if app.owner_sid.is_empty() || app.owner_sid.len() > 184 {
        return Err(AppError::new("invalid_owner", "The app owner is invalid."));
    }
    if app.backend != "windowsService" || app.schema_version != 1 {
        return Err(AppError::new(
            "unsupported_configuration",
            "This configuration version is not supported.",
        ));
    }

    let executable = app.process.executable.trim();
    let working_directory = app.process.working_directory.trim();
    if executable.is_empty() || !Path::new(executable).is_absolute() {
        return Err(AppError::new(
            "invalid_executable",
            "Choose an executable using its full local file path.",
        ));
    }
    if !is_xml_text(executable) {
        return Err(AppError::new(
            "invalid_executable",
            "The executable path contains unsupported characters.",
        ));
    }
    if executable.starts_with(r"\\") || executable.starts_with(r"\?") {
        return Err(AppError::new(
            "unsupported_executable_path",
            "Choose an executable on a local drive.",
        ));
    }
    if working_directory.is_empty() || !Path::new(working_directory).is_absolute() {
        return Err(AppError::new(
            "invalid_working_directory",
            "Choose a working folder using its full local path.",
        ));
    }
    if !is_xml_text(working_directory) {
        return Err(AppError::new(
            "invalid_working_directory",
            "The working folder contains unsupported characters.",
        ));
    }
    if working_directory.starts_with(r"\\") || working_directory.starts_with(r"\?") {
        return Err(AppError::new(
            "unsupported_working_directory",
            "Choose a working folder on a local drive.",
        ));
    }
    if app.process.arguments.len() > MAX_ARGUMENTS_LENGTH || !is_xml_text(&app.process.arguments) {
        return Err(AppError::new(
            "invalid_arguments",
            "The argument list is too long or contains unsupported characters.",
        ));
    }
    if app.restart.delay_seconds > 86_400 || app.advanced.stop_timeout_seconds > 600 {
        return Err(AppError::new(
            "invalid_timing",
            "Choose a restart delay of at most 24 hours and a stop timeout of at most 10 minutes.",
        ));
    }
    if app.process.environment.len() > MAX_ENVIRONMENT_VARIABLES {
        return Err(AppError::new(
            "too_many_environment_variables",
            "Enter no more than 64 environment variables.",
        ));
    }

    let mut names = BTreeSet::new();
    for variable in &app.process.environment {
        let name = variable.name.trim();
        if !is_environment_name(name) || !names.insert(name.to_ascii_uppercase()) {
            return Err(AppError::new(
                "invalid_environment_variable",
                "Environment variable names must be valid and unique, ignoring letter case.",
            ));
        }
        if is_reserved_environment_name(name) {
            return Err(AppError::new(
                "reserved_environment_variable",
                "This environment variable name is reserved by WinSW.",
            ));
        }
        if variable.value.len() > MAX_ENVIRONMENT_VALUE_LENGTH
            || !is_xml_text(&variable.name)
            || !is_xml_text(&variable.value)
        {
            return Err(AppError::new(
                "invalid_environment_value",
                "An environment variable value is too long or contains unsupported characters.",
            ));
        }
    }

    let serialized_size = serde_json::to_vec_pretty(app)
        .map_err(|error| {
            AppError::new(
                "configuration_invalid",
                "Could not validate the app settings.",
            )
            .with_details(error.to_string())
        })?
        .len();
    if serialized_size > MAX_APP_CONFIG_BYTES {
        return Err(AppError::new(
            "configuration_too_large",
            "The app settings are too large to store safely. Reduce the environment variable values and try again.",
        ));
    }

    if check_files {
        let executable_path = Path::new(executable);
        if !executable_path.is_file() {
            return Err(AppError::new(
                "executable_not_found",
                "The selected executable could not be found.",
            ));
        }
        if !Path::new(working_directory).is_dir() {
            return Err(AppError::new(
                "working_directory_not_found",
                "The selected working folder could not be found.",
            ));
        }
    }

    Ok(())
}

fn is_environment_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|character| character.is_ascii_alphanumeric() || character == b'_')
        && name
            .bytes()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == b'_')
}

fn is_xml_text(value: &str) -> bool {
    value.chars().all(|character| {
        matches!(
            character as u32,
            0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF
        )
    })
}

fn is_reserved_environment_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    matches!(upper.as_str(), "BASE" | "SERVICE_ID") || upper.starts_with("WINSW_")
}
