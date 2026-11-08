// Run by Tauri before it bundles the installer: the signed Firefox add-on must be there, match
// the extension's version, and have been signed from the code that is built now.
// `node check-release.mjs` exits 1 with the reason otherwise.
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

export const XPI = 'haul-firefox.xpi';
export const VERSION_FILE = 'haul-firefox.version';
/** web-ext's scratch folder inside `firefox/`; the installer would ship it, so it must be gone. */
export const SCRATCH = '.signed';

/** Every file under `dir`, as paths relative to it with `/` separators, sorted. */
function filesUnder(dir, root = dir) {
  return readdirSync(dir, { withFileTypes: true })
    .flatMap((e) => (e.isDirectory() ? filesUnder(join(dir, e.name), root) : [relative(root, join(dir, e.name)).split(sep).join('/')]))
    .sort();
}

/**
 * SHA-256 (hex) of a built extension folder: each file's relative path and bytes, in path order.
 * @param {string} dir
 * @returns {string}
 */
export function distHash(dir) {
  const h = createHash('sha256');
  for (const f of filesUnder(dir)) {
    h.update(f);
    h.update('\0');
    h.update(readFileSync(join(dir, ...f.split('/'))));
    h.update('\0');
  }
  return h.digest('hex');
}

/**
 * Null when `dir` holds a signed add-on of `version` signed from the code in `distDir`, else what is wrong.
 * @param {string} dir
 * @param {string} version
 * @param {string} distDir
 * @returns {string | null}
 */
export function checkRelease(dir, version, distDir) {
  const sign = 'Run npm run sign:firefox in the extension folder.';
  if (existsSync(join(dir, SCRATCH))) {
    return `extension/firefox/${SCRATCH} is left over from a failed sign. Delete it, then sign again if needed.`;
  }
  if (!existsSync(join(dir, XPI))) return `The signed Firefox add-on (${XPI}) is missing. ${sign}`;
  const vfile = join(dir, VERSION_FILE);
  if (!existsSync(vfile)) return `${XPI} has no ${VERSION_FILE} beside it, so its version is unknown. ${sign}`;
  const [signed = '', hash = ''] = readFileSync(vfile, 'utf8').split(/\r?\n/).map((l) => l.trim());
  if (signed !== version) return `${XPI} is version ${signed}, but the extension is version ${version}. ${sign}`;
  if (!hash) return `${VERSION_FILE} doesn't say which code was signed. Raise the version and run npm run sign:firefox.`;
  if (hash !== distHash(distDir)) {
    return 'The Firefox code changed since it was signed. Raise the version and run npm run sign:firefox.';
  }
  return null;
}

const here = dirname(fileURLToPath(import.meta.url));
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const { version } = JSON.parse(readFileSync(join(here, 'static', 'manifest.json'), 'utf8'));
  const problem = checkRelease(join(here, 'firefox'), version, join(here, 'dist-firefox'));
  if (problem) {
    console.error(problem);
    process.exit(1);
  }
  console.log(`Firefox add-on ${version} is signed and ready to bundle.`);
}
