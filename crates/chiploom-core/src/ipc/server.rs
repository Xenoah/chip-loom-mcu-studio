//! The Core IPC server: a read-dispatch-write loop over any line-oriented
//! transport.
//!
//! The loop is generic over `BufRead`/`Write` rather than hardcoding stdio, which
//! is what makes the protocol testable in-process: the tests below drive the real
//! server through in-memory buffers, and `docs/ipc-protocol.md` is verified by
//! the same code the extension talks to.

use std::io::{BufRead, Write};
use std::time::Instant;

use serde_json::Value;

use super::protocol::{
    Capabilities, DoctorParams, Id, Incoming, InitializeParams, InitializeResult, JSONRPC_VERSION,
    Notification, PongResult, Response, RpcError, codes,
};
use crate::config::Loaded;
use crate::error::{Error, ProtocolError, Result};
use crate::version::{PROTOCOL_VERSION, build_info};

/// Session lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Only `initialize`, `exit` and `$/ping` are served.
    Uninitialized,
    /// Every method is served.
    Ready,
    /// `shutdown` was received; the client is expected to send `exit` next.
    ShuttingDown,
}

/// Why the loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The client sent `exit` after `shutdown`, as the protocol prescribes.
    CleanExit,
    /// The client sent `exit` without `shutdown` first. Served, but noted: it
    /// usually means the client crashed or was killed.
    AbruptExit,
    /// The transport reached end of input. Normal when a client is terminated.
    TransportClosed,
}

impl Outcome {
    /// Whether the session ended the way the protocol prescribes.
    #[must_use]
    pub const fn is_clean(self) -> bool {
        matches!(self, Self::CleanExit)
    }
}

/// A Core IPC session.
#[derive(Debug)]
pub struct Server {
    loaded: Loaded,
    state: State,
    started: Instant,
    session_id: String,
}

impl Server {
    /// Creates a server that answers questions about `loaded`.
    #[must_use]
    pub fn new(loaded: Loaded) -> Self {
        Self {
            loaded,
            state: State::Uninitialized,
            started: Instant::now(),
            // Enough entropy to correlate two logs, cheap enough to need no
            // dependency: the pid plus a monotonic nanosecond reading.
            session_id: format!(
                "{:x}-{:x}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |elapsed| elapsed.as_nanos())
            ),
        }
    }

    /// This session's identifier, also reported in `initialize`.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Reads frames until the client exits or the transport closes.
    ///
    /// # Errors
    /// Fails only on transport failure. A malformed frame or an unknown method
    /// is answered with a JSON-RPC error and the session continues, because one
    /// bad request must not take down an editor session.
    pub fn serve<R: BufRead, W: Write>(&mut self, reader: R, mut writer: W) -> Result<Outcome> {
        let mut lines = reader.lines();

        loop {
            let Some(line) = lines.next() else {
                tracing::debug!("client closed the transport");
                return Ok(Outcome::TransportClosed);
            };
            let line = line.map_err(|source| {
                Error::Protocol(ProtocolError::Transport(format!(
                    "reading from the client failed: {source}"
                )))
            })?;

            // Blank lines are tolerated: some clients flush an empty write.
            if line.trim().is_empty() {
                continue;
            }

            match self.handle_line(&line) {
                Frame::Reply(response) => write_frame(&mut writer, &response)?,
                Frame::Silent => {}
                Frame::Ready(notification) => write_frame(&mut writer, &notification)?,
                Frame::Exit(outcome) => {
                    tracing::debug!(?outcome, "session ending");
                    return Ok(outcome);
                }
            }
        }
    }

    /// Handles one line of input. Kept separate from the I/O so the dispatch
    /// logic is testable without a transport at all.
    fn handle_line(&mut self, line: &str) -> Frame {
        let incoming: Incoming = match serde_json::from_str(line) {
            Ok(incoming) => incoming,
            Err(err) => {
                // No id is recoverable from an unparsable frame, so the response
                // carries `null`, exactly as JSON-RPC 2.0 requires.
                tracing::warn!(error = %err, "discarding malformed frame");
                return Frame::Reply(Response::failure(
                    None,
                    RpcError::new(codes::PARSE_ERROR, format!("malformed JSON frame: {err}")),
                ));
            }
        };

        if let Some(version) = incoming.jsonrpc.as_deref()
            && version != JSONRPC_VERSION
        {
            return Self::reply_error(
                incoming.id,
                RpcError::new(
                    codes::INVALID_REQUEST,
                    format!("unsupported jsonrpc version `{version}`, expected `2.0`"),
                ),
            );
        }

        tracing::trace!(method = %incoming.method, id = ?incoming.id, "dispatching");

        match incoming.method.as_str() {
            "exit" => Frame::Exit(if self.state == State::ShuttingDown {
                Outcome::CleanExit
            } else {
                Outcome::AbruptExit
            }),
            "initialized" => {
                // The client confirms it is listening; announce readiness.
                Frame::Ready(Notification::new(
                    "core/ready",
                    serde_json::json!({
                        "sessionId": self.session_id,
                        "version": build_info().version,
                    }),
                ))
            }
            method => {
                let id = incoming.id.clone();
                let is_notification = incoming.is_notification();
                let result = self.call(method, incoming.params);
                if is_notification {
                    // Notifications get no response, but a failure must not be
                    // swallowed silently or it is invisible to everyone.
                    if let Err(error) = result {
                        tracing::warn!(method, code = error.code, message = %error.message,
                            "notification handler failed");
                    }
                    return Frame::Silent;
                }
                match result {
                    Ok(value) => Frame::Reply(Response::success(id, value)),
                    Err(error) => Frame::Reply(Response::failure(id, error)),
                }
            }
        }
    }

    fn reply_error(id: Option<Id>, error: RpcError) -> Frame {
        if id.is_none() {
            Frame::Silent
        } else {
            Frame::Reply(Response::failure(id, error))
        }
    }

    /// Dispatches one method call.
    fn call(&mut self, method: &str, params: Option<Value>) -> Result<Value, RpcError> {
        // Three methods are served before the handshake: `initialize` itself,
        // `exit` (handled above) and `$/ping`, which a client uses to tell a
        // hung server from a slow one.
        if self.state == State::Uninitialized && !matches!(method, "initialize" | "$/ping") {
            let err = ProtocolError::NotInitialized {
                method: method.to_owned(),
            };
            return Err(
                RpcError::new(codes::SERVER_NOT_INITIALIZED, err.to_string()).with_data(
                    serde_json::json!({ "hint": "Send `initialize` before any other request." }),
                ),
            );
        }

        match method {
            "initialize" => self.initialize(params),
            "shutdown" => {
                self.state = State::ShuttingDown;
                Ok(Value::Null)
            }
            "$/ping" => encode(&PongResult {
                uptime_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
                initialized: self.state != State::Uninitialized,
            }),
            "core/version" => encode(&build_info()),
            "core/config" => encode(&self.loaded),
            "core/doctor" => {
                let params: DoctorParams = parse_params(params)?;
                let report = crate::doctor::run(&self.loaded, params.into());
                encode(&report)
            }
            unknown => Err(RpcError::method_not_found(unknown)),
        }
    }

    fn initialize(&mut self, params: Option<Value>) -> Result<Value, RpcError> {
        if self.state != State::Uninitialized {
            return Err(RpcError::new(
                codes::INVALID_REQUEST,
                ProtocolError::AlreadyInitialized.to_string(),
            ));
        }

        let params: InitializeParams = parse_params(params)?;

        if params.protocol_version != PROTOCOL_VERSION {
            let err = ProtocolError::VersionMismatch {
                requested: params.protocol_version,
                supported: PROTOCOL_VERSION,
            };
            return Err(
                RpcError::new(codes::UNSUPPORTED_PROTOCOL_VERSION, err.to_string()).with_data(
                    serde_json::json!({
                        "requested": params.protocol_version,
                        "supported": PROTOCOL_VERSION,
                        "hint": "Update the Chip Loom extension and the chiploom binary together.",
                    }),
                ),
            );
        }

        tracing::info!(
            client = %params.client_name,
            client_version = params.client_version.as_deref().unwrap_or("unknown"),
            session = %self.session_id,
            "session initialized"
        );

        self.state = State::Ready;

        encode(&InitializeResult {
            server_name: "chiploom-core".to_owned(),
            server_version: build_info().version.to_owned(),
            protocol_version: PROTOCOL_VERSION,
            capabilities: Capabilities::default(),
            session_id: self.session_id.clone(),
            pid: std::process::id(),
        })
    }
}

/// What the loop should do with the result of one frame.
enum Frame {
    Reply(Response),
    Ready(Notification),
    Silent,
    Exit(Outcome),
}

fn parse_params<T: serde::de::DeserializeOwned + Default>(
    params: Option<Value>,
) -> Result<T, RpcError> {
    match params {
        // Absent or explicitly null params mean "all defaults".
        None | Some(Value::Null) => Ok(T::default()),
        Some(value) => serde_json::from_value(value)
            .map_err(|err| RpcError::invalid_params(format!("invalid params: {err}"))),
    }
}

fn encode<T: serde::Serialize>(value: &T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|err| {
        RpcError::new(
            codes::INTERNAL_ERROR,
            format!("failed to encode the result: {err}"),
        )
    })
}

fn write_frame<W: Write, T: serde::Serialize>(writer: &mut W, frame: &T) -> Result<()> {
    let encoded = serde_json::to_string(frame).map_err(|err| {
        Error::Internal(format!(
            "failed to encode an outgoing protocol frame: {err}"
        ))
    })?;
    // One frame per line, flushed immediately: the client is blocking on it.
    writeln!(writer, "{encoded}")
        .and_then(|()| writer.flush())
        .map_err(|source| {
            Error::Protocol(ProtocolError::Transport(format!(
                "writing to the client failed: {source}"
            )))
        })
}

/// Deserialize support so `initialize` params can default cleanly.
impl Default for InitializeParams {
    fn default() -> Self {
        Self {
            client_name: String::new(),
            client_version: None,
            // Defaulting to a version that cannot match forces a client that
            // omits `protocolVersion` to be told so, rather than being silently
            // accepted at whatever this build happens to speak.
            protocol_version: 0,
            workspace_roots: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Loader;
    use crate::paths::Paths;

    /// Drives the real server over in-memory buffers and returns the frames it
    /// wrote, parsed.
    fn exchange(requests: &[Value]) -> (Vec<Value>, Outcome) {
        let temp = tempfile::tempdir().expect("temp dir");
        exchange_in(temp.path(), requests)
    }

    fn exchange_in(root: &std::path::Path, requests: &[Value]) -> (Vec<Value>, Outcome) {
        let paths = Paths::new(root.join("config"), root.join("data"), root.join("cache"));
        let loaded = Loader::new(root.to_path_buf(), paths)
            .load()
            .expect("load config");

        let input = requests
            .iter()
            .map(|value| serde_json::to_string(value).expect("encode request"))
            .collect::<Vec<_>>()
            .join("\n");

        let mut output = Vec::new();
        let mut server = Server::new(loaded);
        let outcome = server
            .serve(std::io::BufReader::new(input.as_bytes()), &mut output)
            .expect("serve");

        let frames = String::from_utf8(output)
            .expect("utf-8 output")
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("parse response"))
            .collect();
        (frames, outcome)
    }

    fn initialize_request(id: i64) -> Value {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "clientName": "test-client",
                "clientVersion": "0.0.0",
                "protocolVersion": PROTOCOL_VERSION,
            }
        })
    }

    #[test]
    fn a_full_session_handshakes_and_exits_cleanly() {
        let (frames, outcome) = exchange(&[
            initialize_request(1),
            serde_json::json!({"jsonrpc": "2.0", "method": "initialized"}),
            serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "core/version"}),
            serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "shutdown"}),
            serde_json::json!({"jsonrpc": "2.0", "method": "exit"}),
        ]);

        assert_eq!(outcome, Outcome::CleanExit);
        assert!(outcome.is_clean());
        // initialize response, core/ready, core/version response, shutdown response.
        assert_eq!(frames.len(), 4, "{frames:#?}");

        let init = &frames[0];
        assert_eq!(init["id"], 1);
        assert_eq!(init["result"]["serverName"], "chiploom-core");
        assert_eq!(init["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert!(init["result"]["sessionId"].is_string());
        assert!(
            init["result"]["capabilities"]["methods"]
                .as_array()
                .expect("methods array")
                .iter()
                .any(|method| method == "core/doctor")
        );

        // `initialized` is a notification, so the only frame it produces is the
        // server's own `core/ready`.
        assert_eq!(frames[1]["method"], "core/ready");
        assert!(frames[1]["params"]["sessionId"].is_string());

        assert_eq!(frames[2]["id"], 2);
        assert_eq!(frames[2]["result"]["version"], crate::version::VERSION);

        // `shutdown` answers with a null result before `exit` ends the loop.
        assert_eq!(frames[3]["id"], 3);
        assert!(frames[3]["result"].is_null());
        assert!(frames[3]["error"].is_null());
    }

    #[test]
    fn requests_before_initialize_are_refused_with_a_hint() {
        let (frames, _) = exchange(&[
            serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "core/version"}),
            serde_json::json!({"jsonrpc": "2.0", "method": "exit"}),
        ]);

        assert_eq!(frames[0]["error"]["code"], codes::SERVER_NOT_INITIALIZED);
        assert!(frames[0]["error"]["data"]["hint"].is_string());
    }

    #[test]
    fn ping_is_served_before_the_handshake() {
        // A client needs a liveness probe that works on a server that has not
        // finished starting, or it cannot tell hung from slow.
        let (frames, _) = exchange(&[
            serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "$/ping"}),
            serde_json::json!({"jsonrpc": "2.0", "method": "exit"}),
        ]);

        assert!(frames[0]["error"].is_null(), "{:#?}", frames[0]);
        assert_eq!(frames[0]["result"]["initialized"], false);
    }

    #[test]
    fn a_protocol_version_mismatch_is_refused_with_both_versions() {
        let (frames, _) = exchange(&[
            serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {"clientName": "old-client", "protocolVersion": 99}
            }),
            serde_json::json!({"jsonrpc": "2.0", "method": "exit"}),
        ]);

        let error = &frames[0]["error"];
        assert_eq!(error["code"], codes::UNSUPPORTED_PROTOCOL_VERSION);
        assert_eq!(error["data"]["requested"], 99);
        assert_eq!(error["data"]["supported"], PROTOCOL_VERSION);
    }

    #[test]
    fn a_client_that_omits_the_protocol_version_is_told_which_field_is_missing() {
        let (frames, _) = exchange(&[
            serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {"clientName": "sloppy-client"}
            }),
            serde_json::json!({"jsonrpc": "2.0", "method": "exit"}),
        ]);
        // Silently accepting it would make a future mismatch undiagnosable.
        assert_eq!(frames[0]["error"]["code"], codes::INVALID_PARAMS);
        let message = frames[0]["error"]["message"].as_str().expect("message");
        assert!(message.contains("protocolVersion"), "{message}");
    }

    #[test]
    fn initialize_with_no_params_at_all_is_a_version_mismatch() {
        // Defaulting to version 0 means "no version stated", which no build serves.
        let (frames, _) = exchange(&[
            serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}),
            serde_json::json!({"jsonrpc": "2.0", "method": "exit"}),
        ]);
        assert_eq!(
            frames[0]["error"]["code"],
            codes::UNSUPPORTED_PROTOCOL_VERSION
        );
    }

    #[test]
    fn initializing_twice_is_refused() {
        let (frames, _) = exchange(&[
            initialize_request(1),
            initialize_request(2),
            serde_json::json!({"jsonrpc": "2.0", "method": "exit"}),
        ]);

        assert!(frames[0]["result"].is_object());
        assert_eq!(frames[1]["error"]["code"], codes::INVALID_REQUEST);
    }

    #[test]
    fn a_malformed_frame_is_answered_and_the_session_survives() {
        let temp = tempfile::tempdir().expect("temp dir");
        let paths = Paths::new(
            temp.path().join("config"),
            temp.path().join("data"),
            temp.path().join("cache"),
        );
        let loaded = Loader::new(temp.path().to_path_buf(), paths)
            .load()
            .expect("load config");

        let input = format!(
            "{{ this is not json\n{}\n{}\n",
            serde_json::to_string(&initialize_request(1)).expect("encode"),
            r#"{"jsonrpc":"2.0","method":"exit"}"#
        );

        let mut output = Vec::new();
        let outcome = Server::new(loaded)
            .serve(std::io::BufReader::new(input.as_bytes()), &mut output)
            .expect("serve");

        let frames: Vec<Value> = String::from_utf8(output)
            .expect("utf-8")
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("parse"))
            .collect();

        assert_eq!(frames[0]["error"]["code"], codes::PARSE_ERROR);
        assert!(
            frames[0]["id"].is_null(),
            "an unreadable frame has no id to echo"
        );
        // The point of the test: the next request is still served.
        assert!(
            frames[1]["result"]["serverName"].is_string(),
            "{:#?}",
            frames[1]
        );
        assert_eq!(outcome, Outcome::AbruptExit);
    }

    #[test]
    fn an_unknown_method_lists_the_known_ones() {
        let (frames, _) = exchange(&[
            initialize_request(1),
            serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "core/teleport"}),
            serde_json::json!({"jsonrpc": "2.0", "method": "exit"}),
        ]);

        assert_eq!(frames[1]["error"]["code"], codes::METHOD_NOT_FOUND);
        let supported = frames[1]["error"]["data"]["supported"]
            .as_array()
            .expect("supported list")
            .len();
        assert_eq!(supported, super::super::protocol::METHODS.len());
    }

    #[test]
    fn a_wrong_jsonrpc_version_is_refused() {
        let (frames, _) = exchange(&[
            serde_json::json!({"jsonrpc": "1.0", "id": 1, "method": "$/ping"}),
            serde_json::json!({"jsonrpc": "2.0", "method": "exit"}),
        ]);
        assert_eq!(frames[0]["error"]["code"], codes::INVALID_REQUEST);
    }

    #[test]
    fn doctor_over_ipc_returns_the_same_report_shape_as_the_cli() {
        let (frames, _) = exchange(&[
            initialize_request(1),
            serde_json::json!({
                "jsonrpc": "2.0", "id": 2, "method": "core/doctor",
                "params": {"writeProbe": true}
            }),
            serde_json::json!({"jsonrpc": "2.0", "method": "exit"}),
        ]);

        let report = &frames[1]["result"];
        assert!(report["checks"].as_array().expect("checks").len() >= 10);
        assert!(report["build"]["version"].is_string());
        assert!(report["durationMs"].is_u64(), "{report:#?}");
    }

    #[test]
    fn config_over_ipc_reports_effective_values_and_provenance() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            temp.path().join(crate::paths::PROJECT_FILE),
            "[project]\nname = \"ipc-fixture\"\n[log]\nlevel = \"debug\"\n",
        )
        .expect("write project file");

        let (frames, _) = exchange_in(
            temp.path(),
            &[
                initialize_request(1),
                serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "core/config"}),
                serde_json::json!({"jsonrpc": "2.0", "method": "exit"}),
            ],
        );

        let result = &frames[1]["result"];
        assert_eq!(result["config"]["project"]["name"], "ipc-fixture");
        assert_eq!(result["config"]["log"]["level"], "debug");
        assert!(result["sources"].as_array().expect("sources").len() >= 5);
        assert!(result["paths"]["dataDir"].is_string());
    }

    #[test]
    fn a_closed_transport_ends_the_session_without_an_error() {
        // The extension being killed must not look like a crash in the core.
        let (_, outcome) = exchange(&[initialize_request(1)]);
        assert_eq!(outcome, Outcome::TransportClosed);
    }

    #[test]
    fn blank_lines_between_frames_are_ignored() {
        let temp = tempfile::tempdir().expect("temp dir");
        let paths = Paths::new(
            temp.path().join("config"),
            temp.path().join("data"),
            temp.path().join("cache"),
        );
        let loaded = Loader::new(temp.path().to_path_buf(), paths)
            .load()
            .expect("load config");

        let input = format!(
            "\n\n{}\n\n{}\n",
            serde_json::to_string(&initialize_request(1)).expect("encode"),
            r#"{"jsonrpc":"2.0","method":"exit"}"#
        );
        let mut output = Vec::new();
        Server::new(loaded)
            .serve(std::io::BufReader::new(input.as_bytes()), &mut output)
            .expect("serve");

        let frames = String::from_utf8(output)
            .expect("utf-8")
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        // Only the `initialize` response: the blank lines produced nothing.
        assert_eq!(frames, 1);
    }

    #[test]
    fn invalid_params_are_reported_as_invalid_params() {
        let (frames, _) = exchange(&[
            initialize_request(1),
            serde_json::json!({
                "jsonrpc": "2.0", "id": 2, "method": "core/doctor",
                "params": {"online": "yes please"}
            }),
            serde_json::json!({"jsonrpc": "2.0", "method": "exit"}),
        ]);
        assert_eq!(frames[1]["error"]["code"], codes::INVALID_PARAMS);
    }

    #[test]
    fn a_failing_notification_produces_no_frame() {
        let (frames, _) = exchange(&[
            initialize_request(1),
            // A notification for a method that does not exist: logged, never answered.
            serde_json::json!({"jsonrpc": "2.0", "method": "core/nope"}),
            serde_json::json!({"jsonrpc": "2.0", "method": "exit"}),
        ]);
        assert_eq!(frames.len(), 1, "{frames:#?}");
    }
}
