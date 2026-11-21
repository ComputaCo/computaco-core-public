#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Size flags for the Wasm build only. They are passed with `--config` rather
# than written into `[profile.release]` because that profile is shared with the
# native builds the benchmarks in docs/performance.md measure, and optimizing
# those for size instead of speed would silently invalidate them.
#
#   opt-level="z"   size over speed for the world, services, kernel and glue.
#   lto="fat"       whole-program, not just per-crate; the module is one cdylib.
#   strip="symbols" drops the Wasm `name` section, 1.4 MB of debug symbols that a
#                   shipped bundle never reads. wasm-bindgen works from the export
#                   table and its own custom sections, not from names.
#
# The pixel path stays at opt-level 3: the rasterizer and the decoders it calls.
# Measured on a 960x640 desktop render in Node (warm p50; gzip -9 of pkg/web):
#
#                                        gzip -9     render p50
#   opt-level 3 everywhere             6,004,786       ~50 ms
#   opt-level z everywhere             4,906,012       ~88 ms
#   z, raster path at 3 (this build)   4,931,475       ~70 ms
#   z, raster + scene building at 3    5,369,981       ~50 ms
#
# The per-package overrides survive fat LTO (the numbers above are with it).
# panic is deliberately not set: wasm32-unknown-unknown already aborts, so there
# is nothing to gain and `[profile.release]` would leak it into native builds.
fast=()
for package in cw-render fontdue jpeg-decoder png miniz_oxide fdeflate; do
  fast+=(--config "profile.release.package.$package.opt-level=3")
done
size=(--config 'profile.release.opt-level="z"' --config 'profile.release.lto="fat"' --config 'profile.release.strip="symbols"')
# Panic locations and debug strings carry each source file's absolute path, which put
# 227 paths naming the build machine's home directory into the public bundle. Remap
# them to fixed prefixes. Later mappings win when several match, so the home directory
# goes first and the more specific ones after it. The standard library's paths are
# already `/rustc/<commit>/`.
cargo_home=${CARGO_HOME:-$HOME/.cargo}
rustup_home=${RUSTUP_HOME:-$HOME/.rustup}
remap=(
  "--remap-path-prefix=$HOME=~"
  "--remap-path-prefix=$rustup_home=/rustup"
  "--remap-path-prefix=$(rustc --print sysroot)=/rustup/toolchain"
  "--remap-path-prefix=$cargo_home/registry/src=/cargo/registry"
  "--remap-path-prefix=$cargo_home/git/checkouts=/cargo/git"
  "--remap-path-prefix=$PWD=."
)
# RUSTFLAGS and CARGO_ENCODED_RUSTFLAGS replace any configured rustflags rather than
# adding to them, so when the caller set one the mappings are appended to it; otherwise
# they go in as Wasm-target config, which cargo merges with any config file's.
if [ -n "${CARGO_ENCODED_RUSTFLAGS:-}" ]; then
  for flag in "${remap[@]}"; do CARGO_ENCODED_RUSTFLAGS+=$'\x1f'"$flag"; done
  export CARGO_ENCODED_RUSTFLAGS
elif [ -n "${RUSTFLAGS:-}" ]; then
  export RUSTFLAGS="$RUSTFLAGS ${remap[*]}"
fi
paths=()
if [ -z "${CARGO_ENCODED_RUSTFLAGS:-}${RUSTFLAGS:-}" ]; then
  flags=""
  for flag in "${remap[@]}"; do flags+="${flags:+, }'$flag'"; done
  paths=(--config "target.wasm32-unknown-unknown.rustflags=[$flags]")
fi
# What a lazy build (CW_WASM_LAZY=1) leaves out of the engine and serves from modules of
# its own, each fetched and instantiated the first time one of its applications is
# launched (docs/app-modules.md).
# The modules are the catalog's (packages/catalog.json): every native package that names one.
mapfile -t modules < <(node -e 'const c=require("./packages/catalog.json");const seen=new Set();for(const p of c.packages){if(p.runtime&&p.runtime.kind==="native"&&p.runtime.module&&!seen.has(p.runtime.module)){seen.add(p.runtime.module);console.log(p.runtime.module)}}')
heavy=$(IFS=,; echo "${modules[*]}")
# CW_WASM_KEEP=freecad (a comma list of those applications) keeps them in a lazy engine
# instead of in modules of their own. The hosted API keeps FreeCAD: a procedure's CAD
# checks and its clicks on a model need the CAD kernel in the engine (docs/cloud-api.md).
keep=()
if [ -n "${CW_WASM_KEEP:-}" ]; then
  IFS=, read -r -a keep <<<"$CW_WASM_KEEP"
  for kept in "${keep[@]}"; do
    case ",$heavy," in *",$kept,"*) ;; *) echo "CW_WASM_KEEP: $kept is not one of $heavy" >&2; exit 1 ;; esac
  done
  lazy=()
  for module in "${modules[@]}"; do
    case ",$CW_WASM_KEEP," in *",$module,"*) ;; *) lazy+=("$module") ;; esac
  done
  modules=("${lazy[@]}")
fi
features=()
# CW_WASM_INTERNET=0 leaves the built-in internet out: a smaller bundle that boots only
# worlds setting `internet: false`.
internet=",internet"
if [ "${CW_WASM_INTERNET:-1}" = 0 ]; then internet=""; fi
# CW_WASM_PLATFORM=ubuntu (or macos, windows, ios, android) builds one platform's shell,
# wallpaper and icons with every application: a world drawn as another platform is
# refused when it boots. Measured 2026-10-28 (gzip -9 of the module): every platform
# 15,426,237; ios 13,104,380; ubuntu 13,183,342; android 13,198,322; windows 13,219,773;
# macos 13,682,173. The applications, the engines and the internet are shared by all.
platforms="os-macos,os-windows,os-ubuntu,os-ios,os-android"
if [ -n "${CW_WASM_PLATFORM:-}" ]; then platforms="os-$CW_WASM_PLATFORM"; fi
if [ "${CW_WASM_LAZY:-0}" = 1 ]; then
  # Each module is the application code alone (cw-app-module with one feature); the
  # engine is built after them, pinning each module's SHA-256 (CW_APP_MODULE_<NAME>).
  rm -rf pkg/modules && mkdir -p pkg/modules
  for module in "${modules[@]}"; do
    cargo build -p cw-app-module --release --target wasm32-unknown-unknown --features "$module" "${size[@]}" "${paths[@]}"
    cp target/wasm32-unknown-unknown/release/cw_app_module.wasm "pkg/modules/$module.wasm"
    export "CW_APP_MODULE_${module^^}=$(sha256sum "pkg/modules/$module.wasm" | cut -d' ' -f1)"
  done
  # Python is a module too, fetched the first time a program runs; it asks the machine
  # for everything it touches (cw_modules::script).
  cargo build -p cw-py-module --release --target wasm32-unknown-unknown "${size[@]}" "${paths[@]}"
  cp target/wasm32-unknown-unknown/release/cw_py_module.wasm pkg/modules/python.wasm
  export "CW_MODULE_PYTHON=$(sha256sum pkg/modules/python.wasm | cut -d' ' -f1)"
  # The internet's world is a data file fetched when a world joins it.
  if [ -n "$internet" ]; then internet=",internet-on-demand"; fi
  # The engine is then split into code modules (scripts/split-modules.mjs: each standard
  # service kind), which needs its function names; the split drops them. It needs
  # binaryen 121 or later (BINARYEN, a bin directory, or PATH).
  # The web families' metric tables, written from the tables a full build embeds, are
  # fetched the first time web text is measured; the engine pins each file.
  rm -rf target/metrics
  cargo run -q -p cw-scene --example export_tables -- target/metrics
  export CW_METRICS_DIR="$PWD/target/metrics"
  kept=""
  for app in "${keep[@]}"; do kept+=",$app"; done
  features=(--no-default-features --features "$platforms,services,python-module,node,sqlite$internet,app-modules,lazy-render,lazy-metrics,code-modules$kept")
  size=(--config 'profile.release.opt-level="z"' --config 'profile.release.lto="fat"' --config 'profile.release.strip="none"')
elif [ -n "${CW_WASM_PLATFORM:-}" ] || [ "${CW_WASM_INTERNET:-1}" = 0 ]; then
  features=(--no-default-features --features "$platforms,services,python,node,sqlite,$heavy$internet")
fi
if [ "${CW_WASM_LAZY:-0}" = 1 ]; then
  # The split reads the linker's map and relocations to move each module's data with it.
  cargo rustc -p cw-wasm --lib --release --target wasm32-unknown-unknown "${size[@]}" "${paths[@]}" "${fast[@]}" "${features[@]}" \
    -- -C link-arg=--emit-relocs -C link-arg=--Map=target/engine.map
else
  cargo build -p cw-wasm --release --target wasm32-unknown-unknown "${size[@]}" "${paths[@]}" "${fast[@]}" "${features[@]}"
fi

rm -rf pkg/web/modules pkg/node/modules pkg/web/data pkg/node/data pkg/web/render.bin pkg/node/render.bin
wasm-bindgen target/wasm32-unknown-unknown/release/cw_wasm.wasm --target web --out-dir pkg/web --out-name computerworld
wasm-bindgen target/wasm32-unknown-unknown/release/cw_wasm.wasm --target nodejs --out-dir pkg/node --out-name computerworld
if [ "${CW_WASM_LAZY:-0}" = 1 ]; then
  # Both targets' engines are the same bytes, so one split serves both.
  cmp -s pkg/web/computerworld_bg.wasm pkg/node/computerworld_bg.wasm || { echo "wasm-bindgen wrote two engines for web and node" >&2; exit 1; }
  node scripts/split-modules.mjs split pkg/web/computerworld_bg.wasm pkg/node/computerworld.js target/wasm32-unknown-unknown/release/cw_wasm.wasm target/engine.map target/split
  cp target/split/engine.wasm pkg/web/computerworld_bg.wasm
  cp target/split/engine.wasm pkg/node/computerworld_bg.wasm
  cp target/split/modules/*.wasm pkg/modules/
  node scripts/split-modules.mjs glue pkg/web/computerworld.js pkg/node/computerworld.js
fi

# No `wasm-opt` pass, deliberately. Measured on this module with binaryen 119,
# every optimization level shrinks the raw file but makes the gzipped download —
# the number that matters — larger, because its rewrites destroy the repetition
# DEFLATE exploits:
#
#                  raw bytes    gzip -9
#   no wasm-opt   10,039,467  4,905,851
#   wasm-opt -Oz   9,063,190  5,058,318  (+152 KB gzip)
#   wasm-opt -Os   9,187,046  5,057,108
#   wasm-opt -O2   9,301,169  5,066,508
#
# Revisit if the bundle is ever served raw, or if a binaryen release changes this.

for package in pkg/web pkg/node; do
  mkdir -p "$package/notices"
  cp LICENSE "$package/notices/PROJECT-LICENSE.txt"
  cp crates/graphics/render/assets/FONT-LICENSE.txt crates/graphics/render/assets/YARU-COPYRIGHT.txt crates/graphics/render/assets/UBUNTU-WALLPAPER-COPYRIGHT.txt crates/graphics/render/assets/fonts/*.txt "$package/notices/"
  cp crates/graphics/render/assets/README.md "$package/notices/ASSET-ATTRIBUTION.md"
  cp crates/graphics/render/assets/apps/freecad/LICENSE "$package/notices/FREECAD-ICONS-LICENSE.txt"
  cp crates/graphics/render/assets/apps/freecad/README.md "$package/notices/FREECAD-ICONS.md"
  cp crates/graphics/render/assets/icons/kicad/LICENSE.txt "$package/notices/KICAD-BITMAPS-LICENSE.txt"
  cp crates/graphics/render/assets/icons/gimp/LICENSE.txt "$package/notices/GIMP-ICONS-LICENSE.txt"
  # The CJK/emoji font pack is not in the module (native builds embed it). Pages
  # fetch `fonts/<file>` on demand and hand the bytes to `installFont`; see
  # crates/graphics/render/assets/README.md. Its OFL notices are among fonts/*.txt above.
  mkdir -p "$package/fonts"
  cp crates/graphics/render/assets/fonts/pack/*.ttf "$package/fonts/"
  # A lazy build also fetches the web families and its application modules, through
  # the module host (`modules.cjs` for Node, `modules.js` for a page or a worker).
  if [ "${CW_WASM_LAZY:-0}" = 1 ]; then
    for family in arimo tinos cousine gelasio carlito caladea lato sourcesans sourceserif poppins montserrat playfair jetbrainsmono; do
      cp crates/graphics/render/assets/fonts/$family-*.ttf "$package/fonts/"
    done
    # The four platforms' UI families: a one-platform build fetches the others' on first use.
    for family in inter opensans ubuntu roboto; do
      cp crates/graphics/render/assets/fonts/$family-*.ttf "$package/fonts/"
    done
    # The script faces and the pack's layout stubs, which text is shaped with.
    cp crates/graphics/render/assets/fonts/noto-*.ttf "$package/fonts/"
    # What only the renderer draws with, fetched by the first frame: DejaVu, the icons,
    # the wallpapers and the symbol masks (the UI families are above).
    cp crates/graphics/render/assets/fonts/dejavu-*.ttf "$package/fonts/"
    rm -rf "$package/assets" && mkdir -p "$package/assets/icons" "$package/assets/wallpapers" "$package/assets/symbols"
    for platform in ${CW_WASM_PLATFORM:-macos windows ubuntu ios android}; do
      cp crates/graphics/render/assets/icons/$platform-*.png "$package/assets/icons/"
      cp crates/graphics/render/assets/wallpapers/$platform.jpg "$package/assets/wallpapers/"
    done
    cp crates/graphics/render/assets/symbols/*.png "$package/assets/symbols/"
    mkdir -p "$package/assets/icons/vscode"
    cp crates/graphics/render/assets/icons/vscode/*.png "$package/assets/icons/vscode/"
    # Applications' own icons (FreeCAD's), fetched one by one when first drawn.
    for app in crates/graphics/render/assets/apps/*/; do
      mkdir -p "$package/assets/apps/$(basename "$app")"
      cp "$app"*.png "$package/assets/apps/$(basename "$app")/"
    done
    mkdir -p "$package/assets/icons/kicad"
    cp crates/graphics/render/assets/icons/kicad/*.png "$package/assets/icons/kicad/"
    mkdir -p "$package/assets/icons/gimp"
    cp crates/graphics/render/assets/icons/gimp/*.png "$package/assets/icons/gimp/"
    mkdir -p "$package/fonts/metrics"
    cp target/metrics/*.bin "$package/fonts/metrics/"
    mkdir -p "$package/fonts/stubs"
    cp crates/graphics/render/assets/fonts/stubs/*.ttf "$package/fonts/stubs/"
    cp -r pkg/modules "$package/modules"
    if [ -n "$internet" ]; then
      mkdir -p "$package/data"
      cp worlds/internet/world.json "$package/data/internet.json"
    fi
    # The node-app packages, fetched by hash the first time a world's site needs one
    # (docs/app-packages.md); their hashes are pinned in packages/catalog.json.
    node scripts/pack-node-apps.mjs "$package/data" >/dev/null
  fi
done
cp crates/bindings/wasm/js/modules.cjs pkg/node/
cp crates/bindings/wasm/js/modules.js pkg/web/
if [ "${CW_WASM_LAZY:-0}" = 1 ]; then
  # What a frame draws with goes as one file, the render bundle, in place of its single
  # files (docs/app-modules.md § The render bundle).
  node scripts/pack-render.cjs pkg/node pkg/web
fi

for package in pkg/web pkg/node; do
  raw=$(wc -c <"$package/computerworld_bg.wasm")
  gz=$(gzip -9 -c "$package/computerworld_bg.wasm" | wc -c)
  printf '%s %12d raw %12d gzip -9\n' "$package" "$raw" "$gz"
done
if [ -e pkg/web/render.bin ]; then
  printf '%s %12d raw %12d gzip -9 (the render bundle, fetched by the first frame)\n' pkg/web/render.bin \
    "$(wc -c <pkg/web/render.bin)" "$(gzip -9 -c pkg/web/render.bin | wc -c)"
fi
for module in pkg/web/modules/*.wasm; do
  [ -e "$module" ] || continue
  raw=$(wc -c <"$module")
  gz=$(gzip -9 -c "$module" | wc -c)
  printf '%s %12d raw %12d gzip -9 (module, fetched on first use)\n' "$module" "$raw" "$gz"
done
for font in pkg/web/fonts/*.ttf pkg/web/fonts/stubs/*.ttf; do
  [ -e "$font" ] || continue
  raw=$(wc -c <"$font")
  gz=$(gzip -9 -c "$font" | wc -c)
  printf '%s %12d raw %12d gzip -9 (font, fetched on first use)\n' "$font" "$raw" "$gz"
done
