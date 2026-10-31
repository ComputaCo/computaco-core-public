#!/usr/bin/env node
// Pack each node-app package under packages/node/<id>/ into one archive the engine
// fetches by hash (docs/app-packages.md): `data/packages/<id>.bin`, a little-endian
// u32 header length, a JSON header {id, files: [{path, offset, bytes, sha256}]} and the
// files' bytes in path order. Deterministic: the same files give the same bytes, so the
// SHA-256 pinned in packages/catalog.json (runtime.sha256) is the build's contract.
//
//   node scripts/pack-node-apps.mjs <out dir>      write the archives and pin their hashes
//   node scripts/pack-node-apps.mjs --check         fail when a pinned hash is stale
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const root = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const src = path.join(root, 'packages/node');
const catalogPath = path.join(root, 'packages/catalog.json');
const check = process.argv.includes('--check');
const outDir = process.argv.find((a) => !a.startsWith('--') && a !== process.argv[0] && a !== process.argv[1]);
if (!check && !outDir) {
    console.error('usage: pack-node-apps.mjs <out dir> | --check');
    process.exit(2);
}
const sha = (b) => createHash('sha256').update(b).digest('hex');
function walk(dir, rel = '') {
    const out = [];
    for (const name of readdirSync(dir).sort()) {
        const p = path.join(dir, name);
        const r = rel ? `${rel}/${name}` : name;
        if (statSync(p).isDirectory()) out.push(...walk(p, r));
        else out.push(r);
    }
    return out;
}
export function pack(id) {
    const dir = path.join(src, id);
    const files = [];
    const chunks = [];
    let offset = 0;
    for (const rel of walk(dir)) {
        const bytes = readFileSync(path.join(dir, rel));
        files.push({ path: rel, offset, bytes: bytes.length, sha256: sha(bytes) });
        chunks.push(bytes);
        offset += bytes.length;
    }
    const header = Buffer.from(JSON.stringify({ id, files }));
    const len = Buffer.alloc(4);
    len.writeUInt32LE(header.length, 0);
    return Buffer.concat([len, header, ...chunks]);
}
const catalog = JSON.parse(readFileSync(catalogPath, 'utf8'));
const ids = readdirSync(src).filter((d) => statSync(path.join(src, d)).isDirectory()).sort();
let stale = [];
for (const id of ids) {
    const bytes = pack(id);
    const digest = sha(bytes);
    const entry = catalog.packages.find((p) => p.runtime?.kind === 'node_app' && p.runtime.package === id);
    if (!entry) {
        console.error(`packages/node/${id} has no node_app entry in packages/catalog.json`);
        process.exit(1);
    }
    if (entry.runtime.sha256 !== digest) stale.push(`${id}: catalog ${entry.runtime.sha256 ?? '(none)'} vs ${digest}`);
    if (!check) {
        entry.runtime.sha256 = digest;
        mkdirSync(path.join(outDir, 'packages'), { recursive: true });
        writeFileSync(path.join(outDir, 'packages', `${id}.bin`), bytes);
        console.log(`${id}: ${bytes.length} bytes, ${digest.slice(0, 12)}`);
    }
}
if (check) {
    if (stale.length) {
        console.error(`node-app package hashes are stale; run node scripts/pack-node-apps.mjs <dir>:\n  ${stale.join('\n  ')}`);
        process.exit(1);
    }
    console.log(`${ids.length} node-app packages match packages/catalog.json`);
} else if (stale.length) {
    writeFileSync(catalogPath, JSON.stringify(catalog, null, 1) + '\n');
    console.log(`pinned ${stale.length} hash(es) in packages/catalog.json`);
}
