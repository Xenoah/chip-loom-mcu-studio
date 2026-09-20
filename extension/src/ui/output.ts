/**
 * The Chip Loom output channel.
 *
 * Every line the core writes to stderr lands here verbatim. That is the whole
 * point of keeping diagnostics off stdout: a user can read exactly what the core
 * said, in order, without the protocol getting in the way.
 */

import * as vscode from 'vscode';

import type { DoctorReport } from '../core/protocol';

/** Wraps the extension's single output channel. */
export class OutputLog {
  private readonly channel: vscode.OutputChannel;

  constructor() {
    this.channel = vscode.window.createOutputChannel('Chip Loom');
  }

  /** Writes a line the extension itself produced. */
  info(message: string): void {
    this.channel.appendLine(`[extension] ${message}`);
  }

  /** Writes a line the core produced, unmodified. */
  core(line: string): void {
    this.channel.appendLine(line);
  }

  /** Writes a failure, with the core's own hint when it supplied one. */
  error(message: string): void {
    for (const line of message.split('\n')) {
      this.channel.appendLine(`[error] ${line}`);
    }
  }

  /** Renders a diagnostics report as the CLI would, and reveals the channel. */
  doctorReport(report: DoctorReport): void {
    const width = Math.max(...report.checks.map((check) => check.title.length));
    this.channel.appendLine('');
    this.channel.appendLine(`Chip Loom diagnostics (core ${report.build.version})`);
    this.channel.appendLine('');
    for (const check of report.checks) {
      const marker = { ok: 'ok  ', warn: 'warn', error: 'fail', skipped: 'skip' }[check.status];
      this.channel.appendLine(`  ${marker} ${check.title.padEnd(width)}  ${check.detail}`);
      if (check.hint) {
        this.channel.appendLine(`       ${''.padEnd(width)}  ${check.hint}`);
      }
    }
    const failures = report.checks.filter((check) => check.status === 'error').length;
    const warnings = report.checks.filter((check) => check.status === 'warn').length;
    this.channel.appendLine('');
    this.channel.appendLine(
      `  ${report.checks.length} checks, ${failures} failure(s), ${warnings} warning(s), ` +
        `${report.durationMs} ms`,
    );
    this.show();
  }

  /** Brings the channel into view without stealing focus from the editor. */
  show(): void {
    this.channel.show(true);
  }

  /** Releases the channel. */
  dispose(): void {
    this.channel.dispose();
  }
}
