# JavaScript and browser Wasm

Node and browser clients run the same canonical Rust runtime compiled to Wasm.
They receive generated JavaScript classes and ordinary objects, not raw-memory
management APIs. There is no separate JavaScript simulator.

## Install from npm

```sh
npm install computerworld@0.3.0
```

One package serves both: Node resolves it to the CommonJS binding, which loads the Wasm
beside it, and bundlers and browsers to the ES module, whose default export
initializes it.

```js
// Node (require, or import from an ES module)
const { World, engineVersion } = require('computerworld');

// Browser, through a bundler or an import map
import init, { World } from 'computerworld';
await init();
```

The package holds the Wasm once, the TypeScript declarations for each entry, the
reference world (`computerworld/worlds/company-2026/world.json`), the font pack and the
third-party notices. 0.3.0 is the last public release of the engine and stays under the
MIT License; newer engines run behind the [hosted API](cloud-api.md), which the
[clients](cloud-clients.md) drive. A lazy build, which downloads the heavy applications and the web
fonts only when first needed, is described in [Lazy builds](app-modules.md); the package
is not built that way.

## Without a bundler

The npm tarball is the browser build as well: `npm pack computerworld@0.3.0` downloads
`computerworld-0.3.0.tgz`, whose `package/` directory holds the ES module
(`computerworld.js`), the CommonJS module (`computerworld.cjs`), `computerworld_bg.wasm`, the
TypeScript declarations, `release.json` with the version, `worlds/`, `fonts/` and
`notices/`. Keep the module, the `.wasm` file and the notices together when copying them into
an application. No Rust, wasm-bindgen or build step is needed.

```sh
npm pack computerworld@0.3.0 && tar -xzf computerworld-0.3.0.tgz
node -e "import('./package/computerworld.cjs').then(m => console.log(m.engineVersion()))"
# Expected: 0.3.0
```

For a browser, import the package's root module:

```js
import init, {World, engineVersion} from './package/computerworld.js';
await init();
console.log(engineVersion()); // 0.3.0
// Supply your own world definition, or load one from the package's worlds/.
const world = new World(definition, 7);
```

Serve these files through a static HTTP server rather than opening `file://`. The
server serves static assets only. After bootstrap the simulated episode needs no
network backend. APIs and checkpoint formats may change between 0.x releases;
do not mix wrapper/Wasm files from different versions.

The engine's source is not public. To run a newer engine than 0.3.0, use the
[hosted API](cloud-api.md).

## Computer input in a page

[Programmatic computer use](programmatic-computer-use.md) describes keyboard,
pointer drag/resize, app/window focus, scene transforms and Canvas output, in Node as well
as in a page. API errors throw; failed individual actions are
reported in `outcomes` even when `step` itself returns normally.

The [project site](https://computaco.ai/) is the same module in a
page: each screen is a `<canvas>` that forwards coordinates and keyboard events to
Rust and paints the frame Rust returns. The module is instantiated in a worker and the
canvas handed to it with `transferControlToOffscreen`, so the world and the rasteriser are
off the page's own thread. It does not implement an alternate desktop state machine.

Do not install a host-fetch adapter just to make synthetic domains work. A real
HTML/browser compatibility bridge is optional and separate.

## Ownership and lifetime

Pass restricted `Environment` handles to actors. Keep `World` in the trusted
harness for snapshots/reset/fork, `exportSnapshot()`/`importSnapshot(text)`,
`stateHash()`, inspection, topology edits and trajectory access. A fork reattaches
existing grants through `branch.session(env.id)`.

Use a decimal string for a seed beyond JavaScript's safe integer range. Other
serialized integral fields outside that range use `BigInt`; results may contain
`BigInt`, so plain `JSON.stringify` is not a general checkpoint serializer. Use
`world.exportSnapshot()` for lossless portable state. Input must consist of plain
objects/arrays and supported scalar values; functions, undefined and non-finite
numbers are rejected.

Wasm objects expose `free()`: release temporary frames, snapshots and completed
sessions/worlds in long-running consumers. `frame.rgba` returns a copied Uint8Array.
Methods are synchronous; use independent Web Workers/runtime instances for
parallel worlds or to keep large renders off your UI thread. Sharing one handle
across workers is not supported.

Owner-only `addComputer(computer, node, links)` and `removeComputer(id)` change the
canonical runtime; `definition()` reads its blueprint. See [owner lifecycle](agent-api.md#owner-controlled-device-lifecycle).

The package's own declarations are rendered as the [JavaScript API](api-javascript.html).
Pin the exact version: snapshots and state hashes name the engine that made them, and only
the same engine version imports an exported snapshot.
