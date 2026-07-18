use std::fmt::Display;
use std::fmt::Formatter;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;

use crate::runtime_locator::RuntimeLocator;
use crate::worker_protocol::ClientMessage;
use crate::worker_protocol::FormatDiagnostic;
use crate::worker_protocol::ServerMessage;
use crate::worker_protocol::WORKER_PROTOCOL_VERSION;
use crate::worker_protocol::WorkerError;
use crate::worker_protocol::WorkerErrorKind;
use serde_json::Map;
use serde_json::Value;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::ChildStdin;
use tokio::process::ChildStdout;
use tokio::process::Command;
use tokio::sync::Mutex;

pub const EXPECTED_OXFMT_VERSION: &str = "0.59.0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerFormatResult {
    pub code: String,
    pub errors: Vec<FormatDiagnostic>,
}

pub struct WorkerManager {
    locator: RuntimeLocator,
    next_request_id: AtomicU32,
    session: Mutex<Option<WorkerSession>>,
}

impl WorkerManager {
    #[must_use]
    pub fn new(locator: RuntimeLocator) -> Self {
        Self {
            locator,
            next_request_id: AtomicU32::new(1),
            session: Mutex::new(None),
        }
    }

    /// Formats one file using the long-lived Oxfmt worker.
    ///
    /// # Errors
    ///
    /// Returns an error when options are not an object, the worker cannot be
    /// started, the protocol is invalid, or Oxfmt reports a worker error.
    pub async fn format(
        &self,
        file_name: &Path,
        source_text: &str,
        options: &Value,
    ) -> Result<WorkerFormatResult, WorkerManagerError> {
        let options = options
            .as_object()
            .cloned()
            .ok_or(WorkerManagerError::InvalidOptions)?;
        let request_id = self
            .next_request_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| WorkerManagerError::RequestIdExhausted)?;
        let mut session = self.session.lock().await;
        if session.is_none() {
            *session = Some(WorkerSession::spawn(&self.locator).await?);
        }

        let result = session
            .as_mut()
            .ok_or_else(|| {
                WorkerManagerError::Protocol("worker session was not initialized".to_owned())
            })?
            .format(request_id, file_name, source_text, options)
            .await;
        if result.is_err() {
            session.take();
        }
        result
    }

    /// Shuts down the worker session, if one is running.
    ///
    /// # Errors
    ///
    /// Returns an error when the shutdown message cannot be sent or the worker
    /// does not acknowledge it.
    pub async fn shutdown(&self) -> Result<(), WorkerManagerError> {
        let mut session = self.session.lock().await;
        if let Some(mut worker) = session.take() {
            worker.shutdown().await?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum WorkerManagerError {
    InvalidOptions,
    RequestIdExhausted,
    Io(std::io::Error),
    Json(serde_json::Error),
    Protocol(String),
    Remote(WorkerError),
}

impl Display for WorkerManagerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOptions => formatter.write_str("worker options must be a JSON object"),
            Self::RequestIdExhausted => formatter.write_str("worker request id space exhausted"),
            Self::Io(error) => write!(formatter, "worker I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "worker message was invalid JSON: {error}"),
            Self::Protocol(message) => write!(formatter, "worker protocol error: {message}"),
            Self::Remote(error) => write!(
                formatter,
                "worker returned {} error: {}",
                error.kind, error.message
            ),
        }
    }
}

impl std::error::Error for WorkerManagerError {}

impl From<std::io::Error> for WorkerManagerError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for WorkerManagerError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<WorkerError> for WorkerManagerError {
    fn from(error: WorkerError) -> Self {
        Self::Remote(error)
    }
}

struct WorkerSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl WorkerSession {
    async fn spawn(locator: &RuntimeLocator) -> Result<Self, WorkerManagerError> {
        let mut child = Command::new(locator.node_program())
            .arg(locator.worker_entry())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| WorkerManagerError::Protocol("worker stdin was not piped".to_owned()))?;
        let stdout = child.stdout.take().ok_or_else(|| {
            WorkerManagerError::Protocol("worker stdout was not piped".to_owned())
        })?;
        let mut session = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        };

        session
            .write(ClientMessage::Initialize {
                protocol_version: WORKER_PROTOCOL_VERSION,
            })
            .await?;
        match session.read().await? {
            ServerMessage::Initialized {
                protocol_version,
                oxfmt_version,
                ..
            } if protocol_version == WORKER_PROTOCOL_VERSION
                && oxfmt_version == EXPECTED_OXFMT_VERSION =>
            {
                Ok(session)
            }
            ServerMessage::Initialized {
                protocol_version,
                oxfmt_version,
                ..
            } => Err(WorkerManagerError::Protocol(format!(
                "worker handshake mismatch: protocol {protocol_version}, Oxfmt {oxfmt_version}"
            ))),
            ServerMessage::Error { error, .. } => Err(error.into()),
            response => Err(WorkerManagerError::Protocol(format!(
                "unexpected handshake response: {response:?}"
            ))),
        }
    }

    async fn format(
        &mut self,
        id: u32,
        file_name: &Path,
        source_text: &str,
        options: Map<String, Value>,
    ) -> Result<WorkerFormatResult, WorkerManagerError> {
        self.write(ClientMessage::Format {
            id,
            file_name: file_name.to_string_lossy().into_owned(),
            source_text: source_text.to_owned(),
            options,
        })
        .await?;
        match self.read().await? {
            ServerMessage::FormatResult {
                id: response_id,
                code,
                errors,
            } if response_id == id => Ok(WorkerFormatResult { code, errors }),
            ServerMessage::Error { error, .. } => Err(error.into()),
            ServerMessage::FormatResult {
                id: response_id, ..
            } => Err(WorkerManagerError::Protocol(format!(
                "response id mismatch: expected {id}, received {response_id}"
            ))),
            response => Err(WorkerManagerError::Protocol(format!(
                "unexpected format response: {response:?}"
            ))),
        }
    }

    async fn shutdown(&mut self) -> Result<(), WorkerManagerError> {
        self.write(ClientMessage::Shutdown).await?;
        match self.read().await? {
            ServerMessage::ShutdownComplete => {
                self.child.wait().await?;
                Ok(())
            }
            ServerMessage::Error { error, .. } => Err(error.into()),
            response => Err(WorkerManagerError::Protocol(format!(
                "unexpected shutdown response: {response:?}"
            ))),
        }
    }

    async fn write(&mut self, message: ClientMessage) -> Result<(), WorkerManagerError> {
        let line = serde_json::to_string(&message)?;
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn read(&mut self) -> Result<ServerMessage, WorkerManagerError> {
        let mut line = String::new();
        if self.stdout.read_line(&mut line).await? == 0 {
            return Err(WorkerManagerError::Protocol(
                "worker exited before sending a response".to_owned(),
            ));
        }
        Ok(serde_json::from_str(&line)?)
    }
}

impl Drop for WorkerSession {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

impl Display for WorkerErrorKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Protocol => "protocol",
            Self::Format => "format",
            Self::Internal => "internal",
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use serde_json::json;
    use tokio::runtime::Builder;

    use super::*;

    const FAKE_WORKER: &str = r#"
import readline from "node:readline";

const input = readline.createInterface({ input: process.stdin });
let count = 0;
for await (const line of input) {
  const message = JSON.parse(line);
  if (message.type === "initialize") {
    console.log(JSON.stringify({
      type: "initialized",
      protocolVersion: 1,
      workerVersion: "test",
      oxfmtVersion: "0.59.0",
    }));
  } else if (message.type === "format") {
    count += 1;
    console.log(JSON.stringify({
      type: "formatResult",
      id: message.id,
      code: `${message.sourceText}:${count}`,
      errors: [],
    }));
  } else if (message.type === "shutdown") {
    console.log(JSON.stringify({ type: "shutdownComplete" }));
    input.close();
    break;
  }
}
"#;

    fn fake_worker_path() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "dprint-plugin-oxfmt-worker-test-{}.mjs",
            std::process::id()
        ));
        fs::write(&path, FAKE_WORKER).expect("fake worker should be writable");
        path
    }

    #[test]
    fn reuses_one_worker_for_sequential_requests() {
        let worker_path = fake_worker_path();
        let locator = RuntimeLocator::new(PathBuf::from("node"), worker_path.clone());
        let manager = WorkerManager::new(locator);
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime should build");

        runtime.block_on(async {
            let first = manager
                .format(Path::new("/tmp/first.ts"), "first", &json!({}))
                .await
                .expect("first format should succeed");
            assert_eq!(first.code, "first:1");

            let second = manager
                .format(Path::new("/tmp/second.ts"), "second", &json!({}))
                .await
                .expect("second format should succeed");
            assert_eq!(second.code, "second:2");
            assert!(second.errors.is_empty());

            manager.shutdown().await.expect("shutdown should succeed");
        });

        fs::remove_file(worker_path).expect("fake worker should be removable");
    }

    #[test]
    fn rejects_non_object_options_before_starting_worker() {
        let manager = WorkerManager::new(RuntimeLocator::new(
            PathBuf::from("missing-node"),
            PathBuf::from("missing-worker.js"),
        ));
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime should build");

        let error = runtime
            .block_on(manager.format(Path::new("/tmp/example.ts"), "", &json!(null)))
            .expect_err("non-object options should fail");
        assert!(matches!(error, WorkerManagerError::InvalidOptions));
    }
}
