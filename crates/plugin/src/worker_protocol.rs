use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

pub const WORKER_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientMessage {
    Initialize {
        protocol_version: u32,
    },
    Format {
        id: u64,
        file_name: String,
        source_text: String,
        options: Value,
    },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerMessage {
    Initialized {
        protocol_version: u32,
        worker_version: String,
        oxfmt_version: String,
    },
    FormatResult {
        id: u64,
        code: String,
    },
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<u64>,
        error: WorkerError,
    },
    ShutdownComplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerError {
    pub kind: WorkerErrorKind,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkerErrorKind {
    Protocol,
    Format,
    Internal,
}
