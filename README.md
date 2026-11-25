# ComputerWorld

A deterministic simulated computer: operating-system shells, a browser with its own HTML,
CSS and JavaScript engine, Python and Node runtimes, a synthetic internet, and a catalog of
applications, every step of which replays to the byte. Agents drive it through pointer and
keyboard or through typed actions, and every observation comes from state the evaluator
can read.

This is the open core, and the place it is written. It carries the engine, the SDK and
module ABI, the application catalog with the base applications, the generic services of
the synthetic internet (search, wiki, mail, calendar, docs, drive, chat, messages, forum,
an assistant, static sites and node apps), the bindings, and the reference worlds that
need only open parts. Applications and services of other crates join it at run time:
`cw_applications::apps::external` registers native applications, and `cw_services::extend`
adds service kinds. The professional applications of the catalog (KiCad, FreeCAD, the
office suite, Visual Studio Code, the video editors, Music, Maps) and the look-alike
services of named products are served by the hosted API at <https://computaco.ai>; a world
that installs one of those applications is refused at boot with that reason here.

- [ComputaCo/computerworld](https://github.com/ComputaCo/computerworld): the Python and
  JavaScript clients of the hosted API, and the engine's release assets.
- `docs/app-packages.md`: the application catalog and how to bring your own.
- `crates/README.md`: the workspace.

Build with `cargo build`, test with `cargo test -p computerworld`, and build the Wasm
bundle with `bash scripts/build-wasm.sh`. The crates are published on crates.io.

Licensed under the MIT License (`LICENSE`).
