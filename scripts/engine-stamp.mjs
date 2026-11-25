#!/usr/bin/env node
// The engine's build stamp: what an engine package (pkg/node, pkg/web) was built from, so a measure refuses an engine
// built from other sources than the tree it records. On 2026-11-25 a scene-pixels-dev row named commit f84bebf7 and
// measured an engine built before e4ca5baf, which that commit holds: its truth graph was the older one.
//
// The stamp is the SHA-256 of the engine's inputs as files (no git: the build machines hold synced trees without
// one): every file of the engine's crates (cw-wasm, or the crates CW_WASM_PACKAGE and CW_APP_MODULE_PACKAGE name, as
// scripts/build-wasm.sh builds them) and of the workspace crates they depend on, Cargo.lock, scripts/build-wasm.sh
// and packages/catalog.json. The stamp records the crates it started from, and a check starts from the same ones. scripts/build-wasm.sh writes it into each package as build-stamp.json; a runner calls
// checkEngine before it loads the package, and records what it returns in its metrics.
//
//   node scripts/engine-stamp.mjs write <package dir>...   (scripts/build-wasm.sh, after wasm-bindgen)
//   node scripts/engine-stamp.mjs check <package dir>      (exit 1 when it does not match this tree)
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
// The workspace the engine is built in: this repository, or one that holds it as its `core/` submodule and wraps its
// crates (scripts/build-wasm.sh's CW_WASM_WORKSPACE).
const ROOT = process.env.CW_WASM_WORKSPACE
  ? path.resolve(process.env.CW_WASM_WORKSPACE)
  : path.basename(HERE) === 'core' && existsSync(path.join(HERE, '..', 'Cargo.toml')) ? path.resolve(HERE, '..') : HERE;
const SKIP = new Set(['target', '.git', 'node_modules']);

const walk = (dir, out = []) => {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    if (SKIP.has(e.name)) continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else if (e.isFile()) out.push(p);
  }
  return out;
};

/** Every workspace crate by its package name: name -> its directory. */
function crates(root) {
  const byName = new Map();
  const dirs = [path.join(root, 'crates'), path.join(root, 'core', 'crates')].filter(existsSync);
  for (const f of dirs.flatMap((d) => walk(d))) {
    if (path.basename(f) !== 'Cargo.toml') continue;
    const name = /^\[package\][^[]*?^name\s*=\s*"([^"]+)"/ms.exec(readFileSync(f, 'utf8'))?.[1];
    if (name) byName.set(name, path.dirname(f));
  }
  return byName;
}

/** The names a manifest depends on in its normal and build dependency tables (dev-dependencies are not built in). */
function dependencies(manifest, known) {
  const out = new Set();
  let on = false;
  for (const line of readFileSync(manifest, 'utf8').split('\n')) {
    const table = /^\s*\[([^\]]+)\]/.exec(line)?.[1];
    if (table) { on = /(^|\.)(dependencies|build-dependencies)$/.test(table) && !table.endsWith('dev-dependencies'); continue; }
    const key = on && /^\s*([A-Za-z0-9_-]+)\s*(=|\.)/.exec(line)?.[1];
    if (key && known.has(key)) out.add(key);
  }
  return out;
}

/** The crates the engine is built from (scripts/build-wasm.sh): its own, and its application modules' when it has them. */
export function enginePackages(env = process.env) {
  return [env.CW_WASM_PACKAGE || 'cw-wasm', ...(env.CW_APP_MODULE_PACKAGE ? [env.CW_APP_MODULE_PACKAGE] : [])];
}

/** The SHA-256 of the engine's inputs in the tree at `root`, from crates `packages`, and how many files it read. */
export function engineInputs(root = ROOT, packages = enginePackages()) {
  const known = crates(root);
  const seen = new Set(), todo = [...packages];
  while (todo.length) {
    const n = todo.pop();
    if (seen.has(n) || !known.has(n)) continue;
    seen.add(n);
    for (const d of dependencies(path.join(known.get(n), 'Cargo.toml'), known)) todo.push(d);
  }
  for (const p of packages) if (!seen.has(p)) throw Error(`no ${p} crate under ${root}/crates`);
  const dirs = [...seen].map((n) => known.get(n));
  const files = new Set(['Cargo.lock', ...['scripts/build-wasm.sh', 'packages/catalog.json'].flatMap((f) => [f, `core/${f}`])].map((f) => path.join(root, f)).filter(existsSync));
  // A crate nested in another's directory is walked once, with its parent.
  for (const d of dirs) if (!dirs.some((o) => o !== d && d.startsWith(o + path.sep))) for (const f of walk(d)) files.add(f);
  const h = createHash('sha256');
  for (const f of [...files].map((f) => path.relative(root, f)).sort()) {
    h.update(f.split(path.sep).join('/')); h.update('\0');
    h.update(createHash('sha256').update(readFileSync(path.join(root, f))).digest()); h.update('\0');
  }
  return { inputs: h.digest('hex'), files: files.size, crates: seen.size };
}

const sha = (f) => createHash('sha256').update(readFileSync(f)).digest('hex');

/**
 * The engine package at `pkg` against the tree at `root`: its stamp must name this tree's inputs. Returns what a row
 * records ({engine_inputs, engine_sha256}); throws when the package has no stamp or another one.
 */
export function checkEngine(pkg, root = ROOT) {
  const stampFile = path.join(pkg, 'build-stamp.json');
  if (!existsSync(stampFile)) throw Error(`${pkg} has no build-stamp.json: build the engine from this tree (scripts/build-wasm.sh) before measuring`);
  const stamp = JSON.parse(readFileSync(stampFile, 'utf8'));
  const now = engineInputs(root, stamp.packages ?? ['cw-wasm']);
  if (stamp.inputs !== now.inputs) throw Error(`${pkg} was built from other engine sources (stamp ${stamp.inputs.slice(0, 12)}, this tree ${now.inputs.slice(0, 12)}): build the engine from this tree (scripts/build-wasm.sh) before measuring`);
  return { engine_inputs: now.inputs, engine_sha256: sha(path.join(pkg, 'computerworld_bg.wasm')) };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const [cmd, ...dirs] = process.argv.slice(2);
  if (cmd === 'write' && dirs.length) {
    const packages = enginePackages();
    const s = engineInputs(ROOT, packages);
    for (const d of dirs) writeFileSync(path.join(d, 'build-stamp.json'), JSON.stringify({ inputs: s.inputs, packages, files: s.files, crates: s.crates, wasm_sha256: sha(path.join(d, 'computerworld_bg.wasm')) }, null, 1) + '\n');
    console.error(`engine stamp ${s.inputs.slice(0, 12)} (${s.crates} crates, ${s.files} files) -> ${dirs.join(', ')}`);
  } else if (cmd === 'check' && dirs.length === 1) {
    try { console.log(JSON.stringify(checkEngine(dirs[0]))); } catch (e) { console.error(e.message); process.exit(1); }
  } else {
    console.error('usage: engine-stamp.mjs write <package dir>... | check <package dir>');
    process.exit(2);
  }
}
