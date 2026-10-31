# Fidelity: measuring ComputerWorld against the real thing

`fidelity/` is the tooling that compares a real operating system and application against
their ComputerWorld reimplementation, step for step, with no model in the loop. It is
built for the mass-conversion loop: capture a reference trajectory once, replay its
stimuli in the simulator forever, and let traditional algorithms say exactly where and
by how much the two diverge, so the long tail of fidelity work is pointed at the right
widget rather than at a screenshot.

Two crates:

- [`core/`](core/) — `cw-fidelity`, pure. The trace ontology, normalization, tree diff,
  pixel metrics, the divergence cascade, coverage and threshold tuning.
- [`tool/`](tool/) — `cw-fidelity-tool`, the `cwfid` binary. Everything that touches the
  host: Xvfb, D-Bus, AT-SPI, xdotool, xwd, the store on disk, and ComputerWorld itself.

Build and test under the shared cargo lock (CLAUDE.md):

    flock ~/.claude/jobs/cargo.lock cargo test -p cw-fidelity -p cw-fidelity-tool
    flock ~/.claude/jobs/cargo.lock cargo build -p cw-fidelity-tool   # target/debug/cwfid

## The ontology (`core/src/schema.rs`)

Everything is defined on what both sides can *observe*, never on either side's internals.

| Entity | What it is |
|---|---|
| `Fingerprint` | The environment of a capture: OS family and version, application id and version, display size, DPI and scale, toolkit, theme, locale, a hash of the fonts available, a hash of the application's configuration. `class()` is the equivalence class two captures must share for a comparison to mean anything (application, major.minor version, display, theme, locale, config). |
| `Source` | How the trajectory was obtained: `atspi-xvfb` (this tool driving the real app), `computer-use` (Claude on a real VM, through a trace), `video`, `rrweb`, `simulator`. Carries the provenance string (a recording file, a session id). |
| `Stimulus` | The input: a pointer op at a point (move, down, up, click, double click, wheel), a drag, a key chord in the procedure spelling (`Ctrl+s`), typed text, or a wait. |
| `Target` | What the gesture was aimed at, as a person finds it: role, label, text, application, and a point in thousandths. Mirrors `computerworld::procedure::Target`. |
| `Resolution` | Where the target resolved on this side: node path, role, name, bounds, the point hit. |
| `SemTree` / `SemNode` | The screen's semantics: a `desktop` root, windows in stacking order, controls and text beneath. Roles are the canonical vocabulary in `normalize::ROLES`; AT-SPI, ARIA and the simulator's roles all map into it. |
| `FrameRef` | A frame in the content-addressed object store (PNG, keyed by the SHA-256 of its RGBA). |
| `Effects` | Side effects beyond the screen: files created, modified or deleted under the home directory (paths relative to it), the clipboard, the window titles. |
| `Settle` | How the screen came to rest: frames sampled until the content hash stopped changing, time waited, whether it did settle. |
| `Observation` | Semantic tree, its normalized hash, frame, effects, settle, time. |
| `Step` | Stimulus, target, resolution, whether it was delivered, and the observation after it. |
| `Trajectory` | Side (reference or simulated), app, fingerprint, source, the procedure and seed it came from, the initial observation and the steps. Its id is the first 16 hex of its content hash. |
| `Annotation` | A verdict on one divergence: `matters`, `cosmetic`, `ignore`, `ref_noise`, bound to a comparison, a step, optionally a finding, a node or a region. |
| `Tolerances` | The thresholds in force; `cwfid tune` fits them to annotations. |

A trajectory with only frames (a video) or only a tree (an rrweb recording before rendering)
is legal: the cascade runs the levels the data allows and says which it ran.

## Normalization (`core/src/normalize.rs`)

Both sides' trees become one comparable shape before any diff:

1. Roles map to the canonical vocabulary. AT-SPI's `text` is a text box when editable
   and static text otherwise; a checkable `push button` is a checkbox.
2. Names are cleaned: whitespace collapsed, GTK/Qt mnemonics (`_File`, `&Save`) dropped,
   trailing ellipses and label colons dropped.
3. Invisible, empty and off-screen nodes are pruned; scrollbars, tooltips and status bars
   are dropped by default (they are the platform's, not the application's).
4. Unnamed wrapper nodes (panels, fillers, viewports) are spliced out.
5. Window-manager controls (resize handles, move grips, the title bar's buttons), a control
   the size of its window, a control's own caption text, shadow copies of a label, and the
   desktop shell's own window (task bar, dock) are dropped; the shell is compared on its own.
6. The tree is **flattened**: windows under the desktop, and under each window its
   interactive controls and texts as one level, with menus, menu bars, tool bars and tab
   lists keeping their items. However deep a toolkit nests, both sides reach this shape.
7. Bounds become window-relative, and siblings are put in reading order (bands of 6 px).

`NormalizeOptions::faithful()` keeps everything for inspection.

## Tree diff (`core/src/treediff.rs`)

`diff` matches identical subtrees by content hash first, aligns the residue of each sibling
sequence by dynamic programming with a node cost that knows a renamed control from a
different one (role must agree; names within `name_similarity_min`), recurses into matched
pairs, reports out-of-order matches as moves (the siblings outside the longest increasing
subsequence), and pairs deleted and inserted subtrees with equal hashes as moves. It yields
an edit script with paths into both trees. `zhang_shasha` is the exact ordered tree edit
distance; the tests check `diff` never reports less than it.

## Pixels (`core/src/pixels.rs`)

Per widget crop, never whole-screenshot equality: `delta_e` (CIE76 in L\*a\*b\*: mean, p95,
max, fraction over the just-noticeable 2.3), `ssim` (8×8 windows over integral images),
`sobel_edges` + `chamfer` (shape agreement as the symmetric mean distance between edge maps
through a 3-4 chamfer distance transform), `dhash` for triage and `block_diff` to localize
a change when no node can.

## The cascade (`core/src/cascade.rs`)

Per step, cheapest first, each level only where the one before found a difference:

1. **Stimulus** — same input on both sides; both delivered it; the target resolved to the
   same role and name.
2. **Hash** — normalized trees equal in content? in bounds?
3. **Tree** — the edit script, each op a finding: `NodeMissing`, `NodeExtra`, `NodeMoved`,
   `NodeChanged` (name, value, state), `BoundsOff` (dx, dy, dw, dh, 1 − IoU), `WindowMissing`.
4. **Widgets** — for matched controls with frames, crop each at its own bounds and measure:
   `ColorOff`, `TextureOff`, `ShapeOff`; a differing `canvas` gets its region located.
5. **Frame** — when a side has no tree, or the trees agree and the pixels do not: the
   region that differs.
6. **Effects** — files, clipboard, settle.

Every finding carries a node path on each side, absolute rectangles on each side, the
metrics, and whether it is over tolerance. A step is *divergent* when any finding is. The
step's `FidelityVector` (semantic, layout, color, shape, texture, effects, overall) is what
aggregates and trends; a trajectory's is the mean over its steps.

## Coverage and tuning

`coverage` cells are (app, screen context, stimulus kind), where the context is the front
window's title before the stimulus; a cell lists the fingerprint classes its references
were captured in. Priorities:
no reference capture > reference never compared > diverging > covered, with diminishing
returns past twenty agreeing comparisons. `tune` joins annotations to the findings they
judge and, per metric, picks the cut point with the best Youden's J; `--apply` writes the
cuts with enough support into the store's `tolerances.json`.

## The store

    <store>/trajectories/<id>.json      semantic trees inline
    <store>/objects/<aa>/<sha256>.png   frames, content-addressed
    <store>/comparisons/<ref>__<sim>.json
    <store>/annotations/<id>.json
    <store>/tolerances.json
    <store>/index.json                   `cwfid index` or `cwfid ls`

`--store DIR` or `$CWFID_STORE`; the default is `./fidelity-store`. Plain JSON and PNG, so
a script or `jq` reads it as well as the tool does.

## `cwfid`

Run `cwfid help`. The shape of a session:

    S=/path/to/store
    # 1. Reference: KiCad in a private Xvfb, the procedure's gestures resolved through AT-SPI.
    cwfid capture-ref worlds/pro-workstation/procedures/eda-voltage-divider.json --app kicad --seed 3 --store $S
    # 2. Lockstep: the same stimuli into ComputerWorld, compared after each step.
    cwfid replay t_<ref> --store $S --text
    # 3. Look at one divergence: trees, frames, marked frames, side-by-side, heat map, crops.
    cwfid inspect t_<ref> t_<sim> --step 12 --out /tmp/step12 --store $S
    # 4. Say what mattered; fit the thresholds to what people said.
    cwfid annotate t_<ref> t_<sim> --step 12 --finding 3 --verdict matters --note "toolbar icon missing" --store $S
    cwfid tune --apply --store $S
    # 5. Where to capture next.
    cwfid coverage --text --store $S

Progress is JSON lines on stderr; results are JSON on stdout (`--text` for a rendering).
A procedure's first gestures usually open the application from the shell; on a reference
desktop the tool launches the application itself, and `--skip-steps N` records those
gestures as not applicable so step indices stay aligned with the simulation's.

`capture-ref` needs Xvfb, dbus-daemon, xdotool, xwd and Python 3 with GObject
introspection for AT-SPI (`python3-gi`, `gir1.2-atspi-2.0`); Openbox is used when
present. Both this machine and `gb10-direct` have them. The application's home is a
fresh sandbox; `--home-template DIR` seeds it (a KiCad configuration that skips the
first-run dialogs), and the sandbox is deleted unless `--keep-sandbox` or `--sandbox DIR`.

### Interaction ids on a real desktop

Procedures aim many gestures at the simulator's own interaction ids
(`*kicad:pm:new`, `*kicad:dlg:ok`). A real application has none, so
`--target-map fidelity/targets/<app>.json` says what each means there: a key chord
(`{"key": "Ctrl+N"}`), typed text, an ordered list of targets to try
(`{"targets": [{"role": "button", "label": "OK"}, {"role": "button", "label": "Save"}]}`),
`{"skip": true}`, or `{"sequence": [...]}` of those, each delivered and the screen re-read
before the next (a menu, then its item). Patterns are globs; `{n}` in a label is the n-th
`*` of the match, so `*kicad:dlg:pick:*:*` can name the tree item it picks. A target may
also name the owning `window` (a title glob) and how to `pick` among matches (`smallest`,
the default; `largest`, for a canvas; `first`). `_dismiss` lists dialogs the tool closes
whenever they appear (a plugin-update question, a settings-migration prompt). A click on
the simulator's title-bar buttons becomes a `window` stimulus, performed through the
window manager on the reference. The map is conversion data kept
beside the application and refined from `cwfid tree <id> --raw --text`, which prints the
real accessibility tree with every role, name and rectangle. `fidelity/targets/kicad.json`
is the start of KiCad's.

### Importing gold from elsewhere

Reference trajectories need not come from this tool's Xvfb. `cwfid import` reads a
trace directory as a trajectory: `trace.jsonl` with one stimulus per line and a PNG per
observation (how a computer-use session on a real Linux, macOS or iOS instance is
recorded), `cwfid import-video` samples a recording at the times a sidecar of events
gives, and `cwfid import-rrweb` turns an rrweb recording's DOM snapshots and
interaction events into semantic trees and stimuli. Each sets `source` accordingly; the
cascade runs the levels the data allows.

## For Claude

This is a tool Claude uses while converting applications. The intended loop: capture or
import gold, replay, read `first_divergence` and the worst findings, `inspect` the step,
fix the application module, replay again. Comparisons and annotations accumulate in the
store, coverage says which screens still have no gold, and the cost of the next thousand
trajectories is the simulator's alone, because a reference, once captured, is reused on
every build.
