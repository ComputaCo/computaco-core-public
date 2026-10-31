# ComputerWorld

A deterministic simulated computer: operating-system shells, a browser with its own HTML,
CSS and JavaScript engine, Python and Node runtimes, a synthetic internet, and a catalog of
applications, every step of which replays to the byte. Agents drive it through pointer and
keyboard or through typed actions, and every observation comes from state the evaluator
can read.

This is the open core. It carries the engine, the SDK and module ABI, the application
catalog with the base applications, the fidelity tooling (`fidelity/`) that measures the
engine against the real product, the reference worlds that need only open parts, and the
clients. The closed applications (KiCad, FreeCAD, the office suite, Visual Studio Code, the
video editors, Music, Maps) are packages served by the hosted API at <https://computaco.ai>:
the office suite, Visual Studio Code, Music and Maps with every plan, and the engineering
and media-production suites bought once per organization (`products` in
`packages/catalog.json`). Here they are marked `hosted_only`, and a world that installs one
is refused at boot with that reason. The synthetic internet in this tree is the generic
subset (search, wiki, mail, calendar, docs, drive, chat, messages, forum, an assistant and
static sites); the look-alike services of named products stay with the hosted service.

- [ComputaCo/computerworld](https://github.com/ComputaCo/computerworld): the Python and
  JavaScript clients of the hosted API (also under `clients/` here) and the engine's release
  assets, installable with pip and npm from its `engine-v0.3.0` release.
- `docs/app-packages.md`: the application catalog and how to bring your own.
- `fidelity/README.md`: capturing the real application and replaying it here.
- `crates/README.md`: the workspace.

Licensed under the MIT License (`LICENSE`).
