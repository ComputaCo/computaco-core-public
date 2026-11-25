# cw-fonts: the faces it bundles, and their licences

The crate's code is MIT. Every face below keeps its own licence, and its full text ships beside it in `fonts/`. Each
face is a subset or an instance built by `build-fonts.py`; outlines, advances and names are unchanged, and each file's
name table still carries its copyright and licence.

| Faces | Files | Source and author | Licence | Licence text |
|---|---|---|---|---|
| DejaVu Sans and DejaVu Sans Mono | `fonts/dejavu-*.ttf` | https://dejavu-fonts.github.io/; Bitstream, Inc. and the DejaVu authors | Bitstream-Vera (DejaVu's changes are public domain) | `fonts/FONT-LICENSE.txt` |
| Noto Sans Hebrew, Arabic, Thai, Devanagari, Bengali, Georgian and Armenian | `fonts/noto-*.ttf` | https://github.com/google/fonts at a pinned commit; the Noto Project Authors | OFL-1.1 | `fonts/NOTOSANS*-OFL.txt` |
| Layout stubs of the font packs' faces (no outlines) | `fonts/stubs/*.ttf` | made from the Noto faces of cw-font-pack-sc, -cjk and -emoji | OFL-1.1 | the `NOTO*-OFL.txt` of each face, in `fonts/` |
| Inter, Open Sans, Roboto | `fonts/{inter,opensans,roboto}-*.ttf` | https://github.com/google/fonts (`ofl/inter`, `ofl/opensans`, `ofl/roboto`); The Inter Project Authors, The Open Sans Project Authors, The Roboto Project Authors | OFL-1.1 | `fonts/INTER-OFL.txt`, `fonts/OPENSANS-OFL.txt`, `fonts/ROBOTO-OFL.txt` |
| Ubuntu | `fonts/ubuntu-*.ttf` | Ubuntu's `fonts-ubuntu` package, https://design.ubuntu.com/font; Canonical Ltd | Ubuntu-font-1.0 | `fonts/UBUNTU-UFL.txt` |
| Arimo, Tinos, Cousine, Gelasio, Carlito, Caladea, Lato, Source Sans 3, Source Serif 4, Poppins, Montserrat, Playfair Display, JetBrains Mono | `fonts/{arimo,tinos,cousine,gelasio,carlito,caladea,lato,sourcesans,sourceserif,poppins,montserrat,playfair,jetbrainsmono}-*.ttf` | https://github.com/google/fonts at a pinned commit (Tinos's licence from the Tinos repository); each family's authors, as its licence file names them | OFL-1.1 | the family's `fonts/*-OFL.txt` |

Lato, Source Sans 3, Source Serif 4 and Playfair Display declare Reserved Font Names in their licences; the subsets
keep the upstream names in their name tables (cw-render's `assets/README.md`, "Web faces").
