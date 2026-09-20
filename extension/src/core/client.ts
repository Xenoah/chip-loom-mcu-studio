/**
 * The JSON-RPC client that talks to `chiploom serve --stdio`.
 *
 * Responsibilities, and the reasoning behind each:
 *
 * * **One frame per line.** stdout carries only protocol frames, so a line
 *   reader is sufficient and cheap. Anything the core wants to say to a human
 *   arrives on stderr instead, and is forwarded to `onLog`.
 * * **Every request times out.** A core that hangs must surface as a failed
 *   command, not as a spinner that never stops.
 * * **Failure is always attributable.** If the child dies, every in-flight
 *   request rejects with the exit status and the tail of stderr, because "request
 *   failed" on its own is not something a user can act on.
 *
 * Deliberately free of any `vscode` import: `test/protocol.test.ts` drives this
 * exact class against a real built binary, which is how Core<->extension
 * communication is verified without launching an editor.
 */

import { type ChildProcessWithoutNullStreams, spawn } from 'node:child_process';
import { createInterface, type Interface } from 'node:readline';

import {
  CoreRpcError,
  type IncomingFrame,
  type InitializeResult,
  JSONRPC_VERSION,
  type JsonValue,
  PROTOCOL_VERSION,
  type RequestId,
} from './protocol';

/** How the client was configured. */
export interface CoreClientOptions {
  /** Path to the `chiploom` executable. */
  executable: string;
  /** Working directory for the child process. */
  cwd?: string | undefined;
  /** Workspace folders to advertise in `initialize`. */
  workspaceRoots?: readonly string[];
  /** Log level to ask the core for. */
  logLevel?: string | undefined;
  /** How long `initialize` may take. */
  startupTimeoutMs?: number;
  /** How long any other request may take. */
  requestTimeoutMs?: number;
  /** Extra environment for the child. */
  env?: NodeJS.ProcessEnv | undefined;
  /** Receives every line the core writes to stderr. */
  onLog?: (line: string) => void;
  /** Receives every notification the core sends. */
  onNotification?: (method: string, params: JsonValue | undefined) => void;
  /** Called once when the child exits, however it exits. */
  onExit?: (code: number | null, signal: NodeJS.Signals | null) => void;
  /** Identifies this client in the core's logs. */
  clientName?: string;
  /** This client's own version. */
  clientVersion?: string;
}

interface Pending {
  resolve: (value: JsonValue) => void;
  reject: (reason: Error) => void;
  timer: NodeJS.Timeout;
  method: string;
}

/** How many stderr lines to keep for attributing a crash. */
const STDERR_TAIL_LINES = 20;

/** A live session with the Chip Loom core. */
export class CoreClient {
  private readonly child: ChildProcessWithoutNullStreams;
  private readonly stdoutReader: Interface;
  private readonly stderrReader: Interface;
  private readonly pending = new Map<RequestId, Pending>();
  private readonly stderrTail: string[] = [];
  private readonly options: CoreClientOptions;
  private nextId = 1;
  private exitReason: Error | undefined;
  private disposed = false;
  private handshake: InitializeResult | undefined;

  private constructor(options: CoreClientOptions) {
    this.options = options;

    this.child = spawn(options.executable, ['serve', '--stdio'], {
      cwd: options.cwd,
      env: { ...process.env, ...options.env },
      stdio: ['pipe', 'pipe', 'pipe'],
      windowsHide: true,
    }) as ChildProcessWithoutNullStreams;

    this.stdoutReader = createInterface({ input: this.child.stdout });
    this.stdoutReader.on('line', (line) => this.handleFrame(line));

    this.stderrReader = createInterface({ input: this.child.stderr });
    this.stderrReader.on('line', (line) => {
      this.stderrTail.push(line);
      if (this.stderrTail.length > STDERR_TAIL_LINES) {
        this.stderrTail.shift();
      }
      options.onLog?.(line);
    });

    this.child.on('error', (error) => {
      this.failAll(new Error(`Could not start the Chip Loom core: ${error.message}`));
    });

    this.child.on('exit', (code, signal) => {
      const how = signal ? `signal ${signal}` : `exit code ${code}`;
      const tail = this.stderrTail.length > 0 ? `\n\n${this.stderrTail.join('\n')}` : '';
      this.failAll(new Error(`The Chip Loom core stopped (${how}).${tail}`));
      options.onExit?.(code, signal);
    });
  }

  /**
   * Starts the core and completes the `initialize` handshake.
   *
   * Resolves only once the core has agreed on a protocol version, so a caller
   * that gets a client back has a usable one.
   */
  static async start(options: CoreClientOptions): Promise<CoreClient> {
    const client = new CoreClient(options);
    try {
      client.handshake = await client.initialize();
    } catch (error) {
      await client.dispose();
      throw error;
    }
    // Tells the core the client is listening; it answers with `core/ready`.
    client.notify('initialized');
    return client;
  }

  /** The handshake result, available once {@link start} has resolved. */
  get session(): InitializeResult {
    if (!this.handshake) {
      throw new Error('The Chip Loom core has not completed its handshake yet.');
    }
    return this.handshake;
  }

  /** Whether the core is still running. */
  get running(): boolean {
    return !this.disposed && this.exitReason === undefined && this.child.exitCode === null;
  }

  /** Whether the core serves `method`, according to the handshake. */
  supports(method: string): boolean {
    return this.handshake?.capabilities.methods.includes(method) ?? false;
  }

  private async initialize(): Promise<InitializeResult> {
    const params = {
      clientName: this.options.clientName ?? 'chiploom-vscode',
      clientVersion: this.options.clientVersion ?? '0.0.0',
      protocolVersion: PROTOCOL_VERSION,
      workspaceRoots: [...(this.options.workspaceRoots ?? [])],
    };
    const result = await this.request(
      'initialize',
      params as unknown as JsonValue,
      this.options.startupTimeoutMs ?? 15_000,
    );
    return result as unknown as InitializeResult;
  }

  /**
   * Sends a request and resolves with its result.
   *
   * @throws {CoreRpcError} when the core answers with an error.
   * @throws {Error} when the request times out or the core stops.
   */
  request(method: string, params?: JsonValue, timeoutMs?: number): Promise<JsonValue> {
    if (this.exitReason) {
      return Promise.reject(this.exitReason);
    }
    if (this.disposed) {
      return Promise.reject(new Error('This Chip Loom core session has been disposed.'));
    }

    const id = this.nextId++;
    const limit = timeoutMs ?? this.options.requestTimeoutMs ?? 30_000;

    return new Promise<JsonValue>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`The Chip Loom core did not answer \`${method}\` within ${limit} ms.`));
      }, limit);
      // A pending request must never hold the process open on its own.
      timer.unref?.();

      this.pending.set(id, { resolve, reject, timer, method });
      this.send({ jsonrpc: JSONRPC_VERSION, id, method, ...(params === undefined ? {} : { params }) });
    });
  }

  /** Sends a notification, which expects no reply. */
  notify(method: string, params?: JsonValue): void {
    if (!this.running) {
      return;
    }
    this.send({ jsonrpc: JSONRPC_VERSION, method, ...(params === undefined ? {} : { params }) });
  }

  /**
   * Shuts the core down, politely first.
   *
   * Sends `shutdown` then `exit` and waits briefly for the child to go; kills it
   * only if it does not. A closing editor window must never leave an orphan.
   */
  async dispose(): Promise<void> {
    if (this.disposed) {
      return;
    }
    this.disposed = true;

    if (this.child.exitCode === null && this.child.signalCode === null) {
      try {
        await this.request('shutdown', undefined, 2_000);
      } catch {
        // Already gone, or unwilling; `exit` and the kill below still apply.
      }
      try {
        this.send({ jsonrpc: JSONRPC_VERSION, method: 'exit' });
        this.child.stdin.end();
      } catch {
        // The pipe is already closed.
      }
      await this.waitForExit(2_000);
    }

    this.stdoutReader.close();
    this.stderrReader.close();
    this.failAll(new Error('The Chip Loom core session was closed.'));
  }

  private async waitForExit(timeoutMs: number): Promise<void> {
    if (this.child.exitCode !== null || this.child.signalCode !== null) {
      return;
    }
    await new Promise<void>((resolve) => {
      const timer = setTimeout(() => {
        this.child.kill('SIGKILL');
        resolve();
      }, timeoutMs);
      timer.unref?.();
      this.child.once('exit', () => {
        clearTimeout(timer);
        resolve();
      });
    });
  }

  private send(frame: Record<string, unknown>): void {
    // One line per frame, which is the whole framing rule.
    this.child.stdin.write(`${JSON.stringify(frame)}\n`);
  }

  private handleFrame(line: string): void {
    if (line.trim().length === 0) {
      return;
    }

    let frame: IncomingFrame;
    try {
      frame = JSON.parse(line) as IncomingFrame;
    } catch {
      this.options.onLog?.(`[client] discarded an unparsable frame: ${line}`);
      return;
    }

    if (frame.id === undefined || frame.id === null) {
      if (frame.method) {
        this.options.onNotification?.(frame.method, frame.params);
      }
      return;
    }

    const pending = this.pending.get(frame.id);
    if (!pending) {
      this.options.onLog?.(`[client] response for unknown request ${String(frame.id)}`);
      return;
    }
    this.pending.delete(frame.id);
    clearTimeout(pending.timer);

    if (frame.error) {
      pending.reject(CoreRpcError.from(frame.error));
    } else {
      pending.resolve(frame.result ?? null);
    }
  }

  private failAll(reason: Error): void {
    this.exitReason ??= reason;
    for (const [, pending] of this.pending) {
      clearTimeout(pending.timer);
      pending.reject(reason);
    }
    this.pending.clear();
  }
}
