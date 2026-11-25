# cw-jsvm: data it bundles

The interpreter is MIT. `data/cldr.json` is locale data (month and day names, number and date patterns) read from
ICU 78.3 as Node v24.21.0 ships it (`tools/generate_cldr.js`). It derives from the Unicode CLDR, © Unicode, Inc., under
the Unicode License v3; the full text is `LICENSES/Unicode-3.0.txt`.
