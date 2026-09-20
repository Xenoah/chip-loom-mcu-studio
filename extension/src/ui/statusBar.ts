/**
 * The Chip Loom status bar item.
 *
 * It answers one question at a glance: is the core connected, and which version?
 * Anything longer belongs in the output channel.
 */

import * as vscode from 'vscode';

/** What the status bar is currently showing. */
export type CoreState =
  | { kind: 'starting' }
  | { kind: 'connected'; version: string; sessionId: string }
  | { kind: 'stopped' }
  | { kind: 'failed'; message: string };

/** Owns the status bar item and keeps it in step with the session. */
export class StatusBar {
  private readonly item: vscode.StatusBarItem;

  constructor() {
    this.item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
    this.item.name = 'Chip Loom';
    this.item.command = 'chiploom.showOutput';
    this.set({ kind: 'stopped' });
    this.item.show();
  }

  /** Reflects a new session state. */
  set(state: CoreState): void {
    switch (state.kind) {
      case 'starting':
        this.item.text = '$(sync~spin) Chip Loom';
        this.item.tooltip = 'Starting the Chip Loom core...';
        this.item.backgroundColor = undefined;
        break;
      case 'connected':
        this.item.text = `$(circuit-board) Chip Loom ${state.version}`;
        this.item.tooltip = new vscode.MarkdownString(
          `**Chip Loom core connected**\n\n` +
            `Version: \`${state.version}\`\n\n` +
            `Session: \`${state.sessionId}\`\n\n` +
            `Click to open the output channel.`,
        );
        this.item.backgroundColor = undefined;
        break;
      case 'stopped':
        this.item.text = '$(debug-disconnect) Chip Loom';
        this.item.tooltip = 'The Chip Loom core is not running. Run "Chip Loom: Restart Core".';
        this.item.backgroundColor = undefined;
        break;
      case 'failed':
        this.item.text = '$(error) Chip Loom';
        this.item.tooltip = new vscode.MarkdownString(
          `**The Chip Loom core could not start**\n\n${state.message}`,
        );
        this.item.backgroundColor = new vscode.ThemeColor('statusBarItem.errorBackground');
        break;
    }
  }

  /** Releases the item. */
  dispose(): void {
    this.item.dispose();
  }
}
