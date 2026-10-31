# Parity: oss-json-server.html

- nodes: 85 (elements and text nodes in Chromium's dump)
- passed: 84 (98.8%)
- missing from the engine's dump: 0
- threshold: 98.0%
- verdict: pass

Tolerances: rect edges within 1 px; widths and heights of text-dependent boxes (inline, table parts, floats, absolutes) within 2 px; keyword properties exact; `font-family` informational.

## Mismatches by property

| property | nodes |
|---|---|
| lines | 1 |

## Worst offenders

### `html>body>main:nth-child(2)>div:nth-child(1)>p:nth-child(2)>#text:nth(1)` (score 40.00)

- lines: expected `5`, got `1`

