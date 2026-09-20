/**
 * Verifies that the extension's client and the Rust core actually talk to each
 * other.
 *
 * These tests spawn the real `chiploom` binary and drive the real
 * {@link CoreClient}. Nothing is mocked and the `vscode` module is never
 * imported, which is why they can run in CI on every platform without an editor.
 *
 * Build the binary first: `cargo build` (or `cargo build --release`). Set
 * `CHIPLOOM_BIN` to test a specific executable.
 */

import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { after, before, describe, it } from 'node:test';

import { CoreClient } from '../src/core/client';
import { candidates, EXECUTABLE, CoreNotFoundError, locateCore } from '../src/core/locate';
import {
  type BuildInfo,
  type ConfigReport,
  CoreRpcError,
  type DoctorReport,
  ErrorCodes,
  PROTOCOL_VERSION,
} from '../src/core/protocol';

const repoRoot = resolve(__dirname, '..', '..', '..');

/** Finds the binary these tests should drive, or explains how to build one. */
function coreExecutable(): string {
  const fromEnv = process.env.CHIPLOOM_BIN?.trim();
  if (fromEnv) {
    return fromEnv;
  }
  for (const profile of ['release', 'debug']) {
    const path = join(repoRoot, 'target', profile, EXECUTABLE);
    if (existsSync(path)) {
      return path;
    }
  }
  throw new Error(
    `No ${EXECUTABLE} found under ${join(repoRoot, 'target')}. ` +
      'Run `cargo build` first, or set CHIPLOOM_BIN.',
  );
}

describe('locating the core', () => {
  it('prefers the configured path, then the environment, then the workspace, then PATH', () => {
    const found = candidates({
      configuredPath: '/opt/chiploom/bin/chiploom',
      workspaceRoots: ['/work/firmware'],
      env: { CHIPLOOM_BIN: '/env/chiploom', PATH: `/usr/bin` },
    });

    assert.deepEqual(
      found.map((candidate) => candidate.origin),
      ['setting', 'environment', 'workspace-release', 'workspace-debug', 'path'],
    );
    assert.equal(found[0]?.path, '/opt/chiploom/bin/chiploom');
    assert.ok(found[2]?.path.includes(join('target', 'release')));
  });

  it('ignores a blank corePath setting rather than searching for an empty name', () => {
    const found = candidates({ configuredPath: '   ', env: { PATH: '/usr/bin' } });
    assert.equal(found.length, 1);
    assert.equal(found[0]?.origin, 'path');
  });

  it('returns the first executable candidate', () => {
    const hit = locateCore({
      configuredPath: '/not/there/chiploom',
      env: { PATH: '/usr/bin' },
      isExecutable: (path) => path === join('/usr/bin', EXECUTABLE),
    });
    assert.equal(hit.origin, 'path');
  });

  it('lists everywhere it looked when nothing is executable', () => {
    assert.throws(
      () => locateCore({ env: { PATH: '/usr/bin' }, isExecutable: () => false }),
      (error: unknown) => {
        if (!(error instanceof CoreNotFoundError)) {
          throw error;
        }
        // A "not found" a user can act on has to say where it looked.
        assert.match(error.message, /\/usr\/bin/);
        assert.match(error.message, /chiploom\.corePath/);
        return true;
      },
    );
  });

  it('finds the binary this checkout just built', () => {
    const hit = locateCore({ workspaceRoots: [repoRoot], env: { PATH: '' } });
    assert.ok(hit.origin === 'workspace-debug' || hit.origin === 'workspace-release');
  });
});

describe('talking to the real core', () => {
  let client: CoreClient;
  const logLines: string[] = [];
  const notifications: Array<{ method: string; params: unknown }> = [];

  before(async () => {
    client = await CoreClient.start({
      executable: coreExecutable(),
      cwd: repoRoot,
      workspaceRoots: [repoRoot],
      clientName: 'chiploom-extension-tests',
      clientVersion: '0.1.0-pre.0',
      startupTimeoutMs: 30_000,
      env: { CHIPLOOM_LOG_LEVEL: 'debug' },
      onLog: (line) => logLines.push(line),
      onNotification: (method, params) => notifications.push({ method, params }),
    });
  });

  after(async () => {
    await client?.dispose();
  });

  it('completes the handshake on the agreed protocol version', () => {
    const session = client.session;
    assert.equal(session.serverName, 'chiploom-core');
    assert.equal(session.protocolVersion, PROTOCOL_VERSION);
    assert.ok(session.serverVersion.length > 0, 'the core must report its version');
    assert.ok(session.sessionId.length > 0, 'the core must report a session id');
    assert.ok(session.pid > 0, 'the core must report its pid');
    assert.ok(client.running);
  });

  it('advertises the methods this extension calls', () => {
    for (const method of ['core/version', 'core/doctor', 'core/config', 'shutdown']) {
      assert.ok(client.supports(method), `core does not advertise ${method}`);
    }
    assert.equal(client.supports('core/build'), false, 'unimplemented methods must not be claimed');
  });

  it('answers core/version with this build', async () => {
    const info = (await client.request('core/version')) as unknown as BuildInfo;
    assert.equal(info.version, client.session.serverVersion);
    assert.equal(info.protocolVersion, PROTOCOL_VERSION);
    assert.ok(info.target.length > 0);
    assert.match(info.builtAt, /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/);
  });

  it('answers core/doctor with a full report', async () => {
    const report = (await client.request('core/doctor', {
      online: false,
      writeProbe: true,
    })) as unknown as DoctorReport;

    assert.ok(report.checks.length >= 10, `expected a full report, got ${report.checks.length}`);
    assert.ok(report.build.version.length > 0);
    assert.equal(typeof report.durationMs, 'number');

    for (const check of report.checks) {
      assert.ok(check.id.length > 0);
      assert.ok(['ok', 'warn', 'error', 'skipped'].includes(check.status), check.status);
      assert.ok(check.detail.length > 0, `${check.id} reported nothing`);
    }
    // The network check must stay skipped unless it was asked for, so this test
    // never depends on the machine having internet access.
    const network = report.checks.find((check) => check.id === 'network.reachability');
    assert.equal(network?.status, 'skipped');
  });

  it('answers core/config with effective values and their provenance', async () => {
    const report = (await client.request('core/config')) as unknown as ConfigReport;
    assert.ok(report.paths.dataDir.length > 0);
    assert.ok(report.sources.length >= 5, 'every layer must be reported');
    assert.ok(['error', 'warn', 'info', 'debug', 'trace'].includes(report.config.log.level));
  });

  it('reports an unknown method as an error without dropping the session', async () => {
    await assert.rejects(
      () => client.request('core/teleport'),
      (error: unknown) => {
        if (!(error instanceof CoreRpcError)) {
          throw error;
        }
        assert.equal(error.code, ErrorCodes.methodNotFound);
        return true;
      },
    );
    // The session has to survive it, or one stray call would kill the editor's core.
    const info = (await client.request('core/version')) as unknown as BuildInfo;
    assert.ok(info.version.length > 0);
    assert.ok(client.running);
  });

  it('reports bad params as invalid params', async () => {
    await assert.rejects(
      () => client.request('core/doctor', { online: 'yes please' }),
      (error: unknown) => {
        if (!(error instanceof CoreRpcError)) {
          throw error;
        }
        assert.equal(error.code, ErrorCodes.invalidParams);
        return true;
      },
    );
  });

  it('forwards the core log to stderr, keeping stdout clean', () => {
    // If any log had gone to stdout the handshake above would have failed to
    // parse, so reaching here already proves the separation; this asserts the
    // other half, that the extension really does receive the log.
    assert.ok(logLines.length > 0, 'expected the core to log to stderr');
    assert.ok(
      logLines.some((line) => line.includes('session initialized')),
      `expected the handshake to be logged, got:\n${logLines.join('\n')}`,
    );
  });

  it('delivers the core/ready notification', () => {
    assert.ok(
      notifications.some((notification) => notification.method === 'core/ready'),
      `expected core/ready, got ${JSON.stringify(notifications)}`,
    );
  });
});

describe('protocol version negotiation', () => {
  it('refuses a client that speaks a version the core does not', async () => {
    // Drives the transport directly, since CoreClient always sends the version
    // it was built against.
    const { spawn } = await import('node:child_process');
    const child = spawn(coreExecutable(), ['serve', '--stdio'], { stdio: 'pipe' });
    const frames: string[] = [];
    child.stdout.on('data', (chunk: Buffer) => frames.push(chunk.toString('utf8')));

    child.stdin.write(
      `${JSON.stringify({
        jsonrpc: '2.0',
        id: 1,
        method: 'initialize',
        params: { clientName: 'from-the-future', protocolVersion: 9999 },
      })}\n`,
    );
    await new Promise((done) => setTimeout(done, 1_500));
    child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'exit' })}\n`);
    child.stdin.end();
    await new Promise((done) => child.once('exit', done));

    const response = JSON.parse(frames.join('').trim().split('\n')[0] ?? '{}') as {
      error?: { code: number; data?: { supported?: number } };
    };
    assert.equal(response.error?.code, ErrorCodes.unsupportedProtocolVersion);
    assert.equal(response.error?.data?.supported, PROTOCOL_VERSION);
  });
});

describe('shutting the core down', () => {
  it('stops the child process and rejects later requests', async () => {
    const client = await CoreClient.start({
      executable: coreExecutable(),
      cwd: repoRoot,
      startupTimeoutMs: 30_000,
    });
    const pid = client.session.pid;
    assert.ok(pid > 0);

    await client.dispose();
    assert.equal(client.running, false);
    await assert.rejects(() => client.request('core/version'));
  });
});
