# Workspace map

Package names are stable; the directories group related implementations. Use
`cargo test -p <package>` to work on one package without depending on its location.
The [runtime architecture](../docs/architecture.md) explains how they fit together.

| Directory | Responsibility | Packages |
| --- | --- | --- |
| [core/](core/) | Contracts, scheduling, actor sessions, recording, state encoding, lazily fetched modules and world construction | `cw-protocol`, `cw-determinism`, `cw-encode`, `cw-sdk`, `cw-kernel`, `cw-environment`, `cw-trajectory`, `cw-evaluation`, `cw-modules`, `cw-blueprint`, `cw-packages`, `cw-snapshot` |
| [machines/](machines/) | Filesystems, processes, shell, network and explicit host adapters | `cw-computer`, `cw-network`, `cw-host-adapters` |
| [languages/](languages/) | Embedded interpreters and deterministic runtime utilities | `cw-pyvm`, `cw-jsvm`, `cw-script-host`, `cw-regex`, `cw-tz`, `cw-zlib` |
| [web/](web/) | HTML/CSS/DOM engine, browser shell, and React-syntax apps without a JS VM | `cw-web` in `engine/`, `cw-browser` in `browser/`, `cw-tsx` and `cw-ui` in `tsx/` and `ui/` |
| [graphics/](graphics/) | Scene descriptions, rendering, artwork and maps | `cw-scene`, `cw-render`, `cw-artwork`, `cw-map` |
| [engines/](engines/) | Application domain engines | `cw-raster`, `cw-sql` |
| [applications/](applications/) | Native applications and desktop presentation, and the registry of applications other crates carry (`apps::external`) | `cw-applications` |
| [services/](services/) | Synthetic internet services, their shared runtime, and the registry other crates extend (`extend`) | `cw-services`, `cw-service-*`; the node-app packages are data under `packages/node/` (docs/app-packages.md) |
| [internet/](internet/) | The built-in internet every world joins unless it opts out | `cw-internet` |
| [bindings/](bindings/) | Thin Python and WebAssembly interfaces, and the modules a lazy Wasm build fetches on first use | `cw-python`, `cw-wasm`, `cw-app-module`, `cw-py-module` |
| [computerworld/](computerworld/) | Public Rust facade; its reference world is the agent desktop | `computerworld` |

Tests and fixtures stay with their owning package. Shared language examples are
in [examples/](../examples/README.md) and input worlds in [worlds/](../worlds/README.md).

Choose an existing package and module when the responsibility fits. Add a crate
for a meaningful dependency/API boundary, not merely to shorten a file. Group
directories contain no manifests of their own. The root workspace lists all
package levels explicitly, and `scripts/checks/check-boundaries.py` recursively
checks simulation packages while allowing only the named host/binding edges.
