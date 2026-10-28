#!/usr/bin/env node
// Split a lazy engine into the engine a client downloads first and code modules it links
// the first time one of their functions runs (docs/app-modules.md § Code modules).
//
//   node scripts/split-modules.mjs split <engine.wasm> <node glue> <linked.wasm> <engine.map> <out-dir>
//   node scripts/split-modules.mjs glue <computerworld.js>...
//
// `split` reads the engine wasm-bindgen wrote (built with its function names kept), puts
// each function of a module's crates into that module (`PLAN`) with every function only
// those reach, and runs binaryen's `wasm-split --multi-split` (binaryen 121 or later,
// from BINARYEN, a bin directory, or PATH): <out-dir>/engine.wasm and
// <out-dir>/modules/<name>.wasm, all without names. A call into a module before it is
// linked goes to a placeholder the glue answers. The data only a module reads moves to
// that module (`moveData`, from the linker's map and the relocations of <linked.wasm>,
// wasm-ld's own output). Last, it writes each module's SHA-256 into the engine (the
// `code-modules` feature of cw-wasm reserves the space), so the engine pins what it
// links. The split moves code and data, never changes them: an engine and its modules
// run what the unsplit build runs.
//
// `glue` patches wasm-bindgen's JavaScript for the split engine: its imports answer the
// placeholders (crates/bindings/wasm/js/link.js), and `setModuleHost` keeps the host
// that links the modules.
import { readFileSync, writeFileSync, mkdirSync, mkdtempSync, rmSync, readdirSync } from 'node:fs';
import os from 'node:os';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { gzipSync } from 'node:zlib';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SERVICE_KINDS = ['assistant', 'bank', 'calendar', 'chat', 'discord', 'docs', 'drive', 'forum', 'geo', 'git', 'issues', 'mail', 'media', 'messages', 'press', 'search', 'shop', 'slack', 'social', 'speaker', 'static_site', 'wiki'];

// Each code module and the crates whose functions it holds, by rank. A function goes to a
// module when it belongs to that module's crates (its path, or the type it is a method
// of, is theirs), or to one of its `paths` (a module or a type of a crate, `crate::path`),
// or matches one of its `functions`, and to no other module's of the same or a lower
// rank; then every
// function only a module's functions call goes to it too. A generic of the standard
// library or the JSON code instantiated for a module's type is placed by its callers
// alone: the compiler merges identical instantiations, so one named for a page's type
// may be the one the engine calls for its own. A function that belongs to two modules of
// the lowest rank it belongs to stays in the engine. A `profiled` module also leaves in
// the engine what every world runs (scripts/split-workload.cjs): a desktop holds a
// browser's state from its first boot and hashes and snapshots it.
const PLAN = {
    modules: {
        ...Object.fromEntries(SERVICE_KINDS.map((kind) => [`service-${kind.replace('_', '-')}`, { rank: 0, crates: [`cw_service_${kind}`] }])),
        // The HTML engine, the browser and the web applications' runtime.
        web: { rank: 1, crates: ['cw_web', 'cw_ui', 'cw_browser', 'cw_tsx'], paths: ['cw_applications::web_app'], profiled: true },
        // The JavaScript engine pages and `node` run on.
        javascript: { rank: 1, crates: ['cw_jsvm'], profiled: true },
        // The SQL engine `sqlite3` runs on.
        sqlite: { rank: 1, crates: ['cw_sql'], profiled: true },
        // The renderer, the rasterizer and the image decoders: a world driven through its
        // scene never draws, so it never links them (docs/app-modules.md § Render files),
        // with the scene renderer a page drives itself (`SceneRenderer`).
        render: { rank: 1, crates: ['cw_render', 'fontdue', 'jpeg_decoder'], paths: ['cw_wasm::JsSceneRenderer'] },
        // Reading a snapshot back (import, restore, fork) and checking it: the parsers of
        // every type of a world's state, which a world that is never restored never runs;
        // and writing a world out as JSON (an exported snapshot, the owner's `inspect`),
        // which a world that is never exported never runs: a step hashes its state with
        // `cw_encode`, not as JSON.
        snapshot: {
            rank: 1,
            crates: [],
            functions: [
                /^<computerworld\[[0-9a-f]+\]::World>::(import_snapshot|restore|fork|export_snapshot|inspect)$/,
                /^<cw_environment\[[0-9a-f]+\]::Environment>::(import_snapshot|restore|fork|validate_snapshot|export_snapshot|inspect)$/,
                /^<cw_kernel\[[0-9a-f]+\]::Runtime>::(fork_and_restore|restore_path|validate_snapshot|forked|inspect)$/,
            ],
        },
        // The engine's side of the application modules (docs/app-modules.md § The interface):
        // the parsers of what a module answers (effects, windows' scenes, pages), linked the
        // first time a heavy application is launched, which fetches its module anyway.
        'app-calls': { rank: 1, crates: [], paths: ['cw_applications::apps::module::ModuleApp'] },
        // The image editors (GIMP, Pinta, Paint, Pixelmator, Sketchbook).
        imaging: { rank: 1, crates: [], paths: ['cw_applications::apps::imaging'] },
        // The debugger of programs (`python3 -m pdb`, `node inspect`, the debug protocol)
        // and the shell's larger tools, each linked the first time it runs.
        debugger: { rank: 1, crates: [], paths: ['cw_computer::debugger'] },
        ...Object.fromEntries(['awk', 'sed', 'git', 'archive', 'find', 'datautils'].map((tool) => [tool, { rank: 1, crates: [], paths: [`cw_computer::${tool}`] }])),
    },
    // What runs whatever the world: every kind is registered when a registry is made,
    // which asks its kind and version, so those stay in the engine and a world fetches
    // only the kinds it declares.
    keep: [
        // The host hands the renderer its loaders when it is set up, and `lazyManifest()`
        // lists the render files: neither draws.
        /^<?(?:&mut )?cw_render\[[0-9a-f]+\]::(?:font_pack::(?:font_pack_status|fetched_ui_faces|set_font_loader)|render_pack::(?:fetches_render_files|set_asset_loader|RenderFile>::(?:path|is_font))|faces::draws_ui_slot)\b/,
        /^cw_service_[a-z_]+\[[0-9a-f]+\]::register$/,
        /^<cw_sdk\[[0-9a-f]+\]::Registry>::register(_service)?::</,
        / as cw_sdk\[[0-9a-f]+\]::Service>::(kind|version)$/,
    ],
};
const MARKER = Buffer.from('computerworld code modules:\n');
const NAMESPACE = 'cw-code';

function binaryen(tool) {
    return process.env.BINARYEN ? path.join(process.env.BINARYEN, tool) : tool;
}
/** Binaryen's names for the engine's functions (unique, escaped as `\hh`), the direct
 * calls between them, and which are indirect targets. */
function callGraph(input) {
    const run = (pass) => execFileSync(binaryen('wasm-opt'), [input, pass, '-o', '/dev/null'], { encoding: 'utf8', maxBuffer: 1 << 30 });
    const names = run('--print-function-map').split('\n').filter(Boolean).map((line) => line.slice(line.indexOf(':') + 1));
    const callers = new Map(names.map((n) => [n, []]));
    const indirect = new Set();
    for (const line of run('--print-call-graph').split('\n')) {
        let m = /^ {2}"(.*)" -> "(.*)"; \/\/ call$/.exec(line);
        if (m) { callers.get(m[2])?.push(m[1]); continue; }
        m = /^ {2}"(.*)" \[style="filled, rounded"\];$/.exec(line);
        if (m) indirect.add(m[1]);
    }
    if (!names.some((n) => n.includes('::'))) throw new Error('the engine has no function names: build it with strip = "none"');
    return { names, callers, indirect };
}
/** Binaryen's names of the functions scripts/split-workload.cjs runs in the unsplit engine. */
function profile(input, glue) {
    const root = path.join(path.dirname(fileURLToPath(import.meta.url)), '..');
    const dir = mkdtempSync(path.join(os.tmpdir(), 'split-profile-'));
    try {
        execFileSync(binaryen('wasm-split'), ['--instrument', input, '-o', path.join(dir, 'computerworld_bg.wasm')], { stdio: ['ignore', 'ignore', 'inherit'] });
        writeFileSync(path.join(dir, 'computerworld.js'), `${readFileSync(glue, 'utf8')}\nmodule.exports.__wasm = wasm;\n`);
        writeFileSync(path.join(dir, 'modules.cjs'), readFileSync(path.join(root, 'crates/bindings/wasm/js/modules.cjs')));
        execFileSync(process.execPath, [path.join(root, 'scripts/split-workload.cjs'), dir, root, path.join(dir, 'profile')], { stdio: 'inherit' });
        const printed = execFileSync(binaryen('wasm-split'), [`--print-profile=${path.join(dir, 'profile')}`, input], { encoding: 'utf8', maxBuffer: 1 << 30, stdio: ['ignore', 'pipe', 'ignore'] });
        return new Set(printed.split('\n').filter((l) => l.startsWith('+ ')).map((l) => l.slice(2)));
    } finally {
        rmSync(dir, { recursive: true, force: true });
    }
}
const unescape = (name) => name.replace(/\\([0-9a-f]{2})/g, (_, hex) => String.fromCharCode(parseInt(hex, 16)));

/** Each module's functions: those its crates name (and no other module's), then every
 * function only they reach. A function anything else calls, an indirect target that no
 * module's crates name, and a function nothing calls directly (an export) stay in the
 * engine. */
function placement({ names, callers, indirect }, ran) {
    const prefix = '^[<&* ]*(?:(?:dyn|mut) )*';
    const within = (p) => { const [c, ...rest] = p.split('::'); return new RegExp(`${prefix}${c}\\[[0-9a-f]+\\]::${rest.join('::')}(?:::|>|$)`); };
    const crates = Object.entries(PLAN.modules).map(([module, { rank, crates: list, paths = [], functions = [] }]) => [module, rank, [...list.map((c) => new RegExp(`${prefix}${c}\\[`)), ...paths.map(within), ...functions]]);
    const ENGINE = '';
    const owners = new Map();
    const stay = new Map(); // module -> how many of its functions every world runs
    for (const name of names) {
        const plain = unescape(name);
        if (PLAN.keep.some((k) => k.test(plain))) { owners.set(name, new Set([ENGINE])); continue; }
        const named = crates.filter(([, , patterns]) => patterns.some((p) => p.test(plain)));
        const lowest = Math.min(...named.map(([, rank]) => rank));
        const hits = named.filter(([, rank]) => rank === lowest).map(([m]) => m);
        if (hits.length === 1 && PLAN.modules[hits[0]].profiled && ran.has(name)) { owners.set(name, new Set([ENGINE])); stay.set(hits[0], (stay.get(hits[0]) ?? 0) + 1); }
        else if (hits.length === 1) owners.set(name, new Set(hits));
        else if (hits.length > 1 || indirect.has(name) || callers.get(name).length === 0) owners.set(name, new Set([ENGINE]));
    }
    const fixed = new Set(owners.keys());
    for (const name of names) if (!fixed.has(name)) owners.set(name, new Set());
    for (let changed = true; changed;) {
        changed = false;
        for (const name of names) {
            if (fixed.has(name)) continue;
            const own = owners.get(name);
            for (const caller of callers.get(name)) {
                for (const o of owners.get(caller)) if (!own.has(o)) { own.add(o); changed = true; }
            }
        }
    }
    const placed = new Map(Object.keys(PLAN.modules).map((m) => [m, []]));
    for (const name of names) {
        const own = owners.get(name);
        if (own.size === 1 && !own.has(ENGINE)) placed.get([...own][0]).push(name);
    }
    for (const [module, n] of stay) console.log(`${module}: ${n} functions every world runs stay in the engine`);
    return placed;
}

// Moving data. A module's code reads constants of its own (its pages' stylesheets and
// templates, its strings and tables) that the split leaves in the engine's data. The
// linker's map (`--Map`) gives every data object's address and size, and its relocations
// (`--emit-relocs`) every pointer in the data. An object moves to
// a module when that module is the only one that points into it, directly or through
// other objects, and the engine never does: the engine's segment loses those bytes (its
// memory there stays zero until the module is linked) and the module gets a segment that
// writes them back at the same address when it is instantiated, before any of its
// functions run. A pointer is an address in an object: in the code, the immediate of an
// `i32.const` or the offset of a load or store that is an address the linker resolved
// somewhere in the code.
function leb(bytes, at) {
    let value = 0, shift = 0, byte;
    do { byte = bytes[at++]; value += (byte & 0x7f) * 2 ** shift; shift += 7; } while (byte & 0x80);
    return [value, at];
}
function sleb32(bytes, at) {
    let value = 0, shift = 0, byte, n = 0;
    do {
        if (n++ === 5 || at >= bytes.length) return null;
        byte = bytes[at++]; value |= (byte & 0x7f) << shift; shift += 7;
    } while (byte & 0x80);
    if (shift < 32 && (byte & 0x40)) value |= -1 << shift;
    return value >>> 0;
}
function encodeLeb(value) {
    const out = [];
    do { let b = value & 0x7f; value = Math.floor(value / 128); if (value) b |= 0x80; out.push(b); } while (value);
    return Buffer.from(out);
}
function encodeSleb(value) {
    const out = [];
    for (;;) {
        const b = value & 0x7f; value >>= 7;
        if ((value === 0 && !(b & 0x40)) || (value === -1 && (b & 0x40))) { out.push(b); break; }
        out.push(b | 0x80);
    }
    return Buffer.from(out);
}
function sections(bytes) {
    const list = [];
    for (let at = 8; at < bytes.length;) {
        const id = bytes[at];
        const [size, start] = leb(bytes, at + 1);
        list.push({ id, head: at, start, end: start + size });
        at = start + size;
    }
    return list;
}
function assemble(bytes, list) {
    const parts = [bytes.subarray(0, 8)];
    for (const section of list) {
        const payload = section.payload ?? bytes.subarray(section.start, section.end);
        parts.push(Buffer.from([section.id]), encodeLeb(payload.length), payload);
    }
    return Buffer.concat(parts);
}
function dataSegments(bytes, section) {
    const segments = [];
    let [count, at] = leb(bytes, section.start);
    for (let i = 0; i < count; i++) {
        const flags = bytes[at++];
        if (flags !== 0 || bytes[at] !== 0x41) throw new Error(`data segment ${i} is not an active segment at a constant address`);
        const address = sleb32(bytes, at + 1);
        [, at] = leb(bytes, at + 1);
        if (bytes[at++] !== 0x0b) throw new Error(`data segment ${i}'s address is not a plain constant`);
        let length;
        [length, at] = leb(bytes, at);
        segments.push({ address, bytes: bytes.subarray(at, at + length) });
        at += length;
    }
    return segments;
}
function encodeSegments(segments) {
    return Buffer.concat([encodeLeb(segments.length), ...segments.flatMap((s) => [Buffer.from([0, 0x41]), encodeSleb(s.address | 0), Buffer.from([0x0b]), encodeLeb(s.bytes.length), s.bytes])]);
}
/** Every value in a code section that can be an address: the immediates of `i32.const`
 * and the offsets of loads and stores. */
function addressesInCode(bytes, section, found) {
    let at = section.start;
    const u = () => { let v = 0, shift = 0, b; do { b = bytes[at++]; v += (b & 0x7f) * 2 ** shift; shift += 7; } while (b & 0x80); return v; };
    const skip = () => { while (bytes[at++] & 0x80); };
    const memarg = () => { if (u() & 0x40) skip(); found(u()); };
    const count = u();
    for (let f = 0; f < count; f++) {
        const size = u();
        const end = at + size;
        for (let groups = u(); groups > 0; groups--) { skip(); at++; }
        while (at < end) {
            const op = bytes[at++];
            if (op === 0x41) { found(sleb32(bytes, at)); skip(); }
            else if (op === 0x42) skip();
            else if (op >= 0x28 && op <= 0x3e) memarg();
            else if (op <= 0x01 || op === 0x05 || op === 0x0b || op === 0x0f || op === 0x1a || op === 0x1b || op === 0xd1 || (op >= 0x45 && op <= 0xc4)) continue;
            else if (op >= 0x02 && op <= 0x04) { const t = bytes[at]; if (t === 0x40 || (t >= 0x6f && t <= 0x7f)) at++; else skip(); }
            else if (op === 0x0c || op === 0x0d || op === 0x10 || op === 0x12 || (op >= 0x20 && op <= 0x26) || op === 0x3f || op === 0x40 || op === 0xd2) skip();
            else if (op === 0x0e) { for (let n = u() + 1; n > 0; n--) skip(); }
            else if (op === 0x11 || op === 0x13) { skip(); skip(); }
            else if (op === 0x1c) { at += u(); }
            else if (op === 0x43) at += 4;
            else if (op === 0x44) at += 8;
            else if (op === 0xd0) at++;
            else if (op === 0xfc) {
                const sub = u();
                if (sub <= 7) continue;
                if (sub === 8 || sub === 10 || sub === 12 || sub === 14) { skip(); skip(); }
                else if (sub <= 17) skip();
                else throw new Error(`code at ${at}: 0xfc ${sub} is not read here`);
            } else if (op === 0xfd) {
                const sub = u();
                if (sub <= 11 || sub === 92 || sub === 93) memarg();
                else if (sub >= 84 && sub <= 91) { memarg(); at++; }
                else if (sub === 12 || sub === 13) at += 16;
                else if (sub >= 21 && sub <= 34) at++;
            } else if (op === 0xfe) {
                const sub = u();
                if (sub === 3) at++;
                else memarg();
            } else throw new Error(`code at ${at - 1}: opcode 0x${op.toString(16)} is not read here`);
        }
        if (at !== end) throw new Error(`code: function ${f} did not end where its size says`);
    }
}
function dataObjects(mapFile) {
    const objects = [];
    let inData = false;
    for (const line of readFileSync(mapFile, 'utf8').split('\n')) {
        const m = /^ *([0-9a-f]+|-) +[0-9a-f]+ +([0-9a-f]+)( +)(\S.*)$/.exec(line);
        if (!m) continue;
        if (m[1] === '-') { inData = m[4] === 'DATA'; continue; }
        if (!inData || m[3].length !== 9) continue;
        const section = /:\((\.[a-z]+)/.exec(m[4]);
        const size = parseInt(m[2], 16);
        if (!size || !section || section[1] === '.bss' || section[1] === '.tbss') continue;
        objects.push({ start: parseInt(m[1], 16), end: parseInt(m[1], 16) + size, name: m[4], owners: new Set() });
    }
    objects.sort((a, b) => a.start - b.start);
    return objects;
}
/** The relocations of section `name` (`reloc.DATA`, `reloc.CODE`) in the linked engine
 * (wasm-ld `--emit-relocs`), as { type, site } with the site's offset in the file. */
function relocations(bytes, list, name) {
    const reloc = list.find((s) => s.id === 0 && bytes.subarray(s.start + 1, s.start + 1 + bytes[s.start]).toString() === name);
    if (!reloc) throw new Error(`the linked engine has no ${name}: link it with --emit-relocs`);
    const WITH_ADDEND = new Set([3, 4, 5, 8, 9, 11, 14, 15, 16, 17, 21, 22, 23, 25]);
    let at = reloc.start + 1 + bytes[reloc.start], target, entries;
    [target, at] = leb(bytes, at);
    [entries, at] = leb(bytes, at);
    const into = list[target];
    if (into?.id !== (name === 'reloc.CODE' ? 10 : 11)) throw new Error(`${name} names section ${target}, which is not what it relocates`);
    const out = [];
    for (let i = 0; i < entries; i++) {
        const type = bytes[at++];
        let offset;
        [offset, at] = leb(bytes, at);
        [, at] = leb(bytes, at);
        if (WITH_ADDEND.has(type)) { while (bytes[at++] & 0x80); }
        out.push({ type, site: into.start + offset });
    }
    return out;
}
/** What the linker resolved in the engine (`--emit-relocs`): every address the code uses
 * (`R_WASM_MEMORY_ADDR_LEB` and `_SLEB`: load and store offsets, `i32.const`), and every
 * pointer in the data, as [site, value] (`R_WASM_MEMORY_ADDR_I32`). */
function linkedAddresses(linkedPath) {
    const bytes = readFileSync(linkedPath);
    const list = sections(bytes);
    const data = list.find((s) => s.id === 11);
    const segments = [];
    let [count, at] = leb(bytes, data.start);
    for (let i = 0; i < count; i++) {
        if (bytes[at] !== 0 || bytes[at + 1] !== 0x41) throw new Error(`${linkedPath}: data segment ${i} is not an active segment at a constant address`);
        const address = sleb32(bytes, at + 2);
        [, at] = leb(bytes, at + 2);
        let length;
        [length, at] = leb(bytes, at + 1);
        segments.push({ address, from: at, to: at + length });
        at += length;
    }
    const code = new Set();
    for (const { type, site } of relocations(bytes, list, 'reloc.CODE')) {
        if (type === 3) code.add(leb(bytes, site)[0]);
        else if (type === 4) code.add(sleb32(bytes, site));
    }
    const pointers = [];
    for (const { type, site } of relocations(bytes, list, 'reloc.DATA')) {
        if (type !== 5) continue;
        const segment = segments.find((s) => site >= s.from && site + 4 <= s.to);
        if (!segment) throw new Error(`${linkedPath}: a data relocation is outside every segment`);
        pointers.push([segment.address + (site - segment.from), bytes.readUInt32LE(site)]);
    }
    return { code, pointers };
}
function objectAt(objects, value) {
    let lo = 0, hi = objects.length - 1;
    while (lo <= hi) {
        const mid = (lo + hi) >> 1;
        if (value < objects[mid].start) hi = mid - 1;
        else if (value >= objects[mid].end) lo = mid + 1;
        else return objects[mid];
    }
    return null;
}
function moveData(enginePath, linkedPath, modulesDir, mapFile) {
    const engine = readFileSync(enginePath);
    const engineSections = sections(engine);
    const dataSection = engineSections.find((s) => s.id === 11);
    const segments = dataSegments(engine, dataSection);
    const objects = dataObjects(mapFile);
    const segmentOf = (o) => segments.find((s) => o.start >= s.address && o.end <= s.address + s.bytes.length);
    // The map must describe this binary: the reserved space starts with its marker.
    const space = objects.find((o) => o.name.includes('CODE_MODULES)'));
    const holder = space && segmentOf(space);
    if (!holder || !holder.bytes.subarray(space.start - holder.address).subarray(0, MARKER.length).equals(MARKER)) {
        throw new Error(`${mapFile} does not describe ${enginePath}`);
    }
    const ENGINE = '';
    const code = engineSections.find((s) => s.id === 10);
    const linked = linkedAddresses(linkedPath);
    // A constant that is no address the linker resolved is a number, not a pointer.
    const pointer = (v) => (linked.code.has(v) ? objectAt(objects, v) : null);
    addressesInCode(engine, code, (v) => pointer(v)?.owners.add(ENGINE));
    const modules = new Map();
    for (const file of readdirSync(modulesDir).sort()) {
        const bytes = readFileSync(path.join(modulesDir, file));
        const list = sections(bytes);
        const own = list.find((s) => s.id === 10);
        const name = file.replace(/\.wasm$/, '');
        modules.set(name, { file: path.join(modulesDir, file), bytes, list });
        if (own) addressesInCode(bytes, own, (v) => pointer(v)?.owners.add(name));
    }
    const edges = [];
    for (const [site, value] of linked.pointers) {
        const from = objectAt(objects, site), to = objectAt(objects, value);
        if (from && to && from !== to) edges.push([from, to]);
    }
    for (const o of objects) if (!segmentOf(o)) o.owners.add(ENGINE);
    for (let changed = true; changed;) {
        changed = false;
        for (const [from, to] of edges) for (const owner of from.owners) if (!to.owners.has(owner)) { to.owners.add(owner); changed = true; }
    }
    const moving = objects.filter((o) => o.owners.size === 1 && !o.owners.has(ENGINE)).map((o) => ({ ...o, module: [...o.owners][0], segment: segmentOf(o) }));
    // Neighbours that go to the same module move as one run, with the zero padding
    // between them.
    const runs = [];
    for (const o of moving) {
        const last = runs.at(-1);
        if (last && last.module === o.module && last.segment === o.segment && o.start - last.end <= 16
            && o.segment.bytes.subarray(last.end - o.segment.address, o.start - o.segment.address).every((b) => b === 0)) last.end = o.end;
        else runs.push({ start: o.start, end: o.end, module: o.module, segment: o.segment });
    }
    const kept = [];
    for (const segment of segments) {
        let from = 0;
        for (const r of runs) {
            if (r.segment !== segment) continue;
            const a = r.start - segment.address;
            if (a > from) kept.push({ address: segment.address + from, bytes: segment.bytes.subarray(from, a) });
            from = r.end - segment.address;
        }
        if (from < segment.bytes.length) kept.push({ address: segment.address + from, bytes: segment.bytes.subarray(from) });
    }
    // Memory starts zeroed: a segment of zeros (the .bss --emit-relocs keeps) writes nothing.
    kept.splice(0, kept.length, ...kept.filter((k) => k.bytes.some((b) => b !== 0)));
    dataSection.payload = encodeSegments(kept);
    const count = engineSections.find((s) => s.id === 12);
    if (count) count.payload = encodeLeb(kept.length);
    // The linker's relocations and symbols, which wasm-bindgen and wasm-split carry over,
    // describe the unsplit engine: nothing reads them.
    const custom = (s) => s.id === 0 && engine.subarray(s.start + 1, s.start + 1 + engine[s.start]).toString();
    const stale = (s) => custom(s) === 'linking' || String(custom(s)).startsWith('reloc.');
    writeFileSync(enginePath, assemble(engine, engineSections.filter((s) => !stale(s))));
    const moved = new Map();
    for (const [name, module] of modules) {
        const mine = runs.filter((r) => r.module === name);
        if (!mine.length) continue;
        if (module.list.some((s) => s.id === 11 || s.id === 12)) throw new Error(`${name} has data of its own`);
        const payload = encodeSegments(mine.map((r) => ({ address: r.start, bytes: r.segment.bytes.subarray(r.start - r.segment.address, r.end - r.segment.address) })));
        module.list.splice(module.list.findIndex((s) => s.id === 10) + 1, 0, { id: 11, payload });
        writeFileSync(module.file, assemble(module.bytes, module.list));
        moved.set(name, mine.reduce((n, r) => n + r.end - r.start, 0));
    }
    return moved;
}

function wasmSplit() {
    const tool = binaryen('wasm-split');
    let help = '';
    try { help = execFileSync(tool, ['--help'], { encoding: 'utf8' }); } catch (e) { help = String(e.stdout ?? ''); }
    if (!help.includes('--multi-split')) {
        throw new Error(`${tool} has no --multi-split: set BINARYEN to the bin directory of binaryen 121 or later`);
    }
    return tool;
}

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const sizes = (bytes) => `${String(bytes.length).padStart(12)} raw ${String(gzipSync(bytes, { level: 9 }).length).padStart(12)} gzip -9`;

function split(input, glue, linked, map, outDir) {
    const placed = placement(callGraph(input), profile(input, glue));
    rmSync(outDir, { recursive: true, force: true });
    mkdirSync(path.join(outDir, 'modules'), { recursive: true });
    const manifest = path.join(outDir, 'split.manifest');
    // wasm-split escapes the names it reads, so the manifest names functions unescaped.
    writeFileSync(manifest, [...placed].filter(([, fns]) => fns.length).map(([m, fns]) => `${m}\n${fns.map(unescape).join('\n')}\n`).join('\n'));
    const engine = path.join(outDir, 'engine.wasm');
    execFileSync(wasmSplit(), [
        '--multi-split', input, '--manifest', manifest, '--out-prefix', path.join(outDir, 'modules') + path.sep,
        '-o', engine, '--placeholder-namespace', NAMESPACE, '--import-namespace', 'engine',
        // CW_SPLIT_NAMES=1 keeps the function names, to attribute what stays in the engine.
        '--export-prefix', '%', ...(process.env.CW_SPLIT_NAMES ? ['-g'] : ['--strip-debug']),
    ], { stdio: 'inherit' });
    rmSync(manifest);
    const moved = moveData(engine, linked, path.join(outDir, 'modules'), map);
    const pins = [];
    for (const file of readdirSync(path.join(outDir, 'modules')).sort()) {
        const name = file.replace(/\.wasm$/, '');
        pins.push(`${name} ${sha256(readFileSync(path.join(outDir, 'modules', file)))}\n`);
    }
    const pinned = readFileSync(engine);
    const at = pinned.indexOf(MARKER);
    if (at < 0 || pinned.indexOf(MARKER, at + 1) >= 0) throw new Error('the engine reserves no single space for its code modules: build cw-wasm with code-modules');
    const text = Buffer.from(pins.join(''));
    if (!pinned.subarray(at + MARKER.length, at + MARKER.length + text.length + 1).every((b) => b === 0)) {
        throw new Error('the engine reserves too little space for its code modules');
    }
    text.copy(pinned, at + MARKER.length);
    writeFileSync(engine, pinned);
    console.log(`${engine} ${sizes(pinned)} (engine)`);
    for (const line of pins) {
        const name = line.split(' ')[0];
        const data = moved.has(name) ? `, ${moved.get(name)} bytes of data` : '';
        console.log(`modules/${name}.wasm ${sizes(readFileSync(path.join(outDir, 'modules', `${name}.wasm`)))} (code module, ${placed.get(name).length} functions${data})`);
    }
}

function glue(files) {
    const link = readFileSync(path.join(path.dirname(fileURLToPath(import.meta.url)), '../crates/bindings/wasm/js/link.js'), 'utf8');
    for (const file of files) {
        let text = readFileSync(file, 'utf8');
        let calls = 0;
        text = text.replace(/(?<!function )__wbg_get_imports\(\)/g, () => { calls++; return '__cw_link_imports(__wbg_get_imports())'; });
        let hosts = 0;
        text = text.replace(/^((?:export )?function setModuleHost\(host\) \{)$/m, (line) => { hosts++; return `${line}\n    __cw_link_host = host;`; });
        if (calls === 0 || hosts !== 1) throw new Error(`${file}: wasm-bindgen's glue changed (${calls} instantiations, ${hosts} setModuleHost)`);
        writeFileSync(file, `${text}\n${link}`);
    }
}

const [command, ...args] = process.argv.slice(2);
if (command === 'split' && args.length === 5) split(...args);
else if (command === 'glue' && args.length) glue(args);
else {
    console.error('usage: split-modules.mjs split <engine.wasm> <node glue> <linked.wasm> <engine.map> <out-dir> | glue <computerworld.js>...');
    process.exit(2);
}
