# cw-render: what it bundles, and under which licence

The crate's code, and the artwork we made, are MIT. The crate also bundles artwork from other projects. Each set keeps
its own licence: the share-alike and copyleft sets stay under their licences, and our MIT does not apply to them. The
crate's `license` field names every licence below. The full licence texts ship in this crate, at the paths in the last
column.

| Asset family | Files | Source and author | Licence | Licence text in this crate |
|---|---|---|---|---|
| FreeCAD's icons, rendered by FreeCAD 1.0.2 to PNG | `assets/apps/freecad/*.png` | https://github.com/FreeCAD/FreeCAD (`src/Gui/Icons`, `src/Mod/PartDesign/Gui/Resources/icons`, `src/Mod/Sketcher/Gui/Resources/icons`); the FreeCAD contributors | LGPL-2.1-or-later | `assets/apps/freecad/LICENSE` |
| Visual Studio Code's codicons (@vscode/codicons 0.0.46, as VS Code 1.143 ships them), rasterised to alpha masks | `assets/icons/vscode/*.png` | https://github.com/microsoft/vscode-codicons; Microsoft Corporation | CC-BY-4.0 | `assets/icons/vscode/LICENSE.txt` |
| KiCad 7.0.11's bitmaps, unchanged | `assets/icons/kicad/*.png` | https://gitlab.com/kicad/code/kicad (`bitmaps_png/`, through Debian's kicad 7.0.11+dfsg-1build4); the KiCad Developers | CC-BY-SA-4.0 | `assets/icons/kicad/LICENSE.txt` |
| GIMP 2.10.36's Symbolic icons, rasterised and tinted | `assets/icons/gimp/*.png` | https://gitlab.gnome.org/GNOME/gimp (tag `GIMP_2_10_36`, `icons/Symbolic/`); Ell, Jakub Steiner, tigert.com, Klaus Staedtler, Barbara Muraus and other authors | GPL-3.0-or-later and CC-BY-SA-3.0 and CC-BY-SA-4.0 and CC0-1.0, as the upstream package states them | `assets/icons/gimp/LICENSE.txt` |
| Yaru icons, unchanged: every `ubuntu-*.png` with no `.svg` beside it | `assets/icons/ubuntu-{browser,calculator,calendar,camera,chat,clock,contacts,docs,files,launcher,mail,maps,music,notes,phone,photos,settings,spreadsheet,store,terminal,trash,weather}.png` | https://github.com/ubuntu/yaru (Ubuntu's `yaru-theme-icon`); Sam Hewitt and the Yaru contributors | CC-BY-SA-4.0 | `assets/YARU-COPYRIGHT.txt` |
| Ubuntu 24.04 "Noble Numbat" default wallpaper, resized and re-encoded | `assets/wallpapers/ubuntu.jpg` | https://launchpad.net/ubuntu-wallpapers; Canonical Ltd | CC-BY-SA-4.0 | `assets/UBUNTU-WALLPAPER-COPYRIGHT.txt` |
| DejaVu fonts, the masters of cw-fonts' subsets | `assets/DejaVuSans.ttf`, `assets/DejaVuSans-Bold.ttf`, `assets/DejaVuSansMono.ttf` | https://dejavu-fonts.github.io/; Bitstream, Inc. and the DejaVu authors | Bitstream-Vera (DejaVu's changes are public domain) | each font's name table carries it; the same text is `fonts/FONT-LICENSE.txt` of cw-fonts |
| Ours: the symbols | `assets/symbols/*` | drawn in `assets/generate-symbols.py`; ComputaCo | MIT | the crate's `license` field |
| Ours: the platform icons (macOS, Windows, iOS, Android, and each Ubuntu one with an `.svg` beside it) | `assets/icons/{macos,windows,ios,android}-*`, `assets/icons/ubuntu-{code,freecad,gimp,pinta,kicad,kdenlive,database}.*` | drawn by hand in `assets/generate-icons.py`; ComputaCo. They are styled after each platform; no vendor's file was copied | MIT | the crate's `license` field |
| Ours: the macOS, Windows, iOS and Android wallpapers | `assets/wallpapers/{macos,windows,ios,android}.jpg` | made by an image model from the prompts in `assets/WALLPAPER-PROMPTS.md`, with no reference image; ComputaCo | MIT | the crate's `license` field |

The wallpapers' PNG masters and the tests are not in the published crate (`exclude` in `Cargo.toml`).
