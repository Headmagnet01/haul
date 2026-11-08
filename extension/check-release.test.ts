import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, test } from 'vitest';
import { checkRelease, distHash } from './check-release.mjs';

const dir = () => mkdtempSync(join(tmpdir(), 'haul-ff-'));

/** A built Firefox copy with one file. */
function dist(text = 'console.log(1)'): string {
  const d = dir();
  writeFileSync(join(d, 'background.js'), text);
  return d;
}

/** A signed add-on folder for `version`, signed from `distDir`. */
function signed(distDir: string, version = '0.1.0'): string {
  const d = dir();
  writeFileSync(join(d, 'haul-firefox.xpi'), 'zip');
  writeFileSync(join(d, 'haul-firefox.version'), `${version}\n${distHash(distDir)}\n`);
  return d;
}

test('missing_xpi_fails', () => {
  const msg = checkRelease(dir(), '0.1.0', dist());
  expect(msg).toContain('haul-firefox.xpi');
  expect(msg).toContain('npm run sign:firefox');
});

test('stale_xpi_fails', () => {
  const code = dist();
  const msg = checkRelease(signed(code, '0.0.9'), '0.1.0', code);
  expect(msg).toContain('0.0.9');
  expect(msg).toContain('0.1.0');
});

test('xpi_without_version_fails', () => {
  const d = dir();
  writeFileSync(join(d, 'haul-firefox.xpi'), 'zip');
  expect(checkRelease(d, '0.1.0', dist())).toContain('npm run sign:firefox');
});

test('matching_xpi_passes', () => {
  const code = dist();
  expect(checkRelease(signed(code), '0.1.0', code)).toBeNull();
});

test('refuses_changed_code', () => {
  const code = dist();
  const d = signed(code);
  writeFileSync(join(code, 'background.js'), 'console.log(2)');
  expect(checkRelease(d, '0.1.0', code)).toBe(
    'The Firefox code changed since it was signed. Raise the version and run npm run sign:firefox.',
  );
});

test('refuses_version_without_hash', () => {
  const d = dir();
  writeFileSync(join(d, 'haul-firefox.xpi'), 'zip');
  writeFileSync(join(d, 'haul-firefox.version'), '0.1.0\n');
  expect(checkRelease(d, '0.1.0', dist())).toContain('npm run sign:firefox');
});

test('refuses_while_signed_scratch_exists', () => {
  const code = dist();
  const d = signed(code);
  mkdirSync(join(d, '.signed'));
  expect(checkRelease(d, '0.1.0', code)).toContain('.signed');
});

test('distHash_is_stable', () => {
  expect(distHash(dist('a'))).toBe(distHash(dist('a')));
  expect(distHash(dist('a'))).not.toBe(distHash(dist('b')));
  const nested = dist('a');
  mkdirSync(join(nested, 'sub'));
  writeFileSync(join(nested, 'sub', 'x.js'), 'x');
  expect(distHash(nested)).not.toBe(distHash(dist('a')));
});
