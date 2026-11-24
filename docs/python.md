# Python binding

Python uses a PyO3 extension containing the canonical Rust runtime. No Node,
Chromium or simulation server is required. CPython 3.9+ is supported by the abi3
binding. A compatible prebuilt wheel needs no Rust toolchain.

## Install

```sh
python -m venv .venv
# Linux/macOS: source .venv/bin/activate
# Windows PowerShell: .venv\Scripts\Activate.ps1
python -m pip install computerworld==0.3.0 \
  --find-links https://github.com/ComputaCo/computerworld/releases/expanded_assets/engine-v0.3.0
python -c "import computerworld; print(computerworld.__version__, computerworld.engine_version)"
# Expected: 0.3.0 0.3.0
```

The [engine-v0.3.0 release](https://github.com/ComputaCo/computerworld/releases/tag/engine-v0.3.0) carries wheels for Linux x86-64 and arm64 (manylinux2014), macOS arm64 and x86-64,
and Windows x64, and no source distribution. 0.3.0 is the last public release of the
engine and stays under the MIT License; newer engines run behind the
[hosted API](cloud-api.md), which the [clients](cloud-clients.md) drive. Wheels
contain the runtime, not an implicit company world: supply your own definition (the
[world schema](world-schema.md) describes it). API and checkpoint compatibility may change between 0.x releases;
keep the exact version with your episode records.

The engine's source is not public; to run a newer engine than 0.3.0, use the
[hosted API](cloud-api.md).

## Threading

`World`, `Environment` and `Snapshot` may be passed to other threads and called from
them. A `World` and every `Environment` minted from it share one lock around the Rust
world, so calls from several threads are **serialized**, not concurrent: a call that
arrives while another is running waits for it. Nothing is silently dropped and nothing
races.

```python
import threading
frame = {}
t = threading.Thread(target=lambda: frame.update(env.render(320, 240)))
t.start(); t.join()          # works: the render happens on that thread
```

What threads do *not* buy you is parallel simulation. One `World` is one state machine,
and the order its actions land in is the order the threads took the lock — which is not
deterministic across runs. **Keep an episode on one thread**, or give each thread its own
`World` (`World(definition, seed)`, or `world.fork(world.snapshot())`, whose result is an
independent world). Threads are for keeping a UI responsive or for driving independent
worlds side by side, not for splitting one episode.

If a call panics inside the simulation, the lock is poisoned and every later call on that
world raises `RuntimeError` naming the situation. That is a deliberate change from
earlier alpha builds, where the objects were marked `unsendable` and a call from a second
thread **aborted the interpreter** instead of raising. Nothing here aborts the host
process. The binding's tests assert both halves: a call from a second thread succeeds, and a
poisoned world raises rather than dying.

`Environment` exposes `id`, `step`, `observe`, `scene(width=1024, height=768)` and
`render(width=1024, height=768)`. Scene access requires a `semantic.v1` or `pixels.v1`
observation grant. `scene` returns layout/interaction data without rasterization.
`render` returns `{"width": int, "height": int, "rgba": bytes}`. Use an image library
to encode those bytes as PNG; no host image library participates in simulation.

See [programmatic computer use](programmatic-computer-use.md) for desktop launch,
keyboard input, pointer dragging, transformed hit targets, PNG output and owner
checkpoint examples, a starting point for an agent integration.

`World` owns reset, snapshots, portable `export_snapshot()` (`bytes`)/`import_snapshot(data)`,
`definition()`, `trajectory()`, `state_hash()` and privileged `inspect()`. Retain it
in the harness; provide only the environment to actor code. `world.session(env.id)`
reattaches a restricted handle after fork/import. Python only converts values; it
contains no separate command, service or desktop semantics.

The owner also supports `world.add_computer(computer, node, links)` and
`world.remove_computer(id)`. These preserve other devices and service state.
See [device lifecycle and checkpoint rules](agent-api.md#owner-controlled-device-lifecycle).

Every member of both handles is listed in the [Python API](api-python.md). Pin the exact
version: snapshots and state hashes name the engine that made them, and only the same
engine version imports an exported snapshot.
