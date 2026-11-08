// Builds the extension and has Mozilla sign the Firefox build as an unlisted add-on.
// Needs the AMO API key in the environment: WEB_EXT_API_KEY (JWT issuer) and WEB_EXT_API_SECRET.
// The result is firefox/haul-firefox.xpi, with its version and a hash of the signed code in
// firefox/haul-firefox.version (check-release compares that hash with the code built for a release).
//
// Every child runs as `node <script>` with no shell, so paths with spaces (this repository's)
// stay one argument each, and the key travels only in the environment web-ext reads itself.
import { spawn } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { SCRATCH, VERSION_FILE, XPI, distHash } from './check-release.mjs';

export const WAITING = 'Uploaded to Mozilla; waiting for review (this can take a few minutes)…';

/** @param {string} here @returns {[string, string[]]} */
export function buildCommand(here) {
  return [process.execPath, [join(here, 'build.mjs')]];
}

/** @param {string} here @param {string} out @returns {[string, string[]]} */
export function signCommand(here, out) {
  const webExt = join(here, 'node_modules', 'web-ext', 'bin', 'web-ext.js');
  return [process.execPath, [webExt, 'sign', '--channel', 'unlisted', '--source-dir', join(here, 'dist-firefox'), '--artifacts-dir', out]];
}

/**
 * Runs a child with no shell, passing its output through as it comes and keeping a copy.
 * @param {string} cmd @param {string[]} args @param {{ cwd: string, env: NodeJS.ProcessEnv }} opts
 * @returns {Promise<{ status: number, output: string }>}
 */
export function runChild(cmd, args, opts) {
  return new Promise((resolve) => {
    const child = spawn(cmd, args, { cwd: opts.cwd, env: opts.env, stdio: ['ignore', 'pipe', 'pipe'] });
    let output = '';
    child.stdout.on('data', (b) => {
      output += b;
      process.stdout.write(b);
    });
    child.stderr.on('data', (b) => {
      output += b;
      process.stderr.write(b);
    });
    child.on('error', (e) => resolve({ status: 1, output: `${output}${e}` }));
    child.on('close', (code) => resolve({ status: code ?? 1, output }));
  });
}

/**
 * The whole sign, with the child runner and output injected (tests fake them). Returns the exit code.
 * The scratch folder is removed on every path, so a failed sign leaves nothing for the installer.
 * @param {{ here: string, env: Record<string, string | undefined>, run?: typeof runChild,
 *   log?: (t: string) => void, err?: (t: string) => void }} o
 * @returns {Promise<number>}
 */
export async function signFlow({ here, env, run = runChild, log = console.log, err = console.error }) {
  const key = env.WEB_EXT_API_KEY;
  const secret = env.WEB_EXT_API_SECRET;
  if (!key || !secret) {
    err(
      'WEB_EXT_API_KEY and WEB_EXT_API_SECRET must be set to your addons.mozilla.org API key ' +
        '(Developer Hub > Manage API Keys: the JWT issuer and the JWT secret).',
    );
    return 1;
  }

  const [node, buildArgs] = buildCommand(here);
  const built = await run(node, buildArgs, { cwd: here, env: { ...process.env } });
  if (built.status !== 0) return built.status || 1;

  const { version } = JSON.parse(readFileSync(join(here, 'static', 'manifest.json'), 'utf8'));
  const out = join(here, 'firefox', SCRATCH);
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  try {
    const [cmd, args] = signCommand(here, out);
    log(WAITING);
    const signed = await run(cmd, args, { cwd: here, env: { ...process.env, WEB_EXT_API_KEY: key, WEB_EXT_API_SECRET: secret } });
    if (signed.status !== 0) {
      if (/version.*(already exists|has already been used|already been submitted)/i.test(signed.output)) {
        err(`Mozilla already has version ${version}. Raise "version" in extension/static/manifest.json and sign again.`);
      }
      return signed.status || 1;
    }
    const xpi = readdirSync(out).find((f) => f.endsWith('.xpi'));
    if (!xpi) {
      err(`web-ext finished but left no .xpi in ${out}.`);
      return 1;
    }
    renameSync(join(out, xpi), join(here, 'firefox', XPI));
    writeFileSync(join(here, 'firefox', VERSION_FILE), `${version}\n${distHash(join(here, 'dist-firefox'))}\n`);
    log(`Signed: extension/firefox/${XPI} (version ${version}).`);
    return 0;
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const here = dirname(fileURLToPath(import.meta.url));
  signFlow({ here, env: process.env }).then((code) => process.exit(code));
}
