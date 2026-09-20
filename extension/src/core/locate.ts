/**
 * Finding the `chiploom` executable.
 *
 * The search order matters: a contributor working in this repository must get
 * the binary they just built, not a stale one from `PATH`, or they will debug a
 * version of the core they are not editing.
 *
 * Deliberately free of any `vscode` import so it can be unit-tested in plain
 * Node, as `test/protocol.test.ts` does.
 */

import { accessSync, constants } from 'node:fs';
import { delimiter, join } from 'node:path';

/** The executable's name on this platform. */
export const EXECUTABLE = process.platform === 'win32' ? 'chiploom.exe' : 'chiploom';

/** Where a candidate came from, for the "not found" message and for logging. */
export type CandidateOrigin =
  | 'setting'
  | 'environment'
  | 'workspace-release'
  | 'workspace-debug'
  | 'path';

/** One place the executable might be. */
export interface Candidate {
  origin: CandidateOrigin;
  path: string;
}

/** Inputs to the search, all injected so the search is testable. */
export interface LocateOptions {
  /** Value of the `chiploom.corePath` setting; empty when unset. */
  configuredPath?: string | undefined;
  /** Open workspace folders, most relevant first. */
  workspaceRoots?: readonly string[];
  /** Environment to read `CHIPLOOM_BIN` and `PATH` from. */
  env?: NodeJS.ProcessEnv;
  /** Predicate for "this path is an executable file". Swapped in tests. */
  isExecutable?: (path: string) => boolean;
}

/** Whether `path` names a file this process can execute. */
export function isExecutableFile(path: string): boolean {
  try {
    accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

/**
 * Builds the ordered list of places to look, without touching the filesystem.
 *
 * Exposed separately so the "not found" message can list exactly what was
 * tried, which is the difference between a fixable report and a shrug.
 */
export function candidates(options: LocateOptions = {}): Candidate[] {
  const env = options.env ?? process.env;
  const roots = options.workspaceRoots ?? [];
  const found: Candidate[] = [];

  const configured = options.configuredPath?.trim();
  if (configured) {
    found.push({ origin: 'setting', path: configured });
  }

  const fromEnv = env.CHIPLOOM_BIN?.trim();
  if (fromEnv) {
    found.push({ origin: 'environment', path: fromEnv });
  }

  // A checkout of this repository: prefer release, then debug.
  for (const root of roots) {
    found.push({ origin: 'workspace-release', path: join(root, 'target', 'release', EXECUTABLE) });
    found.push({ origin: 'workspace-debug', path: join(root, 'target', 'debug', EXECUTABLE) });
  }

  for (const entry of (env.PATH ?? '').split(delimiter)) {
    if (entry.length > 0) {
      found.push({ origin: 'path', path: join(entry, EXECUTABLE) });
    }
  }

  return found;
}

/** Raised when no candidate was executable, listing what was tried. */
export class CoreNotFoundError extends Error {
  constructor(readonly tried: readonly Candidate[]) {
    const lines = tried.map((candidate) => `  ${candidate.origin}: ${candidate.path}`);
    super(
      `Could not find the \`${EXECUTABLE}\` executable.\n\n` +
        `Looked in:\n${lines.join('\n')}\n\n` +
        'Install Chip Loom, or set `chiploom.corePath` to the executable.',
    );
    this.name = 'CoreNotFoundError';
  }
}

/**
 * Returns the first candidate that is an executable file.
 *
 * @throws {CoreNotFoundError} when none of them is.
 */
export function locateCore(options: LocateOptions = {}): Candidate {
  const isExecutable = options.isExecutable ?? isExecutableFile;
  const tried = candidates(options);
  const hit = tried.find((candidate) => isExecutable(candidate.path));
  if (!hit) {
    throw new CoreNotFoundError(tried);
  }
  return hit;
}
