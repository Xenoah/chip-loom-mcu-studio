/**
 * TypeScript mirror of the Core IPC protocol.
 *
 * These declarations exist so the extension is type-checked against the same
 * contract `chiploom-core` implements. The authoritative definition is
 * `crates/chiploom-core/src/ipc/protocol.rs`, and `docs/ipc-protocol.md`
 * describes it in prose; when the Rust side changes, these change with it and
 * `PROTOCOL_VERSION` moves.
 */

/** The protocol version this extension speaks. */
export const PROTOCOL_VERSION = 1;

/** The only JSON-RPC version used. */
export const JSONRPC_VERSION = '2.0';

/** JSON-RPC and Chip Loom error codes. */
export const ErrorCodes = {
  parseError: -32700,
  invalidRequest: -32600,
  methodNotFound: -32601,
  invalidParams: -32602,
  internalError: -32603,
  serverNotInitialized: -32002,
  requestFailed: -32001,
  unsupportedProtocolVersion: -32000,
} as const;

/** A JSON value, which is all a protocol payload can be. */
export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

/** A request identifier. */
export type RequestId = number | string;

/** The error object carried by a failed response. */
export interface RpcErrorBody {
  code: number;
  message: string;
  data?: { kind?: string; hint?: string } & Record<string, JsonValue>;
}

/** A frame received from the core. */
export interface IncomingFrame {
  jsonrpc: string;
  id?: RequestId | null;
  method?: string;
  params?: JsonValue;
  result?: JsonValue;
  error?: RpcErrorBody;
}

/** What the client tells the core about itself. */
export interface InitializeParams {
  clientName: string;
  clientVersion: string;
  protocolVersion: number;
  workspaceRoots: string[];
}

/** What the core tells the client about itself. */
export interface InitializeResult {
  serverName: string;
  serverVersion: string;
  protocolVersion: number;
  capabilities: { methods: string[]; notifications: string[] };
  sessionId: string;
  pid: number;
}

/** Build provenance, from `core/version`. */
export interface BuildInfo {
  version: string;
  protocolVersion: number;
  commit: string;
  dirty: boolean;
  target: string;
  host: string;
  profile: string;
  rustc: string;
  builtAt: string;
}

/** The status of one diagnostic. */
export type CheckStatus = 'ok' | 'warn' | 'error' | 'skipped';

/** One line of a diagnostics report. */
export interface DoctorCheck {
  id: string;
  title: string;
  status: CheckStatus;
  detail: string;
  hint: string | null;
}

/** A complete diagnostics report, from `core/doctor`. */
export interface DoctorReport {
  build: BuildInfo;
  checks: DoctorCheck[];
  durationMs: number;
}

/** Parameters for `core/doctor`. */
export interface DoctorParams {
  online?: boolean;
  writeProbe?: boolean;
}

/** The effective configuration and where it came from, from `core/config`. */
export interface ConfigReport {
  config: {
    project: { name: string | null };
    paths: { data_dir: string | null; cache_dir: string | null };
    log: { level: string; format: string; file: string | null; timestamps: boolean };
    network: { offline: boolean; timeout_secs: number; retries: number };
  };
  paths: {
    configDir: string;
    globalConfigFile: string;
    dataDir: string;
    toolchainsDir: string;
    targetPacksDir: string;
    cacheDir: string;
    logDir: string;
    projectRoot: string | null;
  };
  sources: Array<{ kind: string; path: string | null; status: string }>;
  warnings: Array<{ code: string; message: string }>;
}

/** Payload of the `core/ready` notification. */
export interface ReadyNotification {
  sessionId: string;
  version: string;
}

/**
 * An error reported by the core, rather than by the transport.
 *
 * Carries the structured fields so the extension can show the core's own hint
 * instead of inventing one.
 */
export class CoreRpcError extends Error {
  constructor(
    readonly code: number,
    message: string,
    readonly hint?: string,
    readonly kind?: string,
  ) {
    super(message);
    this.name = 'CoreRpcError';
  }

  static from(body: RpcErrorBody): CoreRpcError {
    return new CoreRpcError(body.code, body.message, body.data?.hint, body.data?.kind);
  }

  /** The sentence to show a user: the failure, plus what to do about it. */
  get displayMessage(): string {
    return this.hint ? `${this.message}\n\n${this.hint}` : this.message;
  }
}
