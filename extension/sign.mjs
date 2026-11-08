// Builds the extension and has Mozilla sign the Firefox build as an unlisted add-on.
// Needs the AMO API key in the environment: WEB_EXT_API_KEY (JWT issuer) and WEB_EXT_API_SECRET.
// The result is firefox/haul-firefox.xpi, with its version in firefox/haul-firefox.version.
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { VERSION_FILE, XPI } from './check-release.mjs';

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

const run = (cmd, args, opts = {}) => spawnSync(cmd, args, { cwd: here, shell: true, encoding: 'utf8', ...opts });

const built = run('npm', ['run', 'build'], { stdio: 'inherit' });
if (built.status !== 0) process.exit(built.status ?? 1);

const { version } = JSON.parse(readFileSync(join(here, 'static', 'manifest.json'), 'utf8'));
const out = join(here, 'firefox', '.signed');
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });

// The key travels in the environment web-ext reads itself, never on a command line or in a log.
const signed = run('npx', ['web-ext', 'sign', '--channel', 'unlisted', '--source-dir', 'dist-firefox', '--artifacts-dir', out], {
  env: { ...process.env, WEB_EXT_API_KEY: key, WEB_EXT_API_SECRET: secret },
});
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
