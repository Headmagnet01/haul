import { expect, test } from 'vitest';
import { buildCommand, signCommand } from './sign.mjs';

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
