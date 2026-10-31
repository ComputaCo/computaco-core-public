# Parity: oss-react-admin.site.json

- nodes: 941 (elements and text nodes in Chromium's dump)
- passed: 940 (99.9%)
- missing from the engine's dump: 0
- threshold: 99.0%
- verdict: pass

Tolerances: rect edges within 1 px; widths and heights of text-dependent boxes (inline, table parts, floats, absolutes) within 2 px; keyword properties exact; `font-family` informational.

## Mismatches by property

| property | nodes |
|---|---|
| background-color | 1 |

## Worst offenders

### `html>body>div:nth-child(4)>div:nth-child(3)>div:nth-child(1)>div:nth-child(1)>div:nth-child(1)>div:nth-child(1)>div:nth-child(1)` (score 1.00)

- background-color: expected `rgba(0, 0, 0, 0.04)`, got `rgba(0, 0, 0, 0.06)`

