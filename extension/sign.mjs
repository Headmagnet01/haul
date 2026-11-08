// Builds the extension and has Mozilla sign the Firefox build as an unlisted add-on.
// Needs the AMO API key in the environment: WEB_EXT_API_KEY (JWT issuer) and WEB_EXT_API_SECRET.
// The result is firefox/haul-firefox.xpi, with its version in firefox/haul-firefox.version.
//
// Every child runs as `node <script>` with no shell, so paths with spaces (this repository's)
// stay one argument each, and the key travels only in the environment web-ext reads itself.
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { VERSION_FILE, XPI } from './check-release.mjs';

/** @param {string} here @returns {[string, string[]]} */
export function buildCommand(here) {
  return [process.execPath, [join(here, 'build.mjs')]];
}

/** @param {string} here @param {string} out @returns {[string, string[]]} */
export function signCommand(here, out) {
  const webExt = join(here, 'node_modules', 'web-ext', 'bin', 'web-ext.js');
  return [process.execPath, [webExt, 'sign', '--channel', 'unlisted', '--source-dir', join(here, 'dist-firefox'), '--artifacts-dir', out]];
}

function main() {
  const here = dirname(fileURLToPath(import.meta.url));
  const key = process.env.WEB_EXT_API_KEY;
  const secret = process.env.WEB_EXT_API_SECRET;
  if (!key || !secret) {
    console.error(
      'WEB_EXT_API_KEY and WEB_EXT_API_SECRET must be set to your addons.mozilla.org API key ' +
        '(Developer Hub > Manage API Keys: the JWT issuer and the JWT secret).',
    );
    process.exit(1);
  }

  const [node, buildArgs] = buildCommand(here);
  const built = spawnSync(node, buildArgs, { cwd: here, stdio: 'inherit' });
  if (built.status !== 0) process.exit(built.status ?? 1);

  const { version } = JSON.parse(readFileSync(join(here, 'static', 'manifest.json'), 'utf8'));
  const out = join(here, 'firefox', '.signed');
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });

  const [cmd, args] = signCommand(here, out);
  const signed = spawnSync(cmd, args, { cwd: here, encoding: 'utf8', env: { ...process.env, WEB_EXT_API_KEY: key, WEB_EXT_API_SECRET: secret } });
  process.stdout.write(signed.stdout ?? '');
  process.stderr.write(signed.stderr ?? '');
  if (signed.status !== 0) {
    if (/version.*(already exists|has already been used|already been submitted)/i.test(`${signed.stdout}${signed.stderr}`)) {
      console.error(`Mozilla already has version ${version}. Raise "version" in extension/static/manifest.json and sign again.`);
    }
    process.exit(signed.status ?? 1);
  }

  const xpi = readdirSync(out).find((f) => f.endsWith('.xpi'));
  if (!xpi) {
    console.error(`web-ext finished but left no .xpi in ${out}.`);
    process.exit(1);
  }
  renameSync(join(out, xpi), join(here, 'firefox', XPI));
  writeFileSync(join(here, 'firefox', VERSION_FILE), `${version}\n`);
  rmSync(out, { recursive: true, force: true });
  console.log(`Signed: extension/firefox/${XPI} (version ${version}).`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main();
