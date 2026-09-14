# DfAM findings — how to read `ttp check`

`dfam.findings[]` is ordered by severity. Each entry is `{verdict, message}`
with the message shaped `DfAM <check>: <measured> … <limit> (<source>) …`.

| check | measured | limit (FDM default) | verdict rules |
|---|---|---|---|
| watertight | non-manifold / boundary edge counts | 0 / 0 | fail if any — mesher property, slicers often auto-repair |
| scale | bbox diagonal | 2 mm … 1.5 m plausible | `need more info` outside → all dimensional verdicts withheld |
| wall thickness | **p05** of ray-cast thickness (min also shown) | ≥ 1.6 mm unsupported wall | fail if p05 < limit (min alone can be a sampling outlier) |
| positive feature | min thickness | ≥ 0.8 mm | fail if min < limit **and** p05 < limit; `need more info` if only min |
| hole diameter | narrowest enclosed exterior gap (2 × SDF at local max, ≥ 4 of 6 axis rays hit material) | ≥ 2.0 mm | fail if < limit; `need more info` if grid too coarse to resolve the limit |
| bridge | longest XY span of a connected downward (> 45°) region not touching the plate | ≤ 10 mm | fail if > limit; upper bound (includes cantilevers) |
| support ratio | downward area not on plate / total area | 30 % advisory | `advisory` above — cost signal, not a failure |

`orientation_hint` (when present): `rotate: <label> → support A → B mm² (-N%), height H mm`.
Labels: `-Z up`, `+X up`, … (axis-aligned) or `sphere #n` (free direction).
The rotation takes that model direction to +Z; apply it in the LOL with
`rotate(...)` or tell the user to rotate in the slicer.

Process limits table (mm, conservative defaults):

| limit | FDM | SLS | SLA/DLP | PBF-LB metal | MJF |
|---|---|---|---|---|---|
| min supported wall | 1.2 | 0.7 | 0.5 | 0.4 | 0.5 |
| min unsupported wall | 1.6 | 0.7 | 1.0 | 0.5 | 0.5 |
| self-supporting angle (° from horizontal) | 45 | n/a | 30 | 45 | n/a |
| min hole diameter | 2.0 | 1.5 | 0.5 | 1.5 | 1.0 |
| min positive feature | 0.8 | 0.8 | 0.2 | 0.4 | 0.5 |
| max unsupported bridge | 10 | n/a | 5 | 2 | n/a |
