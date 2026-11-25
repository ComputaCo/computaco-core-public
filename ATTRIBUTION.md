# Attribution

The code in this repository is MIT (`LICENSE`). Some files are other projects' work, each under its own licence. This
file says where each family of them is and what its licence is. Share-alike and copyleft files stay under their own
licences: our MIT does not apply to them. A build that embeds them (the Wasm module, a native binary) carries their
notices.

## In the published crates

Each crate that bundles another project's files lists them, with source, author, licence and where the licence text
is, in its own `ATTRIBUTION.md`, and its `license` field names every licence it bundles:

| Crate | What it bundles | Licences |
|---|---|---|
| cw-render | FreeCAD, VS Code (codicons), KiCad, GIMP and Yaru icons; the Ubuntu default wallpaper; the DejaVu masters; our own symbols, icons and wallpapers | MIT, LGPL-2.1-or-later, CC-BY-4.0, CC-BY-SA-4.0, CC-BY-SA-3.0, GPL-3.0-or-later, CC0-1.0, Bitstream-Vera (`crates/graphics/render/ATTRIBUTION.md`) |
| cw-fonts | DejaVu, Noto, Inter, Open Sans, Roboto, Ubuntu and thirteen web families | MIT, OFL-1.1, Ubuntu-font-1.0, Bitstream-Vera (`crates/graphics/fonts/ATTRIBUTION.md`) |
| cw-font-pack-sc, -cjk, -emoji | Noto faces | OFL-1.1 (`ATTRIBUTION.md` in each) |
| cw-jsvm | locale data from the Unicode CLDR, through ICU | MIT, Unicode-3.0 (`crates/languages/jsvm/ATTRIBUTION.md`) |
| cw-tsx | a corpus of open-source React and TypeScript projects' sources (`corpus/src/<project>`), as its parity cases | MIT; each project's licence file is in its folder |
| cw-applications | React, vendored for its web applications | MIT (`crates/applications/web/vendor/react.LICENSE`) |
| cw-ui | clsx and zustand, as test fixtures (`tests/islands/packages`) | MIT (the licence file in each) |
| cw-tz | time zone rules derived from the IANA tz database | MIT; the tz data is in the public domain |

## In the repository only (tests and packages, not in a published crate)

| Files | Source | Licence | Licence text |
|---|---|---|---|
| `crates/web/engine/tests/wpt/**` | web-platform-tests | BSD-3-Clause | `crates/web/engine/tests/wpt/LICENSE.md` |
| `crates/web/engine/tests/html5lib/**`, `crates/web/engine/tests/css-parsing-tests/**` | html5lib-tests, css-parsing-tests | MIT | the `LICENSE` in each |
| `crates/web/engine/tests/parity/acid1*` | the W3C CSS1 test suite (test5526c, sec5526c.gif) | W3C Software and Document License | the header of `acid1.html` |
| `crates/web/engine/tests/parity/acid2/**` | the Web Standards Project's Acid2 | as its licence file | `crates/web/engine/tests/parity/acid2/LICENSE.md` |
| `crates/web/engine/tests/vendor/*` | jQuery, React, Preact, Vue, Svelte, Emotion, styled-components, Tailwind, Inter, TodoMVC, json-server | MIT; Inter OFL-1.1 | the `<name>.LICENSE*` beside each |
| `crates/web/engine/tests/parity/*.chromium.png`, `crates/web/engine/tests/framework-parity/*.chromium.png` | Chromium's renderings of this repository's own test pages (invented companies and content) and of the open-source applications above | the pages' own licences | |
| `packages/node/{conduit-react,conduit-vue,json-server,react-admin,todomvc}/**` | the RealWorld front ends, json-server, react-admin and TodoMVC, built unmodified | MIT | the licence file in each `public/` |
| `packages/node/realworld-api/**` | gothinkster's RealWorld Node/Express backend | **not recorded** (no licence file is in the folder; see below) | |

## Not settled

- `packages/node/realworld-api/public/images/{demo-avatar.png,smiley-cyrus.jpeg}` and the server they came with have no
  recorded licence. They are in no published crate.
