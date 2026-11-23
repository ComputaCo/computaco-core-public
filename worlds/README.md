# Worlds

A world is data, not code: one JSON definition the loader consumes, with whatever
directories of seed content its blueprint copies in. The kernel holds no defaults from
any of them, which is why there is more than one here. The one thing a world gets
without declaring it is the [internet](internet), and a world can decline that.

| World | Size | What it is for |
|---|---|---|
| [`internet`](internet) | 91 public services and the backbone | Not a world to run on its own: the internet every other world joins unless it sets `internet: false`. |
| [`company-2026`](company-2026) | 5 machines, 3 OS families, 17 services of its own | The reference ecosystem, on the internet. Compiled into the library, and what the site, the examples and most tests run against. |
| [`robotics-co`](robotics-co) | 9 machines, 3 services of its own, on the internet | Halyard Robotics: engineers, purchasing, sales, marketing, a contract manufacturer and a customer, each on their own device, with fifteen verified procedures and the `rover-launch` society spec. |
| [`lantern-media`](lantern-media) | 5 machines, 1 service of its own, on the internet | Lantern Media: a video editor, a producer, a reporter, a managing editor and outside counsel, with a YouTube channel, a Buzzsprout podcast and a news site behind a CDN; three verified media procedures. |
| [`browser`](browser) | 1 machine, a start page, no internet | One Linux machine whose whole screen is a web browser, with no desktop around it, and a gateway that admits any real host on ports 80 and 443: the world a hosted browser session runs in. |
| [`agent-desktop`](agent-desktop) | 1 machine, 1 intranet wiki | One desktop an agent can use. The blueprint worked example, and the fast half of the determinism corpus. |
| [`pro-workstation`](pro-workstation) | 1 machine, 1 intranet wiki, no internet | One Ubuntu workstation with the professional applications (FreeCAD, KiCad, LibreOffice, GIMP, Kdenlive, VS Code and more) and 18 verified [expert procedures](../docs/expert-procedures.md) in CAD, EDA, media and office work. |
| [`oss-web`](oss-web) | 1 machine, 6 sites of its own | Real open-source web apps (TodoMVC, RealWorld's Conduit and API, react-admin, JSON Server) as public sites, built by their own toolchains; needs `computerworld`'s `oss-web` feature. See [docs/oss-webapps.md](../docs/oss-webapps.md). |
| [`unrelated-lab`](unrelated-lab) | 1 machine, no network | Proof that nothing reference-world is baked into the kernel, and the smallest world to copy. |

Each holds its own `world.json`. Where a world has a `world.yml` beside it, the JSON is
generated from that blueprint and is not edited by hand; `scripts/build-content.sh`
rebuilds every one of them, and `cargo test -p cw-blueprint` fails if a checked-in
`world.json` has drifted from its blueprint.

To write your own, start from [writing a world of your own](../docs/custom-world.md) and
the [world schema](../docs/world-schema.md).
