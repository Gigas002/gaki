use crate::infra::logging::LaunchSession;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    pub timestamp: u64,
    pub level: LogLevel,
    pub event_type: String,
    pub message: String,
    pub stage: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// `events.toml` as read back: every `[[event]]`, oldest first.
#[derive(Debug, Default, Deserialize)]
pub struct EventLog {
    #[serde(default)]
    pub event: Vec<LogEvent>,
}

impl EventLog {
    pub fn parse(text: &str) -> Result<Self> {
        Ok(toml::from_str(text)?)
    }
}

pub struct EventLogger {
    file_path: PathBuf,
}

impl EventLogger {
    pub fn new(session: &LaunchSession) -> Result<Self> {
        std::fs::create_dir_all(&session.log_dir)?;
        let file_path = session.event_log_path();
        Ok(Self { file_path })
    }

    pub fn log(
        &self,
        level: LogLevel,
        event_type: &str,
        message: String,
        stage: Option<String>,
        metadata: HashMap<String, String>,
    ) -> Result<()> {
        let event = LogEvent {
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            level,
            event_type: event_type.to_string(),
            message,
            stage,
            metadata: super::redact_sensitive(metadata),
        };

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)?;

        // One `[[event]]` table per event: appending keeps the file valid TOML.
        #[derive(Serialize)]
        struct One<'a> {
            event: [&'a LogEvent; 1],
        }
        writeln!(file, "{}", toml::to_string(&One { event: [&event] })?)?;

        Ok(())
    }

    pub fn info(
        &self,
        event_type: &str,
        message: String,
        stage: Option<String>,
        metadata: HashMap<String, String>,
    ) -> Result<()> {
        self.log(LogLevel::Info, event_type, message, stage, metadata)
    }

    pub fn error(
        &self,
        event_type: &str,
        message: String,
        stage: Option<String>,
        metadata: HashMap<String, String>,
    ) -> Result<()> {
        self.log(LogLevel::Error, event_type, message, stage, metadata)
    }
}
