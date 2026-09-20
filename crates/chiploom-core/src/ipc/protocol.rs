//! Wire types for the Core IPC protocol.
//!
//! The protocol is JSON-RPC 2.0 carried as newline-delimited JSON over stdio:
//! one complete JSON object per line, no framing headers. The extension spawns
//! `chiploom serve --stdio`, writes requests to the child's stdin and reads
//! responses from its stdout.
//!
//! Two consequences are load-bearing and are enforced elsewhere in the crate:
//!
//! * stdout carries **only** protocol frames. Logs go to stderr (see
//!   [`crate::logging`]), which the client surfaces as an output channel.
//! * every frame is a single line, so a message must not contain a raw newline.
//!   JSON string escaping guarantees this for any payload, including binary data
//!   carried as a base64 string in later phases.
//!
//! See `docs/ipc-protocol.md` for the full specification.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The only JSON-RPC version this protocol uses.
pub const JSONRPC_VERSION: &str = "2.0";

/// Standard and Chip Loom-specific JSON-RPC error codes.
pub mod codes {
    /// Invalid JSON was received.
    pub const PARSE_ERROR: i32 = -32700;
    /// The JSON is not a valid Request object.
    pub const INVALID_REQUEST: i32 = -32600;
    /// The method does not exist.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// The params do not match the method.
    pub const INVALID_PARAMS: i32 = -32602;
    /// An unexpected failure inside the server.
    pub const INTERNAL_ERROR: i32 = -32603;
    /// A request arrived before `initialize` completed.
    pub const SERVER_NOT_INITIALIZED: i32 = -32002;
    /// The request was understood and attempted, but failed.
    pub const REQUEST_FAILED: i32 = -32001;
    /// The client asked for a protocol version this build does not implement.
    pub const UNSUPPORTED_PROTOCOL_VERSION: i32 = -32000;
}

/// Method names this build serves. Also reported in
/// [`InitializeResult::capabilities`] so a client can feature-detect instead of
/// hardcoding a version comparison.
pub const METHODS: &[&str] = &[
    "initialize",
    "initialized",
    "shutdown",
    "exit",
    "$/ping",
    "core/version",
    "core/doctor",
    "core/config",
];

/// Notification methods the server may send to the client unprompted.
pub const SERVER_NOTIFICATIONS: &[&str] = &["core/ready", "core/log"];

/// A JSON-RPC request identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    /// Numeric identifier, which is what most clients use.
    Number(i64),
    /// String identifier, permitted by JSON-RPC 2.0.
    Text(String),
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(number) => write!(f, "{number}"),
            Self::Text(text) => write!(f, "{text}"),
        }
    }
}

/// A frame received from the client.
///
/// `jsonrpc` and `id` are optional at this layer so a malformed frame can be
/// answered with a proper JSON-RPC error rather than a parse failure that loses
/// the request id.
#[derive(Debug, Clone, Deserialize)]
pub struct Incoming {
    /// Must be `"2.0"`.
    #[serde(default)]
    pub jsonrpc: Option<String>,
    /// Absent for notifications.
    #[serde(default)]
    pub id: Option<Id>,
    /// The method being invoked.
    pub method: String,
    /// Method parameters.
    #[serde(default)]
    pub params: Option<Value>,
}

impl Incoming {
    /// Whether this frame expects no response.
    #[must_use]
    pub const fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}

/// An error returned in place of a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcError {
    /// One of [`codes`].
    pub code: i32,
    /// A single sentence a client can show a user directly.
    pub message: String,
    /// Structured detail: the error `kind`, and a `hint` when one applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    /// Builds an error with no structured detail.
    #[must_use]
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Attaches structured detail.
    #[must_use]
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    /// `-32601`, naming the method that does not exist.
    #[must_use]
    pub fn method_not_found(method: &str) -> Self {
        Self::new(
            codes::METHOD_NOT_FOUND,
            format!("unknown method `{method}`"),
        )
        .with_data(serde_json::json!({ "method": method, "supported": METHODS }))
    }

    /// `-32602`, explaining what was wrong with the params.
    #[must_use]
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(codes::INVALID_PARAMS, message)
    }
}

impl From<&crate::error::Error> for RpcError {
    fn from(err: &crate::error::Error) -> Self {
        let mut data = serde_json::json!({ "kind": err.kind() });
        if let Some(hint) = err.hint() {
            data["hint"] = Value::String(hint);
        }
        Self::new(codes::REQUEST_FAILED, err.to_string()).with_data(data)
    }
}

/// A response to a request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Echoes the request id. `null` when the request id could not be read.
    pub id: Option<Id>,
    /// Present on success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Present on failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    /// A successful response.
    #[must_use]
    pub fn success(id: Option<Id>, result: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_owned(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// A failed response.
    #[must_use]
    pub fn failure(id: Option<Id>, error: RpcError) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_owned(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// A server-initiated message that expects no reply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// One of [`SERVER_NOTIFICATIONS`].
    pub method: String,
    /// Payload.
    pub params: Value,
}

impl Notification {
    /// Builds a notification.
    #[must_use]
    pub fn new(method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_owned(),
            method: method.into(),
            params,
        }
    }
}

/// What the client tells the server about itself.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// Client name, e.g. `chiploom-vscode`. Logged, so sessions are attributable.
    pub client_name: String,
    /// Client version.
    #[serde(default)]
    pub client_version: Option<String>,
    /// Protocol version the client speaks. Rejected if this build cannot serve it.
    pub protocol_version: u32,
    /// Workspace folders the client has open. The first is treated as the
    /// project root for configuration purposes.
    #[serde(default)]
    pub workspace_roots: Vec<std::path::PathBuf>,
}

/// What the server tells the client about itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    /// Always `chiploom-core`.
    pub server_name: String,
    /// Release version of the core.
    pub server_version: String,
    /// Protocol version in force for this session.
    pub protocol_version: u32,
    /// What this build can do.
    pub capabilities: Capabilities,
    /// Identifier for this session, so client and server logs can be correlated.
    pub session_id: String,
    /// The server's process id, so a client can report or kill it.
    pub pid: u32,
}

/// Feature discovery. Clients branch on this, never on the version string.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// Every method this build serves.
    pub methods: Vec<String>,
    /// Every notification this build may send.
    pub notifications: Vec<String>,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            methods: METHODS.iter().map(|method| (*method).to_owned()).collect(),
            notifications: SERVER_NOTIFICATIONS
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        }
    }
}

/// Parameters for `core/doctor`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorParams {
    /// Run checks that need the network.
    #[serde(default)]
    pub online: bool,
    /// Prove directories are writable. Defaults to true, matching the CLI.
    #[serde(default = "default_true")]
    pub write_probe: bool,
}

const fn default_true() -> bool {
    true
}

impl From<DoctorParams> for crate::doctor::Options {
    fn from(params: DoctorParams) -> Self {
        Self {
            online: params.online,
            write_probe: params.write_probe,
        }
    }
}

/// Result of `$/ping`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PongResult {
    /// Milliseconds since the server started serving.
    pub uptime_ms: u64,
    /// Whether `initialize` has completed.
    pub initialized: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_round_trips() {
        let frame = r#"{"jsonrpc":"2.0","id":7,"method":"core/version"}"#;
        let incoming: Incoming = serde_json::from_str(frame).expect("parse");
        assert_eq!(incoming.method, "core/version");
        assert_eq!(incoming.id, Some(Id::Number(7)));
        assert!(!incoming.is_notification());
    }

    #[test]
    fn a_notification_has_no_id() {
        let incoming: Incoming =
            serde_json::from_str(r#"{"jsonrpc":"2.0","method":"exit"}"#).expect("parse");
        assert!(incoming.is_notification());
    }

    #[test]
    fn string_ids_are_accepted() {
        let incoming: Incoming =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":"abc","method":"$/ping"}"#)
                .expect("parse");
        assert_eq!(incoming.id, Some(Id::Text("abc".to_owned())));
    }

    #[test]
    fn a_successful_response_omits_the_error_field() {
        let response = Response::success(Some(Id::Number(1)), serde_json::json!({"ok": true}));
        let encoded = serde_json::to_string(&response).expect("encode");
        assert!(encoded.contains("\"result\""), "{encoded}");
        assert!(!encoded.contains("\"error\""), "{encoded}");
    }

    #[test]
    fn a_failed_response_omits_the_result_field() {
        let response =
            Response::failure(Some(Id::Number(1)), RpcError::method_not_found("core/nope"));
        let encoded = serde_json::to_string(&response).expect("encode");
        assert!(encoded.contains("\"error\""), "{encoded}");
        assert!(!encoded.contains("\"result\""), "{encoded}");
        // The error must tell the client what it could have called instead.
        assert!(encoded.contains("core/version"), "{encoded}");
    }

    #[test]
    fn encoded_frames_never_contain_a_raw_newline() {
        // Line-delimited framing depends on this. A message with an embedded
        // newline would be read as two frames.
        let response = Response::success(
            Some(Id::Number(1)),
            serde_json::json!({"detail": "line one\nline two\r\nline three"}),
        );
        let encoded = serde_json::to_string(&response).expect("encode");
        assert!(!encoded.contains('\n'), "{encoded}");
        assert!(!encoded.contains('\r'), "{encoded}");
    }

    #[test]
    fn doctor_params_default_to_the_cli_behaviour() {
        let params: DoctorParams = serde_json::from_str("{}").expect("parse");
        assert!(!params.online, "network checks must be opt-in");
        assert!(
            params.write_probe,
            "the write probe must stay on by default"
        );
    }

    #[test]
    fn capabilities_list_every_served_method() {
        let capabilities = Capabilities::default();
        assert!(
            capabilities
                .methods
                .iter()
                .any(|method| method == "core/doctor")
        );
        assert!(
            capabilities
                .notifications
                .iter()
                .any(|name| name == "core/ready")
        );
        assert_eq!(capabilities.methods.len(), METHODS.len());
    }

    #[test]
    fn core_errors_become_request_failed_with_a_kind() {
        let err = crate::error::Error::Environment("no USB backend".to_owned());
        let rpc = RpcError::from(&err);
        assert_eq!(rpc.code, codes::REQUEST_FAILED);
        let data = rpc.data.expect("data");
        assert_eq!(data["kind"], "environment");
        assert!(data["hint"].is_string(), "environment errors carry a hint");
    }
}
