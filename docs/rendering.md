# Rendering and observations

Two pipelines end in one scene. A native application's is semantic application state →
native page → layout → scene → optional RGBA. A web page's is HTML → DOM → cascade →
layout → paint → scene → optional RGBA, run by `cw-web`. Everything below the scene — hit testing, the semantic
observation, the rasterizer, the font stack — is shared, and structured-only observations
stop before rasterization in both. No Chromium process is involved in either.

`cw-scene::Scene` stores dimensions, background, revision, nodes and an optional
`typeface`. A node has a stable ID, integer bounds, primitive, optional semantic
role/label/value, interaction identifier, clipping, optional `rounded_clip`
(`{rect, radius}`, honoured by hit testing), transform, z-order and opacity.
A client that round-trips scene JSON must carry `typeface` and `rounded_clip`
through, or it will silently lose font selection and window-corner clipping. A
`UiText`/`UiTextBold` node may carry its own `typeface` (omitted when absent), which
overrides the scene's for that node: browser pages whose theme names a `font` set every
text node in the family the list resolves to (`cw_scene::fonts::resolve_family`).

The twelve primitives are `Box`, `RoundedBox`, `UiText`, `UiTextBold`, `Text`,
`AssetImage`, `Shadow`, `Image`, `Path`, `Symbol`, `Backdrop` and `Region`
(invisible interaction area). `Symbol` is a tintable vector glyph; `Backdrop` is a
frosted-glass material. Affine coefficients use 1/1024 units; clipping is expressed
in scene coordinates.

The scene supplies deterministic flow layout. Text measurement is per-primitive:
`UiText`/`UiTextBold` are measured proportionally from the generated advance tables
in `cw-scene::metrics`, with word wrapping and ellipsizing; the `Text` primitive
(terminals, editors) keeps fixed-cell measurement and its golden pixels for plain
rows. `UiText`/`UiTextBold` take two optional fields, omitted from scene JSON when
unset so existing scenes serialize unchanged: `italic` (bool) and `lang` (`"zh-Hans"`,
`"zh-Hant"`, `"ja"`, `"ko"`), which picks regional Han forms. `cw_scene::Style`
(bold, italic, lang) is what the metrics functions take; a plain `bool` still means
bold.

Pair kerning: the web faces (`Typeface::WEB` — Arimo, Tinos, Carlito, Caladea, Lato,
Source Sans 3, Source Serif 4, Montserrat and Playfair Display; Cousine, JetBrains
Mono, Gelasio and Poppins have no `kern` pairs) kern as Chromium does.
`metrics::kern(typeface, style, left, right, size)` is the adjustment between two
adjacent characters in the same 1/64 px as `advance` (`kern_units` gives it in font
units), and `text_width`, `wrap`, `ellipsize` and `text::layout` all add it, so the
renderer, which places a run's glyphs from `text::layout`, draws a kerned run exactly
as wide as it measures. The platform faces and DejaVu (`Inter`, `OpenSans`, `Ubuntu`,
`Roboto`, `DejaVu`, `Mono`) are not kerned: the desktop shells and apps were laid out,
and their golden frames pinned, on plain advances, and the bundled subsets carry no
GPOS for the renderer to have applied. The web engine turns kerning off for a run
with a non-zero `letter-spacing`, as CSS requires.
Hit testing uses transformed coordinates, clipping, disabled state and z-order;
later nodes win equal-z ties. It operates without generating pixels.

`ScenePatch` applies upserts, removals and background changes against an expected
revision and produces damaged rectangles. Invalid revision/duplicate-ID updates
are rejected atomically. The rasterizer can use damage to update the existing
frame; compare full and incremental output in tests when adding primitives.
Thirty-five font files are embedded, not one: DejaVu Sans, Sans Bold, Sans Oblique,
Sans Bold Oblique and Sans Mono; regular, bold, italic and bold italic Inter, Open
Sans, Roboto and Ubuntu for the per-platform shells; and regular and bold Noto Sans
Hebrew, Arabic, Thai, Devanagari, Bengali, Georgian and Armenian. The CJK faces (Noto
Sans SC and KR in two weights, and Traditional Chinese, Japanese and Korean locale
forms), Noto Emoji, Noto Color Emoji and eight further scripts form a *font pack* that
native builds embed and the Wasm build fetches on demand. They ship under three
licenses (DejaVu, SIL OFL 1.1 and the Ubuntu Font Licence 1.0); every notice ships with the
packages, in the npm package's `notices/`.

`UiText` is laid out by `cw_scene::text`: a deterministic fallback chain, Unicode
bidirectional reordering (right-to-left paragraphs and mixed runs), OpenType shaping
with `rustybuzz` (Arabic joining and lam-alef, Indic reordering and conjuncts, Thai
and Hebrew mark attachment, emoji ZWJ/flag/keycap/skin-tone ligatures) and line
breaking between CJK characters with kinsoku. Scene metrics and the renderer share
that one layout, so measured widths, wraps and ellipses are exactly what is drawn.

Emoji draw in colour from Noto Color Emoji's COLRv1 tables, painted by the
renderer's own deterministic COLR rasterizer (gradients, clip boxes, transforms and
composite modes), identically natively and in Wasm; the monochrome glyphs remain the
fallback while the colour file is absent.

Scene layout is a deliberately smaller system than the web engine above it: it is flow
layout for native pages, and it does not claim CSS, vertical or justified text or
browser compositor compatibility. Native applications should be designed for that
contract. Web pages are not: they go through `cw-web`, which is where CSS, the DOM and
script live, and which is what every site on the synthetic internet is written against. The limits
that remain — no vertical writing modes, no synthetic italics where a face lacks a
slant — are listed with the engine's supported property table and enforced for authored
sites by its strict HTML validator.

Benchmarks must distinguish layout, scene patching, rasterization, readback and
image encoding. See [performance](performance.md) for measurements and
comparison limits; lower scene-update time alone does not establish faster full
pixel production.

Raster details: source-over compositing uses straight RGBA. `Image` uses nearest
neighbour sampling; `AssetImage` larger than 256x256 (wallpapers) uses a centred,
aspect-preserving crop with bilinear filtering. Polygon paths support fills and
strokes, antialiased. `fontdue` is pinned and uses the bundled fonts.
Glyph/text-mask caches are bounded. The checked raster API rejects frames above
16,777,216 pixels. Text wraps and ellipsizes on measured advances for `UiText`, on
fixed cells for `Text`, with fallback for unavailable glyphs. `Text` lays out as a
terminal does (`cw_scene::text::terminal`): grapheme clusters in cells, combining
marks in their base's cell, East Asian wide characters and emoji across two cells,
right-to-left runs reordered within each row (VTE's implicit bidi mode) and script
runs shaped, so Arabic joins cell by cell.

## A GPU view

A view of many machines at once (a wall you zoom from ten thousand machines down to one
at native pixels) is drawn with WebGPU, not by the CPU rasterizer. The spike is
`benchmarks/gpu-wall/`. It is a viewer and nothing more. The CPU raster above stays the
reference renderer and the only source of `pixels.v1`. The GPU view draws a machine in
one of two ways:

- **Texture.** It uploads the CPU raster of a machine into a layer of a texture array,
  with a mip chain, and draws every visible machine as one instanced quad in one pass.
  Below 1/16 scale a machine samples its own 80×56 cell of a thumbnail atlas (mips 4–7
  of its layer, copied on the GPU). At 1:1 these pixels are the CPU raster's pixels,
  byte for byte. When a machine changes, the CPU rasterizes it again and the layer is
  uploaded again.
- **Vector.** It reads the scene (`env.scene(w, h)`) and writes one 64-byte instance for
  each primitive. A fragment shader evaluates the shape (a rounded-box SDF, a
  smoothstep shadow). When a machine changes, only its scene is read again: nothing is
  rasterized. This path is an approximation, not a second renderer. Text is drawn as
  bars. An image is drawn in its mean colour, and an asset image in the mean colour of
  the CPU raster over its bounds. A backdrop is drawn as a translucent pane with no blur,
  a symbol as a tinted rounded square, and a path as its bounding box. A rounded clip is
  treated as its rectangle. Levels of detail drop the primitives that are smaller than a
  screen pixel.

Panning and zooming change only a camera uniform, so they never rasterize. See
[performance](performance.md#a-gpu-view-of-many-machines) for the measurements, and for
where the vector path's pixels differ from the raster.

## OS desktop scenes

The optional [native OS presentation](desktop-gui.md) composes application scenes
with deterministic desktop/mobile chrome in Rust. `UiText` is proportional and picks
its face from `Scene::typeface` — Inter, Open Sans, Ubuntu or Roboto per platform,
DejaVu Sans as the fallback — or from its own `typeface`, which may also be one of the
thirteen web families (Arimo, Tinos, Cousine, Gelasio, Carlito, Caladea, Lato, Source
Sans 3, Source Serif 4, Poppins, Montserrat, Playfair Display, JetBrains Mono). `RoundedBox` supplies antialiased rounded surfaces with
matching hit tests. Existing `Text` and its golden pixels remain unchanged. Application nodes
can now have translated scene coordinates inside window content clips. The mouse pointer
is part of a desktop frame: a `Path` glyph at the last pointer position, shaped for what is
under it (see [Pointer](desktop-gui.md#pointer)); phones draw none.
