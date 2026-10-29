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
third-party notices. It is published by the release workflow from the
`computerworld-0.3.0.tgz` asset of the GitHub release, after checking it against that
release's `SHA256SUMS`. A lazy build, which downloads the heavy applications and the web
fonts only when first needed, is described in [Lazy builds](app-modules.md); the package
is not built that way.

## Install a pinned bundle

Download from [v0.3.0](https://github.com/ComputaCo/computerworld/releases/tag/v0.3.0)
and verify the accompanying `SHA256SUMS`. Choose:

- `computerworld-0.3.0-wasm-web.tar.gz`: browser ES module, Wasm and TypeScript declarations.
- `computerworld-0.3.0-wasm-node.tar.gz`: Node CommonJS module, Wasm, declarations and runnable Node demo.

Each archive has a top-level directory matching its filename without the archive
extension. Runtime bundles include `worlds/`, example code, `release.json` with
source/version metadata, and required notices. Keep the module, `.wasm` file and
notices together when copying them into an application. No Rust, wasm-bindgen or
npm installation is needed to use the downloaded bundle.

For Node:

```sh
tar -xzf computerworld-0.3.0-wasm-node.tar.gz
cd computerworld-0.3.0-wasm-node
node -e "console.log(require('./computerworld.js').engineVersion())"
# Expected: 0.3.0
node examples/javascript/computer-interaction.mjs --output ./demo-output
```

For a browser, unpack the web bundle and import its root module:

```js
import init, {World, engineVersion} from './computerworld.js';
await init();
console.log(engineVersion()); // 0.3.0
// Supply your own world definition, or load a bundled worlds/ JSON file.
const world = new World(definition, 7);
```

Serve these files through a static HTTP server rather than opening `file://`. The
server serves static assets only. After bootstrap the simulated episode needs no
network backend. APIs and checkpoint formats may change between alpha releases;
do not mix wrapper/Wasm files from different versions.

## Build and run from source

The engine's source is not public. From the engine checkout, `rustup target add
wasm32-unknown-unknown`, install the `wasm-bindgen-cli` version that `Cargo.lock` names,
then `bash scripts/build-wasm.sh` and `node scripts/checks/smoke-node.cjs`. Without the
source, use the 0.3.0 package above or the [hosted API](cloud-api.md).

## Computer input in a page

[Programmatic computer use](programmatic-computer-use.md) describes keyboard,
pointer drag/resize, app/window focus, scene transforms and Canvas output. The
[Node computer demo](../examples/javascript/computer-interaction.mjs) uses those
same APIs without a browser. API errors throw; failed individual actions are
reported in `outcomes` even when `step` itself returns normally.

The [project site](https://computaco.ai/) is the same module in a
page: each screen is a `<canvas>` that forwards coordinates and keyboard events to
Rust and paints the frame Rust returns. The module is instantiated in a worker and the
canvas handed to it with `transferControlToOffscreen`, so the world and the rasteriser are
off the page's own thread (`site/engine.js` drives the simulator, `site/worker.js` hosts it,
`site/live.js` does the download and the events). It does not implement an alternate
desktop state machine. Run it locally with

```sh
bash scripts/build-wasm.sh && cp -r pkg/web site/pkg
node scripts/site/serve-site.mjs 8000
```

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
Pinning, and what changes between 0.x releases, is [migration](migration.md).
