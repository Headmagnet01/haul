// Bundles the extension twice from the same source:
//   dist/          Chrome and Edge (the worker as an ES module, every other script as an IIFE)
//   dist-firefox/  Firefox (the same scripts, with the manifest from manifest.mjs)
import { build } from 'esbuild';
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { firefoxManifest } from './manifest.mjs';

const pages = ['content', 'popup', 'options', 'picker-page'].map((n) => `src/${n}.ts`).filter(existsSync);

async function bundle(out, target) {
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out);
  cpSync('static', out, { recursive: true });
  const common = { bundle: true, target, outdir: out, logLevel: 'info' };
  if (existsSync('src/background.ts')) await build({ ...common, entryPoints: ['src/background.ts'], format: 'esm' });
  if (pages.length) await build({ ...common, entryPoints: pages, format: 'iife' });
}

await bundle('dist', 'chrome116');
await bundle('dist-firefox', 'firefox128');
const chromium = JSON.parse(readFileSync('static/manifest.json', 'utf8'));
writeFileSync('dist-firefox/manifest.json', JSON.stringify(firefoxManifest(chromium), null, 2) + '\n');

// The signed add-on goes here (npm run sign:firefox). The folder always exists, so builds that
// bundle it work before anything is signed.
mkdirSync('firefox', { recursive: true });
if (!existsSync('firefox/README.txt')) {
  writeFileSync(
    'firefox/README.txt',
    'Drag haul-firefox.xpi onto Firefox, choose Add, then click the Haul button and Pair with Haul.\r\n' +
      'No haul-firefox.xpi here? Sign it with npm run sign:firefox in the extension folder.\r\n',
  );
}
