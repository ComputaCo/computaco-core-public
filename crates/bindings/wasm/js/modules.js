// The module host of a lazy build (docs/app-modules.md), for a page or a worker: the
// engine asks it for a module, a font or a data file by name and SHA-256 and waits for
// the answer, so the bytes must be here when it asks. `preload(manifest)` fetches what
// the engine's `lazyManifest()` lists (all of it, or the names given) ahead of use; a
// worker may instead pass `sync: true`, which fetches on first use with a synchronous
// request (allowed in workers, not on a page's main thread). What a frame draws with
// (the set `render`) comes in one file, the render bundle `render.bin`
// (docs/app-modules.md § The render bundle): `preload(manifest, ['render'])` fetches it
// once, and a sync host fetches it the first time a file it asks for is not served alone.
//
//   import init, * as engine from './computerworld.js';
//   import { createModuleHost } from './modules.js';
//   await init();
//   const host = createModuleHost({ base: new URL('./', import.meta.url) });
//
// `base` is where the engine's own files are (its code modules and `render.bin`);
// `shared` (by default `base`) where the files every platform's engine fetches alike are:
// the application modules, Python, fonts and data (in the npm package, `lazy/shared/`).
//   await host.preload(engine.lazyManifest(), ['kicad']);
//   engine.setModuleHost(host);
//
// `setModuleHost` gives the host `core(request) -> reply`: a module asks the core
// through its import `cw.core` while one of its calls runs (text tables, a program's
// files, network and processes), and the host passes the question on. An engine split
// into code modules asks `link` for one the first time one of its functions runs.
// The render bundle: u32 little-endian n, then n bytes of JSON
// { files: [{ path, sha256, offset, bytes }] }, then the files' bytes, each at its offset
// from the end of the JSON. The same format as `modules.cjs`'s `pack` and `unpack`.
/** The bundle's bytes -> Map(path -> { sha256, bytes }); the bytes are views, not copies. */
export function unpack(data) {
    const u8 = data instanceof Uint8Array ? data : new Uint8Array(data);
    const n = new DataView(u8.buffer, u8.byteOffset, u8.byteLength).getUint32(0, true);
    const { files } = JSON.parse(new TextDecoder().decode(u8.subarray(4, 4 + n)));
    const base = 4 + n;
    return new Map(files.map((f) => [f.path, { sha256: f.sha256, bytes: u8.subarray(base + f.offset, base + f.offset + f.bytes) }]));
}
async function digest(bytes) {
    const hash = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
    return Array.from(hash, (b) => b.toString(16).padStart(2, '0')).join('');
}
export function createModuleHost({ base, shared = base, sync = false } = {}) {
    const files = new Map(); // path -> { sha256, bytes }
    const compiled = new Map(); // name -> { sha256, module, idle: [exports] }
    const linked = new Map(); // code module name -> the engine's table its functions are in
    let bundled = false; // whether a sync host has fetched the render bundle
    function syncFetch(file, from) {
        const request = new XMLHttpRequest();
        request.open('GET', new URL(file, from), false);
        request.responseType = 'arraybuffer';
        request.send();
        if (request.status !== 200) return null;
        return new Uint8Array(request.response);
    }
    function bytesOf(file, want, own) {
        const held = files.get(file);
        if (held && held.sha256 === want) return held.bytes;
        if (!sync) throw new Error(`${file} was not preloaded`);
        // A synchronous fetch cannot hash with SubtleCrypto: the module's own SHA-256
        // is checked by `preload`, and a sync host trusts the origin it was given (and
        // the bundle's index for what it holds).
        let bytes = syncFetch(file, own ? base : shared);
        if (!bytes && !bundled) {
            bundled = true;
            const bundle = syncFetch('render.bin', base);
            if (bundle) for (const [at, entry] of unpack(bundle)) files.set(at, entry);
            const inside = files.get(file);
            if (inside && inside.sha256 === want) bytes = inside.bytes;
        }
        if (!bytes) throw new Error(`${file} could not be fetched`);
        return bytes;
    }
    function compile(name, want) {
        const held = compiled.get(name);
        if (held && held.sha256 === want) return held;
        const made = { sha256: want, module: new WebAssembly.Module(bytesOf(`modules/${name}.wasm`, want, false)), idle: [] };
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
            // A name is a module's, a file's, or a set's: `render` is every file a
            // frame draws with, which a client fetches before its first frame.
            const named = (entry, name) => !names || names.includes(name) || names.includes(entry.set);
            const wanted = [
                ...manifest.modules.filter((m) => named(m, m.name)),
                ...manifest.fonts.filter((f) => named(f, f.file)),
                ...(manifest.data ?? []).filter((d) => named(d, d.file)),
                ...(manifest.assets ?? []).filter((a) => named(a, a.file)),
            ];
            // The render set is one file, fetched once; every other file alone.
            const bundled = wanted.filter((entry) => entry.set === 'render');
            const alone = wanted.filter((entry) => entry.set !== 'render');
            const check = async (entry, bytes) => {
                const got = await digest(bytes);
                if (got !== entry.sha256) throw new Error(`${entry.path} is ${got}; the engine asked for ${entry.sha256}`);
                files.set(entry.path, { sha256: got, bytes });
            };
            const get = async (file, from) => {
                const response = await fetch(new URL(file, from));
                if (!response.ok) throw new Error(`${file}: ${response.status}`);
                return new Uint8Array(await response.arrayBuffer());
            };
            await Promise.all([
                ...alone.map(async (entry) => check(entry, await get(entry.path, entry.kind === 'code' ? base : shared))),
                (async () => {
                    if (!bundled.length) return;
                    const bundle = unpack(await get('render.bin', base));
                    await Promise.all(bundled.map((entry) => {
                        const inside = bundle.get(entry.path);
                        if (!inside) throw new Error(`render.bin holds no ${entry.path}`);
                        return check(entry, inside.bytes);
                    }));
                })(),
            ]);
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
            const module = new WebAssembly.Module(bytesOf(`modules/${name}.wasm`, want, true));
            const table = engine[WebAssembly.Module.imports(module).find((i) => i.kind === 'table').name];
            new WebAssembly.Instance(module, { engine });
            linked.set(name, table);
            return table;
        },
        font(file, want) {
            return bytesOf(`fonts/${file}`, want, false);
        },
        data(file, want) {
            return bytesOf(`data/${file}`, want, false);
        },
        asset(file, want) {
            return bytesOf(`assets/${file}`, want, true);
        },
        core: () => { throw new Error('this host was not given to an engine (setModuleHost)'); },
    };
    return api;
}
