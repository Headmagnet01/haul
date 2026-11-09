import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, test } from 'vitest';
import { distHash } from './check-release.mjs';
import { WAITING, buildCommand, signCommand, signFlow } from './sign.mjs';

const HERE = 'C:\\tools\\claude for project\\extension';

test('sign_command_keeps_paths_with_spaces_whole', () => {
  const out = `${HERE}\\firefox\\.signed`;
  const [cmd, args] = signCommand(HERE, out);
  expect(cmd).toBe(process.execPath);
  expect(args[0]).toBe(`${HERE}\\node_modules\\web-ext\\bin\\web-ext.js`);
  expect(args.slice(1)).toEqual(['sign', '--channel', 'unlisted', '--source-dir', `${HERE}\\dist-firefox`, '--artifacts-dir', out]);
});

test('sign_command_never_carries_the_key', () => {
  const [, args] = signCommand(HERE, 'x');
  expect(args.join(' ')).not.toMatch(/api-key|api-secret/);
});

test('build_command_runs_node_directly', () => {
  expect(buildCommand(HERE)).toEqual([process.execPath, [`${HERE}\\build.mjs`]]);
});

/** An extension folder with a manifest and a built Firefox copy. */
function here(): string {
  const d = mkdtempSync(join(tmpdir(), 'haul-sign-'));
  mkdirSync(join(d, 'static'));
  writeFileSync(join(d, 'static', 'manifest.json'), JSON.stringify({ version: '0.2.0' }));
  mkdirSync(join(d, 'dist-firefox'));
  writeFileSync(join(d, 'dist-firefox', 'background.js'), 'console.log(1)');
  return d;
}

const env = { WEB_EXT_API_KEY: 'k', WEB_EXT_API_SECRET: 's' };
const quiet = { log: () => {}, err: () => {} };

test('cleans_scratch_on_failure', async () => {
  const d = here();
  const run = async (_cmd: string, args: string[]) => {
    if (args.includes('sign')) {
      writeFileSync(join(d, 'firefox', '.signed', 'half.xpi'), 'x');
      return { status: 1, output: 'Mozilla said no' };
    }
    return { status: 0, output: '' };
  };
  expect(await signFlow({ here: d, env, run, ...quiet })).toBe(1);
  expect(existsSync(join(d, 'firefox', '.signed'))).toBe(false);
});

test('prints_waiting_line_before_signing', async () => {
  const d = here();
  const said: string[] = [];
  let seenAtSign: string[] = [];
  const run = async (_cmd: string, args: string[]) => {
    if (!args.includes('sign')) return { status: 0, output: '' };
    seenAtSign = [...said];
    return { status: 1, output: '' };
  };
  await signFlow({ here: d, env, run, log: (t: string) => said.push(t), err: () => {} });
  expect(WAITING).toBe('Uploaded to Mozilla; waiting for review (this can take a few minutes)…');
  expect(seenAtSign).toContain(WAITING);
});

test('writes_version_and_hash', async () => {
  const d = here();
  const run = async (_cmd: string, args: string[]) => {
    if (args.includes('sign')) writeFileSync(join(d, 'firefox', '.signed', 'haul-0.2.0.xpi'), 'signed');
    return { status: 0, output: '' };
  };
  expect(await signFlow({ here: d, env, run, ...quiet })).toBe(0);
  expect(readFileSync(join(d, 'firefox', 'haul-firefox.xpi'), 'utf8')).toBe('signed');
  expect(readFileSync(join(d, 'firefox', 'haul-firefox.version'), 'utf8')).toBe(`0.2.0\n${distHash(join(d, 'dist-firefox'))}\n`);
  expect(existsSync(join(d, 'firefox', '.signed'))).toBe(false);
});

test('hash_ignores_files_web_ext_leaves_in_the_build', async () => {
  const d = here();
  const before = distHash(join(d, 'dist-firefox'));
  const run = async (_cmd: string, args: string[]) => {
    if (args.includes('sign')) {
      // web-ext saves these in --source-dir; the next build wipes them.
      writeFileSync(join(d, 'dist-firefox', '.amo-upload-uuid'), 'abc');
      writeFileSync(join(d, 'dist-firefox', '.web-extension-id'), 'haul@x');
      writeFileSync(join(d, 'firefox', '.signed', 'haul-0.2.0.xpi'), 'signed');
    }
    return { status: 0, output: '' };
  };
  expect(await signFlow({ here: d, env, run, ...quiet })).toBe(0);
  expect(readFileSync(join(d, 'firefox', 'haul-firefox.version'), 'utf8')).toBe(`0.2.0\n${before}\n`);
});

test('missing_key_stops_before_building', async () => {
  const d = here();
  let ran = false;
  const run = async () => {
    ran = true;
    return { status: 0, output: '' };
  };
  expect(await signFlow({ here: d, env: {}, run, ...quiet })).toBe(1);
  expect(ran).toBe(false);
});
