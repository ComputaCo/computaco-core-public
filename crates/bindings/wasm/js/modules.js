// The module host of a lazy build (docs/app-modules.md), for a page or a worker: the
// engine asks it for a module, a font or a data file by name and SHA-256 and waits for
// the answer, so the bytes must be here when it asks. `preload(manifest)` fetches what
// the engine's `lazyManifest()` lists (all of it, or the names given) ahead of use; a
// worker may instead pass `sync: true`, which fetches on first use with a synchronous
// request (allowed in workers, not on a page's main thread).
//
//   import init, * as engine from './computerworld.js';
//   import { createModuleHost } from './modules.js';
//   await init();
//   const host = createModuleHost({ base: new URL('./', import.meta.url) });
//   await host.preload(engine.lazyManifest(), ['kicad']);
//   engine.setModuleHost(host);
//
// `setModuleHost` gives the host `core(request) -> reply`: a module asks the core
// through its import `cw.core` while one of its calls runs (text tables, a program's
// files, network and processes), and the host passes the question on. An engine split
// into code modules asks `link` for one the first time one of its functions runs.
async function digest(bytes) {
    const hash = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
    return Array.from(hash, (b) => b.toString(16).padStart(2, '0')).join('');
}
export function createModuleHost({ base, sync = false } = {}) {
    const files = new Map(); // path -> { sha256, bytes }
    const compiled = new Map(); // name -> { sha256, module, idle: [exports] }
    const linked = new Map(); // code module name -> the engine's table its functions are in
    function syncFetch(file) {
        const request = new XMLHttpRequest();
        request.open('GET', new URL(file, base), false);
        request.responseType = 'arraybuffer';
        request.send();
        if (request.status !== 200) throw new Error(`${file}: ${request.status}`);
        return new Uint8Array(request.response);
    }
    function bytesOf(file, want) {
        const held = files.get(file);
        if (held && held.sha256 === want) return held.bytes;
        // A synchronous fetch cannot hash with SubtleCrypto: the module's own SHA-256
        // is checked by `preload`, and a sync host trusts the origin it was given.
        if (sync) return syncFetch(file);
        throw new Error(`${file} was not preloaded`);
    }
    function compile(name, want) {
        const held = compiled.get(name);
        if (held && held.sha256 === want) return held;
        const made = { sha256: want, module: new WebAssembly.Module(bytesOf(`modules/${name}.wasm`, want)), idle: [] };
        compiled.set(name, made);
        return made;
    }
    function instantiate(made) {
        const self = {};
        const imports = {
            cw: {
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
        async preload(manifest, names = null) {
            const wanted = [
                ...manifest.modules.filter((m) => !names || names.includes(m.name)),
                ...manifest.fonts.filter((f) => !names || names.includes(f.file)),
                ...(manifest.data ?? []).filter((d) => !names || names.includes(d.file)),
            ];
            await Promise.all(wanted.map(async (entry) => {
                const bytes = new Uint8Array(await (await fetch(new URL(entry.path, base))).arrayBuffer());
                const got = await digest(bytes);
                if (got !== entry.sha256) throw new Error(`${entry.path} is ${got}; the engine asked for ${entry.sha256}`);
                files.set(entry.path, { sha256: got, bytes });
            }));
        },
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
        link(name, want, engine) {
            const held = linked.get(name);
            if (held) return held;
            const module = new WebAssembly.Module(bytesOf(`modules/${name}.wasm`, want));
            const table = engine[WebAssembly.Module.imports(module).find((i) => i.kind === 'table').name];
            new WebAssembly.Instance(module, { engine });
            linked.set(name, table);
            return table;
        },
        font(file, want) {
            return bytesOf(`fonts/${file}`, want);
        },
        data(file, want) {
            return bytesOf(`data/${file}`, want);
        },
        core: () => { throw new Error('this host was not given to an engine (setModuleHost)'); },
    };
    return api;
}
