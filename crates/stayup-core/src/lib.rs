use ts_rs::TS;

pub mod model;
pub mod validation;
pub mod winsw;

pub use model::{
    AdvancedConfig, AppAction, AppDraft, AppOverview, AppTemplate, EnvironmentVariable,
    LogResponse, LogStream, ManagedApp, ProcessConfig, RestartConfig, RestartPolicy, ServiceStatus,
};

pub const WINSW_VERSION: &str = "2.12.0";
pub const MAX_APP_CONFIG_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for AppError {}

pub trait RunnerBackend {
    fn list_owned(&self, owner_sid: &str) -> Result<Vec<AppOverview>, AppError>;

    fn create(
        &self,
        owner_sid: &str,
        draft: AppDraft,
        start_now: bool,
    ) -> Result<AppOverview, AppError>;

    fn update(
        &self,
        owner_sid: &str,
        app_id: uuid::Uuid,
        expected_revision: u64,
        draft: AppDraft,
    ) -> Result<AppOverview, AppError>;

    fn act(
        &self,
        owner_sid: &str,
        app_id: uuid::Uuid,
        action: AppAction,
    ) -> Result<AppOverview, AppError>;

    fn set_startup(
        &self,
        owner_sid: &str,
        app_id: uuid::Uuid,
        enabled: bool,
    ) -> Result<AppOverview, AppError>;

    fn remove(
        &self,
        owner_sid: &str,
        app_id: uuid::Uuid,
        remove_logs: bool,
    ) -> Result<(), AppError>;
}

#[cfg(test)]
mod tests;
