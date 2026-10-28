'use strict';
// The module host of a lazy build (docs/app-modules.md), for Node: the engine asks it
// for a module, a font or a data file by name and SHA-256 and waits for the answer.
// Files are read beside the engine (`modules/<name>.wasm`, `fonts/<file>`,
// `data/<file>`), then from `shared` (in the npm package, `lazy/shared/`: what every
// platform's engine fetches alike), or through `read(path)` (a cache filled from a
// server); what a frame draws with is read from the render bundle `render.bin` beside
// the engine;
// bytes that do not hash to what the engine asked for are refused, so a run always
// loads the code and data it recorded.
//
//   const engine = require('computerworld/lazy/ubuntu/computerworld.cjs');
//   const { createModuleHost } = require('computerworld/lazy/ubuntu/modules.cjs');
//   engine.setModuleHost(createModuleHost());
//
// `setModuleHost` gives the host `core(request) -> reply`: a module asks the core
// through its import `cw.core` while one of its calls runs (text tables, a program's
// files, network and processes), and the host passes the question on. An engine split
// into code modules asks `link` for one the first time one of its functions runs.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

// The render bundle (docs/app-modules.md § The render bundle): a lazy engine's render set
// as one file, so a client fetches one file before its first frame.
//
//   u32 little-endian n, then n bytes of JSON { files: [{ path, sha256, offset, bytes }] },
//   then the files' bytes, each at its offset from the end of the JSON
//
// The index's SHA-256s are what the engine pins; a host still hashes every file it serves.
const encoder = new TextEncoder();
const decoder = new TextDecoder();
/** [{ path, sha256, bytes: Uint8Array }] -> the bundle's bytes. */
function pack(files) {
    let offset = 0;
    const index = files.map((f) => { const e = { path: f.path, sha256: f.sha256, offset, bytes: f.bytes.length }; offset += f.bytes.length; return e; });
    const head = encoder.encode(JSON.stringify({ files: index }));
    const out = new Uint8Array(4 + head.length + offset);
    new DataView(out.buffer).setUint32(0, head.length, true);
    out.set(head, 4);
    files.forEach((f, i) => out.set(f.bytes, 4 + head.length + index[i].offset));
    return out;
}
/** The bundle's bytes -> Map(path -> { sha256, bytes }); the bytes are views, not copies. */
function unpack(data) {
    const u8 = data instanceof Uint8Array ? data : new Uint8Array(data);
    const n = new DataView(u8.buffer, u8.byteOffset, u8.byteLength).getUint32(0, true);
    const { files } = JSON.parse(decoder.decode(u8.subarray(4, 4 + n)));
    const base = 4 + n;
    return new Map(files.map((f) => [f.path, { sha256: f.sha256, bytes: u8.subarray(base + f.offset, base + f.offset + f.bytes) }]));
}

/** `shared`: where the files every platform's engine fetches alike are, if not in `dir`
 * (by default `../shared`, the npm package's layout, when it is there). `bundle`: the
 * render bundle's bytes, or a function that returns them (called once, the first time
 * `read` finds no file); by default `render.bin` in `dir`, if there. */
function createModuleHost({
    dir = __dirname,
    shared = fs.existsSync(path.join(dir, '..', 'shared')) ? path.join(dir, '..', 'shared') : dir,
    read = (file) => {
        try { return fs.readFileSync(path.join(dir, file)); } catch (e) { if (e.code !== 'ENOENT' || shared === dir) throw e; }
        return fs.readFileSync(path.join(shared, file));
    },
    bundle = () => fs.readFileSync(path.join(dir, 'render.bin')),
    onLoad = null,
} = {}) {
    let render = null; // path -> { sha256, bytes }, once read
    function inBundle(file) {
        if (!render) {
            let bytes = null;
            try { bytes = typeof bundle === 'function' ? bundle() : bundle; } catch (e) { if (e.code !== 'ENOENT') throw e; }
            render = bytes ? unpack(bytes) : new Map();
        }
        return render.get(file)?.bytes;
    }
    const compiled = new Map(); // name -> { sha256, module, idle: [exports] }
    const loads = [];
    const linked = new Map(); // code module name -> the engine's table its functions are in
    const sha256 = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');
    function fetch(file, want) {
        // The render set is not beside the engine as single files: a file that is not
        // there is looked for in the bundle (read once, so a client that never draws never reads it).
        let bytes;
        try { bytes = read(file); } catch (e) { bytes = inBundle(file); if (!bytes) throw e; }
        const got = sha256(bytes);
        if (got !== want) throw new Error(`${file} is ${got}; the engine asked for ${want}`);
        return bytes;
    }
    function compile(name, want) {
        const held = compiled.get(name);
        if (held && held.sha256 === want) return held;
        const started = performance.now();
        const bytes = fetch(`modules/${name}.wasm`, want);
        const fetched = performance.now();
        const made = { sha256: want, module: new WebAssembly.Module(bytes), idle: [] };
        const load = { kind: 'module', name, sha256: want, bytes: bytes.length, fetchMs: fetched - started, instantiateMs: performance.now() - fetched };
        compiled.set(name, made);
        loads.push(load);
        onLoad?.(load);
        return made;
    }
    function instantiate(made) {
        const self = {};
        const imports = {
            cw: {
                // A question to the core: the answer goes into a buffer of the module's,
                // its length first, which the module frees.
                core(ptr, len) {
                    const request = new Uint8Array(self.exports.memory.buffer, ptr, len).slice();
                    const reply = api.core(request);
                    const at = self.exports.cw_alloc(reply.length + 4);
                    new DataView(self.exports.memory.buffer).setUint32(at, reply.length, true);
                    new Uint8Array(self.exports.memory.buffer, at + 4, reply.length).set(reply);
                    return at;
                },
            },
        };
        self.exports = new WebAssembly.Instance(made.module, imports).exports;
        return self.exports;
    }
    const api = {
        /** One request to module `name`; the reply's bytes. A call made while another
         * call of the same module is still running (a program that starts another) runs
         * in an instance of its own. */
        call(name, want, request) {
            const made = compile(name, want);
            const exports = made.idle.pop() ?? instantiate(made);
            // A trapped instance may hold a broken heap: it never returns to `idle`.
            const at = exports.cw_alloc(request.length);
            new Uint8Array(exports.memory.buffer, at, request.length).set(request);
            const slot = exports.cw_call(at, request.length);
            const view = new DataView(exports.memory.buffer);
            const ptr = view.getUint32(slot, true), len = view.getUint32(slot + 4, true);
            const reply = new Uint8Array(exports.memory.buffer, ptr, len).slice();
            exports.cw_free(ptr, len);
            made.idle.push(exports);
            return reply;
        },
        /** Link code module `name` into the engine instance whose exports are `engine`
         * (its glue asks, the first time one of the module's functions runs); the table
         * the module's functions are now in. */
        link(name, want, engine) {
            const held = linked.get(name);
            if (held) return held;
            const started = performance.now();
            const bytes = fetch(`modules/${name}.wasm`, want);
            const fetched = performance.now();
            const module = new WebAssembly.Module(bytes);
            const table = engine[WebAssembly.Module.imports(module).find((i) => i.kind === 'table').name];
            new WebAssembly.Instance(module, { engine });
            linked.set(name, table);
            const load = { kind: 'code', name, sha256: want, bytes: bytes.length, fetchMs: fetched - started, instantiateMs: performance.now() - fetched };
            loads.push(load);
            onLoad?.(load);
            return table;
        },
        /** One on-demand font file. */
        font(file, want) {
            return this.file('font', `fonts/${file}`, file, want);
        },
        /** One on-demand data file (the built-in internet's world). */
        data(file, want) {
            return this.file('data', `data/${file}`, file, want);
        },
        /** One image the renderer draws with (an icon, a wallpaper, a symbol), asked by
         * the first frame that needs it. */
        asset(file, want) {
            return this.file('asset', `assets/${file}`, file, want);
        },
        file(kind, at, name, want) {
            const started = performance.now();
            const bytes = fetch(at, want);
            const load = { kind, name, sha256: want, bytes: bytes.length, fetchMs: performance.now() - started };
            loads.push(load);
            onLoad?.(load);
            return bytes;
        },
        /** Every module, font, data file and image this host has loaded, in order, with the time each took. */
        loads: () => loads.slice(),
        /** Set by the engine's `setModuleHost`. */
        core: () => { throw new Error('this host was not given to an engine (setModuleHost)'); },
    };
    return api;
}
module.exports = { createModuleHost, pack, unpack };
