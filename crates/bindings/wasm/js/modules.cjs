'use strict';
// The module host of a lazy build (docs/app-modules.md), for Node: the engine asks it
// for an application module or a font by name and SHA-256 and waits for the answer.
// Files are read beside the engine (`modules/<name>.wasm`, `fonts/<file>`), or through
// `read(path)` (a cache filled from a server, a bundle); bytes that do not hash to what
// the engine asked for are refused, so a run always loads the code it recorded.
//
//   const engine = require('computerworld');
//   const { createModuleHost } = require('computerworld/modules.cjs');
//   engine.setModuleHost(createModuleHost());
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

function createModuleHost({ dir = __dirname, read = (file) => fs.readFileSync(path.join(dir, file)), onLoad = null } = {}) {
    const instances = new Map();
    const loads = [];
    const sha256 = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');
    function fetch(file, want) {
        const bytes = read(file);
        const got = sha256(bytes);
        if (got !== want) throw new Error(`${file} is ${got}; the engine asked for ${want}`);
        return bytes;
    }
    function instance(name, want) {
        const held = instances.get(name);
        if (held && held.sha256 === want) return held;
        const started = performance.now();
        const bytes = fetch(`modules/${name}.wasm`, want);
        const fetched = performance.now();
        const exports = new WebAssembly.Instance(new WebAssembly.Module(bytes), {}).exports;
        const load = { kind: 'module', name, sha256: want, bytes: bytes.length, fetchMs: fetched - started, instantiateMs: performance.now() - fetched };
        loads.push(load);
        onLoad?.(load);
        const made = { sha256: want, exports };
        instances.set(name, made);
        return made;
    }
    return {
        /** One request to application module `name`; the reply's bytes. */
        call(name, want, request) {
            const { exports } = instance(name, want);
            try {
                const at = exports.cw_alloc(request.length);
                new Uint8Array(exports.memory.buffer, at, request.length).set(request);
                const slot = exports.cw_call(at, request.length);
                const view = new DataView(exports.memory.buffer);
                const ptr = view.getUint32(slot, true), len = view.getUint32(slot + 4, true);
                const reply = new Uint8Array(exports.memory.buffer, ptr, len).slice();
                exports.cw_free(ptr, len);
                return reply;
            }
            catch (e) {
                // A trapped instance may hold a broken heap: the next call starts a fresh one.
                instances.delete(name);
                throw e;
            }
        },
        /** One on-demand font file. */
        font(file, want) {
            const started = performance.now();
            const bytes = fetch(`fonts/${file}`, want);
            const load = { kind: 'font', name: file, sha256: want, bytes: bytes.length, fetchMs: performance.now() - started };
            loads.push(load);
            onLoad?.(load);
            return bytes;
        },
        /** Every module and font this host has loaded, in order, with the time each took. */
        loads: () => loads.slice(),
    };
}
module.exports = { createModuleHost };
