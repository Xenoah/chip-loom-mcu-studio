/**
 * Owns the lifetime of one connection to the Chip Loom core.
 *
 * Everything that can fail while starting the core is handled here and turned
 * into a state the status bar can show and a message the output channel can
 * explain, so no failure reaches the user as an unhandled rejection.
 */

import * as vscode from 'vscode';

import { CoreClient } from './core/client';
import { CoreNotFoundError, locateCore } from './core/locate';
import {
  type ConfigReport,
  CoreRpcError,
  type DoctorParams,
  type DoctorReport,
  type BuildInfo,
  type JsonValue,
  type ReadyNotification,
} from './core/protocol';
import type { OutputLog } from './ui/output';
import type { StatusBar } from './ui/statusBar';

/** Manages starting, restarting and stopping the core. */
export class CoreSession implements vscode.Disposable {
  private client: CoreClient | undefined;
  private starting: Promise<CoreClient | undefined> | undefined;

  constructor(
    private readonly log: OutputLog,
    private readonly status: StatusBar,
    private readonly extensionVersion: string,
  ) {}

  /** The live client, or `undefined` when the core is not running. */
  get current(): CoreClient | undefined {
    return this.client?.running === true ? this.client : undefined;
  }

  /**
   * Starts the core if it is not already running.
   *
   * Concurrent calls share one attempt, so two commands issued at once cannot
   * spawn two cores.
   */
  async start(): Promise<CoreClient | undefined> {
    if (this.current) {
      return this.current;
    }
    this.starting ??= this.startOnce().finally(() => {
      this.starting = undefined;
    });
    return this.starting;
  }

  private async startOnce(): Promise<CoreClient | undefined> {
    this.status.set({ kind: 'starting' });

    const settings = vscode.workspace.getConfiguration('chiploom');
    const roots = (vscode.workspace.workspaceFolders ?? [])
      .filter((folder) => folder.uri.scheme === 'file')
      .map((folder) => folder.uri.fsPath);

    try {
      const candidate = locateCore({
        configuredPath: settings.get<string>('corePath'),
        workspaceRoots: roots,
      });
      this.log.info(`using core at ${candidate.path} (found via ${candidate.origin})`);

      const client = await CoreClient.start({
        executable: candidate.path,
        cwd: roots[0],
        workspaceRoots: roots,
        clientName: 'chiploom-vscode',
        clientVersion: this.extensionVersion,
        startupTimeoutMs: settings.get<number>('startupTimeoutMs') ?? 15_000,
        // The core writes logs to stderr, so raising the level here cannot
        // corrupt the protocol channel.
        env: { CHIPLOOM_LOG_LEVEL: settings.get<string>('logLevel') ?? 'info' },
        onLog: (line) => this.log.core(line),
        onNotification: (method, params) => this.onNotification(method, params),
        onExit: (code, signal) => {
          this.client = undefined;
          this.status.set({ kind: 'stopped' });
          this.log.info(`core exited (${signal ? `signal ${signal}` : `code ${code}`})`);
        },
      });

      this.client = client;
      const { serverVersion, sessionId, protocolVersion } = client.session;
      this.log.info(
        `connected to core ${serverVersion}, protocol v${protocolVersion}, session ${sessionId}`,
      );
      this.status.set({ kind: 'connected', version: serverVersion, sessionId });
      return client;
    } catch (error) {
      const message = describe(error);
      this.log.error(message);
      this.status.set({ kind: 'failed', message: firstLine(message) });
      // A missing binary is a setup problem worth interrupting for; anything
      // else is already in the output channel with a link to it.
      if (error instanceof CoreNotFoundError) {
        void vscode.window
          .showErrorMessage('Chip Loom: the core executable was not found.', 'Show Details')
          .then((choice) => {
            if (choice) {
              this.log.show();
            }
          });
      }
      return undefined;
    }
  }

  /** Stops the core and starts it again. */
  async restart(): Promise<CoreClient | undefined> {
    await this.stop();
    return this.start();
  }

  /** Stops the core if it is running. */
  async stop(): Promise<void> {
    const client = this.client;
    this.client = undefined;
    if (client) {
      await client.dispose();
    }
    this.status.set({ kind: 'stopped' });
  }

  /** Fetches build provenance from the core. */
  async version(): Promise<BuildInfo> {
    return (await this.call('core/version')) as unknown as BuildInfo;
  }

  /** Runs the core's diagnostics. */
  async doctor(params: DoctorParams = {}): Promise<DoctorReport> {
    return (await this.call('core/doctor', params as unknown as JsonValue)) as unknown as DoctorReport;
  }

  /** Fetches the effective configuration and its provenance. */
  async config(): Promise<ConfigReport> {
    return (await this.call('core/config')) as unknown as ConfigReport;
  }

  /**
   * Sends a request, starting the core first if needed.
   *
   * @throws when the core cannot be started, does not serve the method, or fails.
   */
  private async call(method: string, params?: JsonValue): Promise<JsonValue> {
    const client = await this.start();
    if (!client) {
      throw new Error('The Chip Loom core is not running.');
    }
    if (!client.supports(method)) {
      // Capability-driven rather than version-driven, so an older core degrades
      // with an explanation instead of a confusing protocol error.
      throw new Error(
        `This Chip Loom core (${client.session.serverVersion}) does not provide \`${method}\`.`,
      );
    }
    return client.request(method, params);
  }

  private onNotification(method: string, params: JsonValue | undefined): void {
    if (method === 'core/ready') {
      const ready = params as unknown as ReadyNotification | undefined;
      this.log.info(`core ready (session ${ready?.sessionId ?? 'unknown'})`);
      return;
    }
    this.log.info(`notification ${method}: ${JSON.stringify(params)}`);
  }

  /** Stops the core. Called when the extension is deactivated. */
  dispose(): void {
    void this.stop();
  }
}

/** Turns anything thrown into a message worth showing. */
export function describe(error: unknown): string {
  if (error instanceof CoreRpcError) {
    return error.displayMessage;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

function firstLine(message: string): string {
  return message.split('\n')[0] ?? message;
}
