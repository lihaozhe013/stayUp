use serde::Serialize;

use crate::{AppError, ManagedApp, RestartPolicy};

#[derive(Serialize)]
#[serde(rename = "service")]
struct ServiceXml {
    id: String,
    name: String,
    description: String,
    executable: String,
    arguments: String,
    workingdirectory: String,
    startmode: &'static str,
    stoptimeout: String,
    serviceaccount: ServiceAccountXml,
    env: Vec<EnvironmentXml>,
    logpath: String,
    log: LogXml,
    onfailure: OnFailureXml,
    resetfailure: &'static str,
    #[serde(rename = "securityDescriptor")]
    securitydescriptor: String,
}

#[derive(Serialize)]
struct ServiceAccountXml {
    domain: &'static str,
    user: &'static str,
}

#[derive(Serialize)]
struct EnvironmentXml {
    #[serde(rename = "@name")]
    name: String,
    #[serde(rename = "@value")]
    value: String,
}

#[derive(Serialize)]
struct LogXml {
    #[serde(rename = "@mode")]
    mode: &'static str,
    #[serde(rename = "sizeThreshold")]
    size_threshold: u32,
    #[serde(rename = "keepFiles")]
    keep_files: u32,
}

#[derive(Serialize)]
struct OnFailureXml {
    #[serde(rename = "@action")]
    action: &'static str,
    #[serde(rename = "@delay")]
    delay: String,
}

pub fn render_service_config(
    app: &ManagedApp,
    log_directory: &str,
    security_descriptor: &str,
) -> Result<String, AppError> {
    let on_failure = match app.restart.policy {
        RestartPolicy::Never => "none",
        RestartPolicy::OnFailure => "restart",
    };
    let xml = ServiceXml {
        id: app.service_id(),
        name: app.name.trim().to_owned(),
        description: format!("Managed by StayUp: {}", app.name.trim()),
        executable: app.process.executable.clone(),
        arguments: app.process.arguments.clone(),
        workingdirectory: app.process.working_directory.clone(),
        startmode: if app.startup_enabled {
            "Automatic"
        } else {
            "Manual"
        },
        stoptimeout: format!("{}sec", app.advanced.stop_timeout_seconds),
        serviceaccount: ServiceAccountXml {
            domain: "NT AUTHORITY",
            user: "LocalService",
        },
        env: app
            .process
            .environment
            .iter()
            .map(|variable| EnvironmentXml {
                name: variable.name.clone(),
                value: variable.value.clone(),
            })
            .collect(),
        logpath: log_directory.to_owned(),
        log: if app.logging_enabled {
            LogXml {
                mode: "roll-by-size",
                size_threshold: 10 * 1024,
                keep_files: 5,
            }
        } else {
            LogXml {
                mode: "none",
                size_threshold: 10 * 1024,
                keep_files: 5,
            }
        },
        onfailure: OnFailureXml {
            action: on_failure,
            delay: format!("{} sec", app.restart.delay_seconds),
        },
        resetfailure: "1 day",
        securitydescriptor: security_descriptor.to_owned(),
    };

    quick_xml::se::to_string(&xml)
        .map(|xml| format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n{xml}\n"))
        .map_err(|error| {
            AppError::new(
                "winsw_configuration_error",
                "Could not generate the service configuration.",
            )
            .with_details(error.to_string())
        })
}
