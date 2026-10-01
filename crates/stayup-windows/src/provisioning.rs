use std::{
    fs,
    io::Write,
    os::windows::{ffi::OsStrExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Output},
    thread,
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};
use stayup_core::{AppDraft, AppError, ManagedApp, ServiceStatus, validation::validate_app};
use uuid::Uuid;
use windows::{
    Win32::{
        Foundation::HLOCAL,
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            },
            DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
        },
        System::{
            Services::{
                OpenSCManagerW, OpenServiceW, SC_MANAGER_CONNECT, SERVICE_QUERY_STATUS,
                SetServiceObjectSecurity,
            },
            SystemInformation::{GetSystemDirectoryW, GetWindowsDirectoryW},
        },
    },
    core::{PCWSTR, PWSTR},
};

use crate::{
    helper::HelperOperation,
    platform::{self, windows_error},
    scm,
    storage::{self, AppPaths},
};

const WINSW_SHA256: &str = "b5066b7bbdfba1293e5d15cda3caaea88fbeab35bd5b38c41c913d492aadfc4f";
const CREATE_NO_WINDOW: u32 = 0x08000000;
const MAX_OPERATION_OUTPUT: usize = 16 * 1024;

pub fn apply_request(
    owner_sid: &str,
    operation: HelperOperation,
) -> Result<Option<Uuid>, AppError> {
    storage::validate_sid(owner_sid)?;
    match operation {
        HelperOperation::Create { draft, start_now } => {
            create(owner_sid, draft, start_now).map(Some)
        }
        HelperOperation::Update {
            app_id,
            expected_revision,
            draft,
        } => update(owner_sid, app_id, expected_revision, draft).map(|()| None),
        HelperOperation::SetStartup { app_id, enabled } => {
            set_startup(owner_sid, app_id, enabled).map(|()| None)
        }
        HelperOperation::Remove {
            app_id,
            remove_logs,
        } => remove(owner_sid, app_id, remove_logs).map(|()| None),
    }
}

pub fn remove_all_managed_services() -> Result<(), AppError> {
    let root = platform::program_data_root()?;
    storage::reject_reparse_components(root.parent().unwrap_or(&root), &root)?;
    let managed = root.join("Managed");
    if !managed.exists() {
        return Ok(());
    }
    storage::reject_reparse_components(&root, &managed)?;
    for owner_entry in fs::read_dir(&managed).map_err(io_error(
        "uninstall_enumeration_failed",
        "Could not read the managed app folders.",
    ))? {
        let owner_entry = owner_entry.map_err(io_error(
            "uninstall_enumeration_failed",
            "Could not read the managed app folders.",
        ))?;
        let Some(owner_sid) = owner_entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        storage::validate_sid(&owner_sid)?;
        storage::reject_reparse_components(&managed, &owner_entry.path())?;
        if !owner_entry
            .file_type()
            .map_err(io_error(
                "uninstall_enumeration_failed",
                "Could not inspect a managed app folder.",
            ))?
            .is_dir()
        {
            continue;
        }
        for app in storage::load_owned(&owner_sid)? {
            remove(&owner_sid, app.id, false).map_err(|error| {
                AppError::new(
                    "uninstall_cleanup_failed",
                    format!(
                        "Could not remove {} ({}) during uninstall.",
                        app.name, app.id
                    ),
                )
                .with_details(format!(
                    "{} {}",
                    error.message,
                    error.details.unwrap_or_default()
                ))
            })?;
        }
    }
    Ok(())
}

fn create(owner_sid: &str, draft: AppDraft, start_now: bool) -> Result<Uuid, AppError> {
    platform::ensure_winsw_framework()?;
    let app = draft.into_app(owner_sid.to_owned());
    validate_app(&app, true)?;
    let paths = storage::paths(owner_sid, app.id)?;
    ensure_managed_layout(owner_sid, &app, &paths)?;
    let mut install_attempted = false;
    let result: Result<(), AppError> = (|| {
        install_runtime(&paths)?;
        let operation_logs = operation_log_directory(owner_sid, app.id)?;
        let install_xml = render_xml(&app, &operation_logs.to_string_lossy())?;
        write_private_file(&paths.xml, install_xml.as_bytes())?;
        install_attempted = true;
        run_winsw(&paths, &["install"])?;

        scm::set_startup_and_recovery(&app)?;
        apply_service_acl(&app)?;
        let service_sid = service_account_sid(&app.service_id())?;
        prepare_app_acl(owner_sid, &paths, &service_sid)?;
        let xml = render_xml(&app, &paths.log_directory.to_string_lossy())?;
        write_private_file(&paths.xml, xml.as_bytes())?;
        storage::write(&app)?;
        Ok(())
    })();

    if let Err(error) = result {
        record_admin_operation(
            owner_sid,
            app.id,
            "create",
            &format!(
                "Failed: {} {}",
                error.message,
                error.details.as_deref().unwrap_or_default()
            ),
        );
        let service_exists = !matches!(scm::service_status(&app.service_id()), Err(ref status_error) if status_error.code == "service_missing");
        if install_attempted
            && service_exists
            && let Err(remove_error) = uninstall_wrapper(&app, &paths)
        {
            let previous = error.details.clone().unwrap_or_default();
            return Err(error.with_details(format!(
                "{previous} Cleanup also failed: {}",
                remove_error.message
            )));
        }
        let _ = storage::remove_configuration(owner_sid, app.id);
        let _ = remove_directory_checked(&paths.runtime_directory);
        return Err(error);
    }
    if start_now && let Err(error) = scm::act(&app, stayup_core::AppAction::Start) {
        record_admin_operation(
            owner_sid,
            app.id,
            "start after create",
            &format!(
                "Failed: {} {}",
                error.message,
                error.details.as_deref().unwrap_or_default()
            ),
        );
    }
    record_admin_operation(
        owner_sid,
        app.id,
        "create",
        if start_now {
            "Completed and started."
        } else {
            "Completed."
        },
    );
    Ok(app.id)
}

fn update(
    owner_sid: &str,
    app_id: Uuid,
    expected_revision: u64,
    draft: AppDraft,
) -> Result<(), AppError> {
    let old_app = storage::load(owner_sid, app_id)?;
    if old_app.revision != expected_revision {
        return Err(AppError::new(
            "revision_conflict",
            "This app changed while you were editing it. Refresh the app list and try again.",
        ));
    }
    let paths = storage::paths(owner_sid, app_id)?;
    scm::ensure_registered_service(&old_app, &paths.wrapper)?;
    let old_xml = fs::read(&paths.xml).map_err(io_error(
        "service_configuration_read_failed",
        "Could not read the current generated service configuration.",
    ))?;
    let mut updated = draft.into_app(owner_sid.to_owned());
    updated.id = app_id;
    updated.revision = old_app.revision.saturating_add(1);
    updated.created_at = old_app.created_at.clone();
    updated.updated_at = format!("{}", now_seconds());
    updated.winsw_version = old_app.winsw_version.clone();
    validate_app(&updated, true)?;

    let previous_state = scm::service_status(&old_app.service_id())?;
    let was_running = matches!(
        previous_state,
        ServiceStatus::Running | ServiceStatus::StartPending
    );
    if was_running {
        scm::act(&old_app, stayup_core::AppAction::Stop)?;
    }

    let new_xml = render_xml(&updated, &paths.log_directory.to_string_lossy())?;
    let operation: Result<(), AppError> = (|| {
        write_private_file(&paths.xml, new_xml.as_bytes())?;
        scm::set_startup_and_recovery(&updated)?;
        storage::write(&updated)?;
        Ok(())
    })();
    if let Err(error) = operation {
        record_admin_operation(
            owner_sid,
            app_id,
            "update",
            &format!(
                "Failed: {} {}",
                error.message,
                error.details.as_deref().unwrap_or_default()
            ),
        );
        let rollback_xml = write_private_file(&paths.xml, &old_xml);
        let rollback_scm = scm::set_startup_and_recovery(&old_app);
        let rollback_json = storage::write(&old_app);
        let mut details = error.details.clone().unwrap_or_default();
        if rollback_xml.is_err() || rollback_scm.is_err() || rollback_json.is_err() {
            details.push_str(" The previous configuration could not be fully restored; review the app before starting it.");
        }
        if was_running {
            let _ = scm::act(&old_app, stayup_core::AppAction::Start);
        }
        return Err(error.with_details(details));
    }
    if was_running && let Err(error) = scm::act(&updated, stayup_core::AppAction::Start) {
        record_admin_operation(
            owner_sid,
            app_id,
            "update restart",
            &format!(
                "Failed: {} {}",
                error.message,
                error.details.as_deref().unwrap_or_default()
            ),
        );
    }
    record_admin_operation(owner_sid, app_id, "update", "Completed.");
    Ok(())
}

fn set_startup(owner_sid: &str, app_id: Uuid, enabled: bool) -> Result<(), AppError> {
    let mut app = storage::load(owner_sid, app_id)?;
    let paths = storage::paths(owner_sid, app_id)?;
    scm::ensure_registered_service(&app, &paths.wrapper)?;
    if app.startup_enabled == enabled {
        return Ok(());
    }
    let old_app = app.clone();
    let old_xml = fs::read(&paths.xml).map_err(io_error(
        "service_configuration_read_failed",
        "Could not read the current generated service configuration.",
    ))?;
    app.startup_enabled = enabled;
    app.revision = app.revision.saturating_add(1);
    app.updated_at = format!("{}", now_seconds());
    let xml = render_xml(&app, &paths.log_directory.to_string_lossy())?;
    write_private_file(&paths.xml, xml.as_bytes())?;
    if let Err(error) = scm::set_startup_and_recovery(&app).and_then(|()| storage::write(&app)) {
        record_admin_operation(
            owner_sid,
            app_id,
            "startup setting",
            &format!(
                "Failed: {} {}",
                error.message,
                error.details.as_deref().unwrap_or_default()
            ),
        );
        let _ = write_private_file(&paths.xml, &old_xml);
        let _ = scm::set_startup_and_recovery(&old_app);
        let _ = storage::write(&old_app);
        return Err(error);
    }
    record_admin_operation(
        owner_sid,
        app_id,
        "startup setting",
        if enabled {
            "Automatic startup enabled."
        } else {
            "Automatic startup disabled."
        },
    );
    Ok(())
}

fn remove(owner_sid: &str, app_id: Uuid, remove_logs: bool) -> Result<(), AppError> {
    let app = storage::load(owner_sid, app_id)?;
    let paths = storage::paths(owner_sid, app_id)?;
    let result = (|| {
        match scm::ensure_registered_service(&app, &paths.wrapper) {
            Ok(()) => uninstall_wrapper(&app, &paths)?,
            Err(error) if error.code == "service_missing" => {}
            Err(error) => return Err(error),
        }
        storage::remove_configuration(owner_sid, app_id)?;
        remove_directory_checked(&paths.runtime_directory)?;
        if remove_logs {
            remove_directory_checked(&paths.log_directory)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            record_admin_operation(owner_sid, app_id, "remove", "Completed.");
            Ok(())
        }
        Err(error) => {
            record_admin_operation(
                owner_sid,
                app_id,
                "remove",
                &format!(
                    "Failed: {} {}",
                    error.message,
                    error.details.as_deref().unwrap_or_default()
                ),
            );
            Err(error)
        }
    }
}

fn uninstall_wrapper(app: &ManagedApp, paths: &AppPaths) -> Result<(), AppError> {
    scm::ensure_registered_service(app, &paths.wrapper)?;
    match run_winsw(paths, &["stop"]) {
        Ok(_) => {}
        Err(_error)
            if scm::service_status(&app.service_id()).ok() == Some(ServiceStatus::Stopped) => {}
        Err(error) => return Err(error),
    }
    run_winsw(paths, &["uninstall"])?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        match scm::service_status(&app.service_id()) {
            Err(error) if error.code == "service_missing" => return Ok(()),
            _ => thread::sleep(Duration::from_millis(250)),
        }
    }
    Err(AppError::new(
        "service_remove_pending",
        "Windows has not finished removing the app’s service yet.",
    ))
}

fn ensure_managed_layout(
    owner_sid: &str,
    app: &ManagedApp,
    paths: &AppPaths,
) -> Result<(), AppError> {
    let root = platform::program_data_root()?;
    storage::reject_reparse_components(root.parent().unwrap_or(&root), &root)?;
    fs::create_dir_all(&root).map_err(io_error(
        "storage_setup_failed",
        "Could not create the protected app storage folder.",
    ))?;
    set_acl(
        &root,
        &[
            ("*S-1-5-18", "(OI)(CI)F"),
            ("*S-1-5-32-544", "(OI)(CI)F"),
            ("*S-1-5-11", "(X)"),
        ],
    )?;

    for directory in [
        root.join("Managed"),
        root.join("Runtimes"),
        root.join("Logs"),
        operation_log_directory(owner_sid, app.id)?,
    ] {
        fs::create_dir_all(&directory).map_err(io_error(
            "storage_setup_failed",
            "Could not create a protected app storage folder.",
        ))?;
        set_acl(
            &directory,
            &[
                ("*S-1-5-18", "(OI)(CI)F"),
                ("*S-1-5-32-544", "(OI)(CI)F"),
                ("*S-1-5-11", "(X)"),
            ],
        )?;
    }

    let managed_owner = paths
        .configuration
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| {
            AppError::new(
                "managed_path_invalid",
                "The managed configuration folder is invalid.",
            )
        })?;
    let runtime_owner = paths.runtime_directory.parent().ok_or_else(|| {
        AppError::new(
            "managed_path_invalid",
            "The managed runtime folder is invalid.",
        )
    })?;
    let logs_owner = paths.log_directory.parent().ok_or_else(|| {
        AppError::new("managed_path_invalid", "The managed log folder is invalid.")
    })?;
    for directory in [managed_owner, runtime_owner, logs_owner] {
        fs::create_dir_all(directory).map_err(io_error(
            "storage_setup_failed",
            "Could not create an app owner folder.",
        ))?;
        set_acl(
            directory,
            &[
                ("*S-1-5-18", "(OI)(CI)F"),
                ("*S-1-5-32-544", "(OI)(CI)F"),
                (&format!("*{owner_sid}"), "(OI)(CI)RX"),
            ],
        )?;
    }
    for directory in [
        paths.configuration.parent().unwrap().to_path_buf(),
        paths.runtime_directory.clone(),
        paths.log_directory.clone(),
    ] {
        fs::create_dir_all(&directory).map_err(io_error(
            "storage_setup_failed",
            "Could not create an app storage folder.",
        ))?;
        storage::reject_reparse_components(&root, &directory)?;
    }
    set_acl(
        paths.configuration.parent().unwrap(),
        &[
            ("*S-1-5-18", "(OI)(CI)F"),
            ("*S-1-5-32-544", "(OI)(CI)F"),
            (&format!("*{owner_sid}"), "(OI)(CI)RX"),
        ],
    )?;
    set_acl(
        &paths.runtime_directory,
        &[
            ("*S-1-5-18", "(OI)(CI)F"),
            ("*S-1-5-32-544", "(OI)(CI)F"),
            (&format!("*{owner_sid}"), "(OI)(CI)RX"),
        ],
    )?;
    set_acl(
        &paths.log_directory,
        &[
            ("*S-1-5-18", "(OI)(CI)F"),
            ("*S-1-5-32-544", "(OI)(CI)F"),
            (&format!("*{owner_sid}"), "(OI)(CI)RX"),
        ],
    )?;
    Ok(())
}

fn prepare_app_acl(owner_sid: &str, paths: &AppPaths, service_sid: &str) -> Result<(), AppError> {
    let root = platform::program_data_root()?;
    storage::reject_reparse_components(&root, &paths.runtime_directory)?;
    storage::reject_reparse_components(&root, &paths.log_directory)?;
    set_acl(
        &paths.runtime_directory,
        &[
            ("*S-1-5-18", "(OI)(CI)F"),
            ("*S-1-5-32-544", "(OI)(CI)F"),
            (&format!("*{owner_sid}"), "(OI)(CI)RX"),
            (&format!("*{service_sid}"), "(OI)(CI)RX"),
        ],
    )?;
    set_acl(
        &paths.log_directory,
        &[
            ("*S-1-5-18", "(OI)(CI)F"),
            ("*S-1-5-32-544", "(OI)(CI)F"),
            (&format!("*{owner_sid}"), "(OI)(CI)RX"),
            (&format!("*{service_sid}"), "(OI)(CI)M"),
        ],
    )?;
    set_acl(
        paths.configuration.parent().unwrap(),
        &[
            ("*S-1-5-18", "(OI)(CI)F"),
            ("*S-1-5-32-544", "(OI)(CI)F"),
            (&format!("*{owner_sid}"), "(OI)(CI)RX"),
            (&format!("*{service_sid}"), "(OI)(CI)RX"),
        ],
    )?;
    Ok(())
}

fn install_runtime(paths: &AppPaths) -> Result<(), AppError> {
    let installed_root = platform::program_files_root()?;
    let candidates = [
        installed_root.join("resources/winsw/WinSW.NET461.exe"),
        installed_root.join("resources/WinSW.NET461.exe"),
        installed_root.join("WinSW.NET461.exe"),
    ];
    let source = candidates.iter().find(|path| path.is_file())
        .ok_or_else(|| AppError::new("winsw_missing", "The bundled service runtime is missing. Repair the StayUp installation and try again."))?;
    storage::reject_reparse_components(&installed_root, source)?;
    let bytes = fs::read(source).map_err(io_error(
        "winsw_read_failed",
        "Could not read the bundled service runtime.",
    ))?;
    let checksum = sha256_hex(Sha256::digest(&bytes).as_slice());
    if checksum != WINSW_SHA256 {
        return Err(AppError::new(
            "winsw_integrity_failed",
            "The bundled service runtime failed its integrity check.",
        ));
    }
    fs::create_dir_all(&paths.runtime_directory).map_err(io_error(
        "winsw_install_failed",
        "Could not create the private service runtime folder.",
    ))?;
    storage::reject_reparse_components(&platform::program_data_root()?, &paths.runtime_directory)?;
    storage::reject_reparse_components(&platform::program_data_root()?, &paths.wrapper)?;
    fs::copy(source, &paths.wrapper).map_err(io_error(
        "winsw_install_failed",
        "Could not copy the private service runtime.",
    ))?;
    let copied = fs::read(&paths.wrapper).map_err(io_error(
        "winsw_install_failed",
        "Could not verify the private service runtime.",
    ))?;
    if sha256_hex(Sha256::digest(&copied).as_slice()) != WINSW_SHA256 {
        return Err(AppError::new(
            "winsw_integrity_failed",
            "The private service runtime failed its integrity check.",
        ));
    }
    Ok(())
}

fn render_xml(app: &ManagedApp, log_directory: &str) -> Result<String, AppError> {
    let service_descriptor = format!(
        "D:P(A;;RCLRPWP;;;{})(A;;GA;;;SY)(A;;GA;;;BA)",
        app.owner_sid
    );
    stayup_core::winsw::render_service_config(app, log_directory, &service_descriptor)
}

fn run_winsw(paths: &AppPaths, arguments: &[&str]) -> Result<Output, AppError> {
    storage::reject_reparse_components(&platform::program_data_root()?, &paths.wrapper)?;
    let windows_directory = windows_directory()?;
    let system_path = windows_directory.join("System32");
    let operation_logs = platform::program_data_root()?.join("Operations");
    let output = Command::new(&paths.wrapper)
        .args(arguments)
        .current_dir(&paths.runtime_directory)
        .env_clear()
        .env("SystemRoot", &windows_directory)
        .env("WINDIR", &windows_directory)
        .env("PATH", system_path)
        .env("TEMP", &operation_logs)
        .env("TMP", &operation_logs)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(io_error(
            "winsw_operation_failed",
            "Could not run the service manager.",
        ))?;
    if !output.status.success() {
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let text = if diagnostic.trim().is_empty() {
            stdout.trim()
        } else {
            diagnostic.trim()
        };
        return Err(AppError::new(
            "winsw_operation_failed",
            "The Windows service could not be changed.",
        )
        .with_details(truncate_details(text)));
    }
    Ok(output)
}

fn apply_service_acl(app: &ManagedApp) -> Result<(), AppError> {
    let manager = unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) }
        .map_err(|error| {
            windows_error(
                "service_manager_unavailable",
                "Windows Service Control Manager could not be opened.",
                error,
            )
        })?;
    let name = wide(&app.service_id());
    let service = unsafe {
        OpenServiceW(
            manager,
            PCWSTR(name.as_ptr()),
            SERVICE_QUERY_STATUS | 0x0004_0000,
        )
    }
    .map_err(|error| {
        windows_error(
            "service_install_failed",
            "Windows did not register the app’s service.",
            error,
        )
    })?;
    let descriptor = format!(
        "D:P(A;;RCLRPWP;;;{})(A;;GA;;;SY)(A;;GA;;;BA)",
        app.owner_sid
    );
    let descriptor_wide = wide(&descriptor);
    unsafe {
        let mut security_descriptor = PSECURITY_DESCRIPTOR::default();
        let security_result = ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(descriptor_wide.as_ptr()),
            1,
            &mut security_descriptor,
            None,
        );
        let result = match security_result {
            Ok(()) => {
                let result = SetServiceObjectSecurity(
                    service,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    security_descriptor,
                );
                let _ = windows::Win32::Foundation::LocalFree(Some(HLOCAL(
                    security_descriptor.0.cast(),
                )));
                result
            }
            Err(error) => Err(error),
        };
        let _ = windows::Win32::System::Services::CloseServiceHandle(service);
        let _ = windows::Win32::System::Services::CloseServiceHandle(manager);
        result.map_err(|error| {
            windows_error(
                "service_permissions_failed",
                "Could not grant your Windows account permission to manage this app.",
                error,
            )
        })?;
    }
    Ok(())
}

fn service_account_sid(service_id: &str) -> Result<String, AppError> {
    let account = format!("NT SERVICE\\{service_id}");
    let account_wide = wide(&account);
    unsafe {
        let mut sid_bytes = 0u32;
        let mut domain_bytes = 0u32;
        let mut use_type = windows::Win32::Security::SID_NAME_USE::default();
        let _ = windows::Win32::Security::LookupAccountNameW(
            PCWSTR::null(),
            PCWSTR(account_wide.as_ptr()),
            None,
            &mut sid_bytes,
            None,
            &mut domain_bytes,
            &mut use_type,
        );
        if sid_bytes == 0 || sid_bytes > 4096 {
            return Err(AppError::new(
                "service_identity_unavailable",
                "Windows could not create the app’s private service identity.",
            ));
        }
        let mut sid = vec![0u64; (sid_bytes as usize).div_ceil(8)];
        let mut domain = vec![0u16; domain_bytes as usize];
        windows::Win32::Security::LookupAccountNameW(
            PCWSTR::null(),
            PCWSTR(account_wide.as_ptr()),
            Some(windows::Win32::Security::PSID(sid.as_mut_ptr().cast())),
            &mut sid_bytes,
            Some(PWSTR(domain.as_mut_ptr())),
            &mut domain_bytes,
            &mut use_type,
        )
        .map_err(|error| {
            windows_error(
                "service_identity_unavailable",
                "Windows could not create the app’s private service identity.",
                error,
            )
        })?;
        let mut sid_string = PWSTR::null();
        ConvertSidToStringSidW(
            windows::Win32::Security::PSID(sid.as_mut_ptr().cast()),
            &mut sid_string,
        )
        .map_err(|error| {
            windows_error(
                "service_identity_unavailable",
                "Windows could not create the app’s private service identity.",
                error,
            )
        })?;
        let result = sid_string.to_string().unwrap_or_default();
        let _ = windows::Win32::Foundation::LocalFree(Some(HLOCAL(sid_string.0.cast())));
        Ok(result)
    }
}

fn set_acl(path: &Path, entries: &[(&str, &str)]) -> Result<(), AppError> {
    let system = system_directory()?;
    let icacls = system.join("icacls.exe");
    if !icacls.is_file() {
        return Err(AppError::new(
            "acl_tool_missing",
            "Windows file permissions could not be updated.",
        ));
    }
    let mut command = Command::new(icacls);
    command.arg(path).arg("/inheritance:r").arg("/grant:r");
    for (account, rights) in entries {
        command.arg(format!("{account}:{rights}"));
    }
    let output = command
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(io_error(
            "acl_update_failed",
            "Could not apply the protected app permissions.",
        ))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AppError::new(
            "acl_update_failed",
            "Could not apply the protected app permissions.",
        )
        .with_details(truncate_details(&String::from_utf8_lossy(&output.stderr))))
    }
}

fn system_directory() -> Result<PathBuf, AppError> {
    let mut buffer = vec![0u16; 32_768];
    let length = unsafe { GetSystemDirectoryW(Some(&mut buffer)) } as usize;
    if length == 0 || length >= buffer.len() {
        return Err(AppError::new(
            "system_directory_unavailable",
            "A required Windows system folder could not be located.",
        ));
    }
    buffer.truncate(length + 1);
    Ok(platform::windows_path_from_wide(&buffer))
}

fn windows_directory() -> Result<PathBuf, AppError> {
    let mut buffer = vec![0u16; 32_768];
    let length = unsafe { GetWindowsDirectoryW(Some(&mut buffer)) } as usize;
    if length == 0 || length >= buffer.len() {
        return Err(AppError::new(
            "system_directory_unavailable",
            "A required Windows system folder could not be located.",
        ));
    }
    buffer.truncate(length + 1);
    Ok(platform::windows_path_from_wide(&buffer))
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path.parent().ok_or_else(|| {
        AppError::new(
            "managed_path_invalid",
            "The protected configuration path is invalid.",
        )
    })?;
    storage::reject_reparse_components(&platform::program_data_root()?, path)?;
    let temporary = parent.join(format!(".stayup-{}.tmp", Uuid::new_v4().simple()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(io_error(
            "service_configuration_write_failed",
            "Could not create a protected service configuration.",
        ))?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(AppError::new(
            "service_configuration_write_failed",
            "Could not save the protected service configuration.",
        )
        .with_details(error.to_string()));
    }
    drop(file);
    let source: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        windows::Win32::Storage::FileSystem::MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            windows::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                | windows::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|error| {
        windows_error(
            "service_configuration_write_failed",
            "Could not save the protected service configuration.",
            error,
        )
    })
}

fn remove_directory_checked(path: &Path) -> Result<(), AppError> {
    let root = platform::program_data_root()?;
    storage::reject_reparse_components(&root, path)?;
    if path.exists() {
        fs::remove_dir_all(path).map_err(io_error(
            "managed_file_cleanup_failed",
            "Could not remove a protected StayUp folder.",
        ))?;
    }
    Ok(())
}

fn operation_log_directory(owner_sid: &str, app_id: Uuid) -> Result<PathBuf, AppError> {
    Ok(platform::program_data_root()?
        .join("Operations")
        .join(owner_sid)
        .join(app_id.to_string()))
}

fn record_admin_operation(owner_sid: &str, app_id: Uuid, operation: &str, result: &str) {
    let Ok(directory) = operation_log_directory(owner_sid, app_id) else {
        return;
    };
    let Ok(root) = platform::program_data_root() else {
        return;
    };
    if storage::reject_reparse_components(&root, &directory).is_err() {
        return;
    }
    if fs::create_dir_all(&directory).is_err() {
        return;
    }
    let _ = set_acl(
        &directory,
        &[("*S-1-5-18", "(OI)(CI)F"), ("*S-1-5-32-544", "(OI)(CI)F")],
    );
    let timestamp = now_seconds();
    let line = format!(
        "{timestamp} {operation}: {}\n",
        result.replace(['\r', '\n'], " ")
    );
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("operations.log"))
    {
        let _ = file.write_all(line.as_bytes());
    }
}

fn truncate_details(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() <= MAX_OPERATION_OUTPUT {
        trimmed.to_owned()
    } else {
        trimmed[..MAX_OPERATION_OUTPUT].to_owned()
    }
}

fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

fn io_error(code: &'static str, message: &'static str) -> impl FnOnce(std::io::Error) -> AppError {
    move |error| AppError::new(code, message).with_details(error.to_string())
}
