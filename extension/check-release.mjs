// Run by Tauri before it bundles the installer: the signed Firefox add-on must be there and
// match the extension's version. `node check-release.mjs` exits 1 with the reason otherwise.
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const XPI = 'haul-firefox.xpi';
export const VERSION_FILE = 'haul-firefox.version';

/**
 * Null when `dir` holds a signed add-on of `version`, else what is wrong.
 * @param {string} dir
 * @param {string} version
 * @returns {string | null}
 */
export function checkRelease(dir, version) {
  const sign = 'Run npm run sign:firefox in the extension folder.';
  if (!existsSync(join(dir, XPI))) return `The signed Firefox add-on (${XPI}) is missing. ${sign}`;
  const vfile = join(dir, VERSION_FILE);
  if (!existsSync(vfile)) return `${XPI} has no ${VERSION_FILE} beside it, so its version is unknown. ${sign}`;
  const signed = readFileSync(vfile, 'utf8').trim();
  if (signed !== version) return `${XPI} is version ${signed}, but the extension is version ${version}. ${sign}`;
  return null;
}

const here = dirname(fileURLToPath(import.meta.url));
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const { version } = JSON.parse(readFileSync(join(here, 'static', 'manifest.json'), 'utf8'));
  const problem = checkRelease(join(here, 'firefox'), version);
  if (problem) {
    console.error(problem);
    process.exit(1);
  }
  console.log(`Firefox add-on ${version} is signed and ready to bundle.`);
}
