use serde::Deserialize;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;

pub const WORKER_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ClientMessage {
    Initialize {
        protocol_version: u32,
    },
    Format {
        id: u32,
        file_name: String,
        source_text: String,
        options: Map<String, Value>,
    },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ServerMessage {
    Initialized {
        protocol_version: u32,
        worker_version: String,
        oxfmt_version: String,
    },
    FormatResult {
        id: u32,
        code: String,
        errors: Vec<FormatDiagnostic>,
    },
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<u32>,
        error: WorkerError,
    },
    ShutdownComplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatDiagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub labels: Vec<DiagnosticLabel>,
    pub help_message: Option<String>,
    pub codeframe: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Advice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticLabel {
    pub message: Option<String>,
    pub start: u32,
    pub end: u32,
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

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde_json::Value;

    use super::*;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ProtocolFixture {
        name: String,
        direction: MessageDirection,
        message: Value,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    enum MessageDirection {
        Client,
        Server,
    }

    #[test]
    fn round_trips_shared_protocol_fixtures() {
        let fixtures: Vec<ProtocolFixture> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/protocol/messages.json"
        ))
        .expect("protocol fixtures should be valid JSON");

        for fixture in fixtures {
            let round_tripped = match fixture.direction {
                MessageDirection::Client => {
                    let message: ClientMessage = serde_json::from_value(fixture.message.clone())
                        .unwrap_or_else(|error| {
                            panic!("failed to deserialize {}: {error}", fixture.name)
                        });
                    serde_json::to_value(message).unwrap_or_else(|error| {
                        panic!("failed to serialize {}: {error}", fixture.name)
                    })
                }
                MessageDirection::Server => {
                    let message: ServerMessage = serde_json::from_value(fixture.message.clone())
                        .unwrap_or_else(|error| {
                            panic!("failed to deserialize {}: {error}", fixture.name)
                        });
                    serde_json::to_value(message).unwrap_or_else(|error| {
                        panic!("failed to serialize {}: {error}", fixture.name)
                    })
                }
            };

            assert_eq!(round_tripped, fixture.message, "fixture: {}", fixture.name);
        }
    }

    #[test]
    fn rejects_non_object_options() {
        let error = serde_json::from_value::<ClientMessage>(serde_json::json!({
            "type": "format",
            "id": 1,
            "fileName": "/example.ts",
            "sourceText": "",
            "options": []
        }))
        .expect_err("non-object options should fail");

        assert!(error.to_string().contains("map"));
    }

    #[test]
    fn rejects_out_of_range_request_identifiers() {
        let error = serde_json::from_value::<ClientMessage>(serde_json::json!({
            "type": "format",
            "id": u64::from(u32::MAX) + 1,
            "fileName": "/example.ts",
            "sourceText": "",
            "options": {}
        }))
        .expect_err("out-of-range request identifiers should fail");

        assert!(error.to_string().contains("u32"));
    }

    #[test]
    fn rejects_an_unknown_message_type() {
        let error = serde_json::from_value::<ClientMessage>(serde_json::json!({
            "type": "unknown"
        }))
        .expect_err("unknown message types should fail");

        assert!(error.to_string().contains("unknown variant"));
    }
}
