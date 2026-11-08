// Bundles the extension into dist/: the worker as an ES module, every other script as an IIFE.
import { build } from 'esbuild';
import { cpSync, existsSync, mkdirSync, rmSync } from 'node:fs';

const out = 'dist';
rmSync(out, { recursive: true, force: true });
mkdirSync(out);
cpSync('static', out, { recursive: true });

const common = { bundle: true, target: 'chrome116', outdir: out, logLevel: 'info' };
const pages = ['content', 'popup', 'options', 'picker-page'].map((n) => `src/${n}.ts`).filter(existsSync);
if (existsSync('src/background.ts')) await build({ ...common, entryPoints: ['src/background.ts'], format: 'esm' });
if (pages.length) await build({ ...common, entryPoints: pages, format: 'iife' });
