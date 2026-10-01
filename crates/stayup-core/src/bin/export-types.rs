use std::{fs, path::PathBuf};

use ts_rs::TS;

use stayup_core::{
    AdvancedConfig, AppAction, AppDraft, AppError, AppOverview, AppTemplate, EnvironmentVariable,
    LogResponse, LogStream, ManagedApp, ProcessConfig, RestartConfig, RestartPolicy, ServiceStatus,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Default::default();
    let mut generated = String::from(
        "// Generated from the Rust domain models. Re-run `bun run types:generate` after editing them.\n",
    );
    for definition in [
        AppError::decl(&config),
        EnvironmentVariable::decl(&config),
        ProcessConfig::decl(&config),
        RestartPolicy::decl(&config),
        RestartConfig::decl(&config),
        AdvancedConfig::decl(&config),
        AppDraft::decl(&config),
        ManagedApp::decl(&config),
        AppTemplate::decl(&config),
        AppAction::decl(&config),
        ServiceStatus::decl(&config),
        AppOverview::decl(&config),
        LogStream::decl(&config),
        LogResponse::decl(&config),
    ] {
        generated.push('\n');
        generated.push_str(&definition.replace("type ", "export type "));
        generated.push('\n');
    }

    let output_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src")
        .canonicalize()?;
    fs::write(output_directory.join("bindings.ts"), generated)?;
    Ok(())
}
