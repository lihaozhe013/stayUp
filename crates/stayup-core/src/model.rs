use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EnvironmentVariable {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProcessConfig {
    pub executable: String,
    pub arguments: String,
    pub working_directory: String,
    #[serde(default)]
    pub environment: Vec<EnvironmentVariable>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppDraft {
    pub name: String,
    pub process: ProcessConfig,
    pub startup_enabled: bool,
    pub restart: RestartConfig,
    pub logging_enabled: bool,
    pub advanced: AdvancedConfig,
}

impl AppDraft {
    pub fn into_app(self, owner_sid: String) -> ManagedApp {
        let mut app = ManagedApp::new(self.name, owner_sid, self.process);
        app.startup_enabled = self.startup_enabled;
        app.restart = self.restart;
        app.logging_enabled = self.logging_enabled;
        app.advanced = self.advanced;
        app
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RestartPolicy {
    #[default]
    Never,
    OnFailure,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestartConfig {
    pub policy: RestartPolicy,
    pub delay_seconds: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdvancedConfig {
    pub stop_timeout_seconds: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ManagedApp {
    #[ts(type = "string")]
    pub id: Uuid,
    #[ts(type = "number")]
    pub revision: u64,
    pub name: String,
    pub owner_sid: String,
    pub process: ProcessConfig,
    pub startup_enabled: bool,
    pub restart: RestartConfig,
    pub logging_enabled: bool,
    pub advanced: AdvancedConfig,
    pub created_at: String,
    pub updated_at: String,
    pub schema_version: u32,
    pub backend: String,
    pub winsw_version: String,
}

impl ManagedApp {
    pub fn new(name: String, owner_sid: String, process: ProcessConfig) -> Self {
        let now = now_timestamp();
        Self {
            id: Uuid::new_v4(),
            revision: 1,
            name,
            owner_sid,
            process,
            startup_enabled: true,
            restart: RestartConfig {
                policy: RestartPolicy::OnFailure,
                delay_seconds: 5,
            },
            logging_enabled: true,
            advanced: AdvancedConfig {
                stop_timeout_seconds: 15,
            },
            created_at: now.clone(),
            updated_at: now,
            schema_version: 1,
            backend: "windowsService".to_owned(),
            winsw_version: crate::WINSW_VERSION.to_owned(),
        }
    }

    pub fn service_id(&self) -> String {
        format!("stayup{}", self.id.simple())
    }

    pub fn replace_from(&mut self, mut draft: Self) {
        draft.id = self.id;
        draft.revision = self.revision.saturating_add(1);
        draft.owner_sid = self.owner_sid.clone();
        draft.created_at = self.created_at.clone();
        draft.schema_version = self.schema_version;
        draft.backend = self.backend.clone();
        draft.winsw_version = self.winsw_version.clone();
        draft.updated_at = now_timestamp();
        *self = draft;
    }

    pub fn environment_map(&self) -> BTreeMap<String, String> {
        self.process
            .environment
            .iter()
            .map(|variable| (variable.name.to_ascii_uppercase(), variable.value.clone()))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AppTemplate {
    BackgroundApplication,
    LocalServer,
    BackgroundWorker,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ServiceStatus {
    Running,
    StartPending,
    StopPending,
    Stopped,
    Missing,
    Unknown,
    NeedsAttention,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppOverview {
    pub app: ManagedApp,
    pub status: ServiceStatus,
    pub status_detail: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AppAction {
    Start,
    Stop,
    Restart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LogStream {
    Stdout,
    Stderr,
    Diagnostic,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LogResponse {
    pub content: String,
    pub truncated: bool,
    pub byte_limit: u32,
}

impl AppTemplate {
    pub fn defaults(self) -> TemplateDefaults {
        let custom = matches!(self, Self::Custom);
        TemplateDefaults {
            startup_enabled: !custom,
            restart: RestartConfig {
                policy: if custom {
                    RestartPolicy::Never
                } else {
                    RestartPolicy::OnFailure
                },
                delay_seconds: 5,
            },
            logging_enabled: true,
            stop_timeout_seconds: 15,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateDefaults {
    pub startup_enabled: bool,
    pub restart: RestartConfig,
    pub logging_enabled: bool,
    pub stop_timeout_seconds: u32,
}

fn now_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format!("{seconds}")
}
