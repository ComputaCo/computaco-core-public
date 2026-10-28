// The module host of a lazy build (docs/app-modules.md), for a page or a worker: the
// engine asks it for an application module or a font by name and SHA-256 and waits for
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
async function digest(bytes) {
    const hash = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
    return Array.from(hash, (b) => b.toString(16).padStart(2, '0')).join('');
}
export function createModuleHost({ base, sync = false } = {}) {
    const files = new Map(); // path -> { sha256, bytes }
    const instances = new Map();
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
    function instance(name, want) {
        const held = instances.get(name);
        if (held && held.sha256 === want) return held.exports;
        const exports = new WebAssembly.Instance(new WebAssembly.Module(bytesOf(`modules/${name}.wasm`, want)), {}).exports;
        instances.set(name, { sha256: want, exports });
        return exports;
    }
    return {
        async preload(manifest, names = null) {
            const wanted = [
                ...manifest.modules.filter((m) => !names || names.includes(m.name)),
                ...manifest.fonts.filter((f) => !names || names.includes(f.file)),
            ];
            await Promise.all(wanted.map(async (entry) => {
                const bytes = new Uint8Array(await (await fetch(new URL(entry.path, base))).arrayBuffer());
                const got = await digest(bytes);
                if (got !== entry.sha256) throw new Error(`${entry.path} is ${got}; the engine asked for ${entry.sha256}`);
                files.set(entry.path, { sha256: got, bytes });
            }));
        },
        call(name, want, request) {
            const exports = instance(name, want);
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
                instances.delete(name);
                throw e;
            }
        },
        font(file, want) {
            return bytesOf(`fonts/${file}`, want);
        },
    };
}
