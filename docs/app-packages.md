# Application packages

Every application a simulated machine can install is a **package**: a manifest in the
application catalog, [`packages/catalog.json`](../packages/catalog.json), saying what real
product it imitates, how its code reaches the engine, and what each platform calls it.
The engine's dispatch, the shells' launchers and docks, the hosted API's choice of host and
the fidelity tooling all read this one table. Nothing else keeps a list of applications.

## Names

A package's **short id** is the application kind a world installs and a procedure
launches: `kicad`, `spreadsheet`, `files`. Its **qualified id** adds what it imitates as
suffixes, each correct and each ignorable, the way a casual label drops a model's
variant:

    <id>[-<version>][-<platform>][-<device>]
    kicad-8.0-ubuntu-desktop   kicad-8.0   kicad-windows   kicad

A world may write any of these in `installed_apps`; the engine resolves them to the same
package and the shell shows the platform's own product name (`KiCad 8.0` on Windows,
`Thunderbird Mail` for `mail` on Ubuntu, `Finder` for `files` on macOS). `application.v1
list` answers each application's `id` (short), `name` (casual) and `label` (what this
machine's shell calls it), so a client can be as formal or as casual as it likes.

## The manifest

| Field | Meaning |
|---|---|
| `id`, `name` | Short id and casual name |
| `runtime` | `engine` (files, browser, terminal, editor: part of the compositor), `native` (a Rust application compiled into the engine when its `feature` is on, else served from `module` as the application tagged `tag`), `web` (a TSX or script application), `node_app` (a published web application with a Node backend, from a `package` of files by hash) |
| `imitates` | The product, its vendor and the version the imitation targets |
| `platforms` | Per platform: the product `label`, its `version` there, and the `icon` to draw when another package's is reused. A platform absent here does not offer the package |
| `category`, `publisher`, `license` | Where it sits in a launcher and whose it is |
| `interactions` | Glob of the interaction ids its pages expose, which procedures aim at |
| `fidelity` | The fidelity tooling's data for it: the target map and home template `cwfid` uses to drive the real product |
| `product` | The professional product it is part of (§ Professional applications), when it is sold rather than included |

`shelves` lists, per platform, the applications the shell shows and in what order: the
dock, the application grid, the home screen pages.

## Professional applications

`products` is the price list of the professional applications: the engineering and
media-production suites, each with a one-time `price_cents` for a whole organization.

| Product | Packages | Price |
|---|---|---|
| `kicad` | `kicad` | $490 |
| `freecad` | `freecad` | $490 |
| `video` | `clipchamp`, `imovie`, `kdenlive`, `videoeditor` | $390 |

A package names its product in `product`; one purchase unlocks every package of the
product, on every platform, for every member of the organization that bought it. The hosted
API refuses a world that installs one the organization has not bought (`402
application_not_purchased`), and meters the steps of worlds that run one beyond the plan's
monthly allowance ([the hosted API](cloud-api.md) § Professional applications). The other
closed applications — the office suite, Visual Studio Code, Music, Maps — are `hosted_only`
in the open-core build and come with every plan of the hosted API. The local engine (the
Node and Python packages, the browser bundle) runs whatever modules it is given; the catalog's
`product` is a fact about the hosted service, not a lock in the engine.

## Where the code lives

The catalog names modules; the build pins them. `scripts/build-wasm.sh` builds one Wasm
module per module name the catalog lists and pins each module's SHA-256 into the engine,
and a step records which modules it ran so a replay refuses other bytes
(`the engine's design notes`). The hosted API places a world on the host that
carries the modules its machines' packages need.

A package installed at run time (`cw_packages::install`, with its module registered
through `cw_applications::apps::module::register_module`) joins the same table: the
engine serves it exactly as it serves the catalog's, and its hash is pinned the same way.
That is the door for an application that is not ours.

## Adding an application

1. Add its manifest to `packages/catalog.json`, with every platform's label, and put it on
   the shelves of the platforms that offer it.
2. For a native application, implement it in `crates/applications/src/apps/` and list its
   variant in `native_apps!`; the build checks that every compiled-in kind is in the
   catalog. Give it a feature and a module when it is heavy.
3. For the fidelity tooling, add `fidelity/targets/<id>.json` and, if the real product
   needs one, a home template; name them in the manifest.

`cargo test -p cw-packages` validates the catalog; `cargo test -p cw-applications` checks
the code and the catalog agree.

## Bringing your own

A package that is not ours goes through the same table. Locally, `computerworld::install_package`
(Python: `computerworld.install_package(manifest)`; JavaScript: `installPackage(manifestJson)`)
puts the manifest in the catalog and, for a native module, registers its name and hash so the
module host serves it; for a web package the script in `runtime.script` is defined as a web
application; for a `node_app` the archive is fetched by the pinned hash like ours. On the
hosted API, `POST /v1/packages` keeps a package with the account, and a world whose machines
install it or whose services name it gets it installed before it boots, and carries it through
export, import and fork. Workers cannot compile Wasm at run time, so the hosted API takes
`web` and `node_app` packages; a native Wasm module package runs in the local engine (the
Node and Python packages, the browser).

A package's manifest for upload is the catalog's shape with the code added:

```json
{"id": "my-crm", "name": "My CRM", "runtime": {"kind": "web", "script": "…", "style": "…", "react": true},
 "platforms": {"ubuntu": {"label": "My CRM"}, "macos": {"label": "My CRM"}}, "category": "business"}
```

```json
{"id": "my-site", "name": "My site", "runtime": {"kind": "node_app", "sha256": "<of the archive>"}}
```

The archive is what `scripts/pack-node-apps.mjs` writes for a directory with a
`manifest.json`, `public/` and, for a backend, `server/` (docs/oss-webapps.md § How they are
hosted). A world then installs `my-crm` on a machine, or names `my-site` in a `node-app`
service's `initial_state.package`, exactly as it would one of ours.

