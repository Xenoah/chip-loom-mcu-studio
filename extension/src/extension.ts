/**
 * Extension entry point.
 *
 * The extension contributes no behaviour of its own: every command is a call
 * into the Chip Loom core over the same protocol any other client would use, so
 * the editor can never do something `chiploom` on the command line cannot.
 */

import * as vscode from 'vscode';

import { CoreSession, describe } from './session';
import { OutputLog } from './ui/output';
import { StatusBar } from './ui/statusBar';

/** Called by VS Code when the extension activates. */
export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const log = new OutputLog();
  const status = new StatusBar();
  const version = readVersion(context);
  const session = new CoreSession(log, status, version);

  context.subscriptions.push(log, status, session);
  log.info(`Chip Loom extension ${version} activated`);

  register(context, 'chiploom.showOutput', () => {
    log.show();
  });

  register(context, 'chiploom.showVersion', async () => {
    const info = await session.version();
    const commit = info.commit === 'unknown' ? '' : ` (${info.commit}${info.dirty ? '-dirty' : ''})`;
    log.info(`core ${info.version}${commit} for ${info.target}, built ${info.builtAt}`);
    const choice = await vscode.window.showInformationMessage(
      `Chip Loom core ${info.version}${commit} — ${info.target}`,
      'Show Details',
    );
    if (choice) {
      log.show();
    }
  });

  register(context, 'chiploom.runDoctor', async () => {
    const report = await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Window, title: 'Chip Loom: running diagnostics' },
      () => session.doctor({ writeProbe: true }),
    );
    log.doctorReport(report);

    const failures = report.checks.filter((check) => check.status === 'error');
    if (failures.length > 0) {
      void vscode.window.showErrorMessage(
        `Chip Loom: ${failures.length} diagnostic${failures.length === 1 ? '' : 's'} failed. ` +
          `First: ${failures[0]?.title}.`,
      );
      return;
    }
    const warnings = report.checks.filter((check) => check.status === 'warn').length;
    void vscode.window.showInformationMessage(
      warnings > 0
        ? `Chip Loom is ready, with ${warnings} warning${warnings === 1 ? '' : 's'}.`
        : 'Chip Loom is ready.',
    );
  });

  register(context, 'chiploom.restartCore', async () => {
    log.info('restarting the core on request');
    const client = await session.restart();
    if (client) {
      void vscode.window.showInformationMessage(
        `Chip Loom core ${client.session.serverVersion} restarted.`,
      );
    }
  });

  if (vscode.workspace.getConfiguration('chiploom').get<boolean>('autoStart') ?? true) {
    // Failure is reported through the status bar and output channel, so this is
    // deliberately not awaited: activation must not block on a slow core.
    void session.start();
  }
}

/** Called by VS Code when the extension deactivates. */
export function deactivate(): void {
  // Everything is registered in `context.subscriptions`, which VS Code disposes.
}

/**
 * Registers a command, turning any failure into a message and a log entry.
 *
 * Without this wrapper a rejected promise in a command handler surfaces only as
 * "command failed" with no detail.
 */
function register(
  context: vscode.ExtensionContext,
  command: string,
  handler: () => Promise<void> | void,
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand(command, async () => {
      try {
        await handler();
      } catch (error) {
        const message = describe(error);
        void vscode.window.showErrorMessage(`Chip Loom: ${message.split('\n')[0]}`);
        throw error;
      }
    }),
  );
}

function readVersion(context: vscode.ExtensionContext): string {
  const version = (context.extension.packageJSON as { version?: unknown }).version;
  return typeof version === 'string' ? version : '0.0.0';
}
