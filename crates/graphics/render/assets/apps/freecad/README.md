# FreeCAD icons

The icons the simulated FreeCAD draws: its toolbar commands (24 px, each with the
disabled form Qt derives, `-off`), and the model tree's, workbench selector's,
Sketcher constraint list's and element list's icons (16 px, `-16`).

They are FreeCAD 1.0.2's own artwork, copyright the FreeCAD contributors, licensed
under the GNU Lesser General Public License version 2.1 or later (`LICENSE` beside
this file). Source: https://github.com/FreeCAD/FreeCAD (`src/Gui/Icons`,
`src/Mod/PartDesign/Gui/Resources/icons`, `src/Mod/Sketcher/Gui/Resources/icons`).

They were not redrawn. FreeCAD 1.0.2 (the conda AppImage) rendered them itself: a
start-up macro run under Xvfb activated the Part Design and Sketcher workbenches and
saved each toolbar action's `QIcon` at 24 px, in its normal and disabled modes, and
each named resource under `:/icons` the simulator uses at 16 px, as PNG. A file is
named after FreeCAD's command (`PartDesign_Pad.png`) or resource
(`Sketcher_Sketch-16.png`). Only the icons the simulator shows are kept.
