import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, test } from 'vitest';
import { checkRelease } from './check-release.mjs';

const dir = () => mkdtempSync(join(tmpdir(), 'haul-ff-'));

test('missing_xpi_fails', () => {
  const msg = checkRelease(dir(), '0.1.0');
  expect(msg).toContain('haul-firefox.xpi');
  expect(msg).toContain('npm run sign:firefox');
});

test('stale_xpi_fails', () => {
  const d = dir();
  writeFileSync(join(d, 'haul-firefox.xpi'), 'zip');
  writeFileSync(join(d, 'haul-firefox.version'), '0.0.9\n');
  const msg = checkRelease(d, '0.1.0');
  expect(msg).toContain('0.0.9');
  expect(msg).toContain('0.1.0');
});

test('xpi_without_version_fails', () => {
  const d = dir();
  writeFileSync(join(d, 'haul-firefox.xpi'), 'zip');
  expect(checkRelease(d, '0.1.0')).toContain('npm run sign:firefox');
});

test('matching_xpi_passes', () => {
  const d = dir();
  writeFileSync(join(d, 'haul-firefox.xpi'), 'zip');
  writeFileSync(join(d, 'haul-firefox.version'), '0.1.0\n');
  expect(checkRelease(d, '0.1.0')).toBeNull();
});
