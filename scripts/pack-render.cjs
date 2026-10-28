#!/usr/bin/env node
'use strict';
// pack-render.cjs <package> [<package> ...]: pack a lazy engine's render set into
// `render.bin` in each package directory and remove the set's single files, so a client
// fetches one file before its first frame (docs/app-modules.md § The render bundle).
//
// The render set is what `lazyManifest()` marks `set: "render"`: the `render` code module,
// DejaVu, the platform's UI family, its icons and wallpaper, and the symbol masks. The
// first package's Node engine (`computerworld.js`) gives the manifest; every package
// receives the same bundle. The format is the module hosts' (`modules.cjs` `pack`).
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { pack } = require('../crates/bindings/wasm/js/modules.cjs');

const packages = process.argv.slice(2);
if (!packages.length) throw new Error('usage: pack-render.cjs <package> [<package> ...]');
const node = packages.find((dir) => fs.existsSync(path.join(dir, 'computerworld.js')) && !fs.readFileSync(path.join(dir, 'computerworld.js'), 'utf8').includes('import.meta'));
if (!node) throw new Error('no Node package (a CommonJS computerworld.js) among ' + packages.join(', '));
const manifest = require(path.resolve(node, 'computerworld.js')).lazyManifest();
const set = [...manifest.modules, ...manifest.fonts, ...(manifest.assets ?? [])].filter((entry) => entry.set === 'render');
if (!set.some((entry) => entry.kind === 'code')) throw new Error('the manifest marks no render code module: is this a lazy engine split into code modules?');
const files = set.map((entry) => {
    const bytes = fs.readFileSync(path.join(node, entry.path));
    const got = crypto.createHash('sha256').update(bytes).digest('hex');
    if (got !== entry.sha256) throw new Error(`${entry.path} is ${got}; the engine pins ${entry.sha256}`);
    return { path: entry.path, sha256: entry.sha256, bytes };
});
const bundle = pack(files);
for (const dir of packages) {
    fs.writeFileSync(path.join(dir, 'render.bin'), bundle);
    for (const file of files) fs.rmSync(path.join(dir, file.path), { force: true });
}
console.log(`render.bin ${String(bundle.length).padStart(12)} raw, ${files.length} files`);
