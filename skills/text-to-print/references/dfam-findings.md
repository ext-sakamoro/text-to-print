# DfAM findings — how to read `ttp check`

`dfam.findings[]` is ordered by severity. Each entry is `{verdict, message}`
with the message shaped `DfAM <check>: <measured> … <limit> (<source>) …`.

| check | measured | limit (FDM default) | verdict rules |
|---|---|---|---|
| watertight | non-manifold / boundary edge counts | 0 / 0 | fail if any — mesher property, slicers often auto-repair |
| scale | bbox diagonal | 2 mm … 1.5 m plausible | `need more info` outside → all dimensional verdicts withheld |
| wall thickness | **p05** of ray-cast thickness (min also shown) | ≥ 1.6 mm unsupported wall | measured only — the verdict moved to `printability` (2026-09-28), see below |
| positive feature | min thickness | ≥ 0.8 mm | fail if min < limit **and** p05 < limit; `need more info` if only min |
| hole diameter | narrowest enclosed exterior gap (2 × SDF at local max, ≥ 4 of 6 axis rays hit material) | ≥ 2.0 mm | fail if < limit; `need more info` if grid too coarse to resolve the limit |
| bridge | longest XY span of a connected downward (> 45°) region not touching the plate | ≤ 10 mm | fail if > limit; upper bound (includes cantilevers) |
| support ratio | downward area not on plate / total area | 30 % advisory | `advisory` above — cost signal, not a failure |

## `printability` — the proof-based verdict (owns wall thickness + connectivity)

`ttp check` / `ttp export --format 3mf` also return a `printability` object.
It answers the same wall-thickness question as `dfam wall thickness` but
**states how it knows**, which is why the verdict lives here: DfAM measures
p05 over ray-cast samples, so a thin wall between samples reads as "nothing
found", and "nothing found" is not "nothing there".

| field | values | reading |
|---|---|---|
| `erosion` | `proved` / `violated` / `undecided` | erosion by `min_wall_mm / 2` is exact for an SDF (`Round { radius: -t/2 }`), so `violated` proves **every** region is thinner than the limit and `proved` exhibits a witness box that is thick enough. `undecided` = octree depth ran out |
| `min_local_thickness_mm` | mm or `null` | from each triangle centroid, march inward by `-d(p)`: every individual number is exact, the sampling is *where* (the tessellation) |
| `thin_triangles` | count | triangles under `min_wall_mm` |
| `connectivity` | `proved` / `violated` / `undecided` / `not_run` | two-point reachability (`alice_lol::law::Constraint::Reachable`) between the two farthest interior points. `violated` = separate pieces (the print comes apart). Resolution-dependent: the check retries at a finer grid before reporting `undecided` |
| `max_overhang_deg` / `overhang_triangles` | deg / count | closed form `asin(-n·b)`, recorded only — a sphere always has downward faces, so this never gates |
| `fail_messages` | `Printability <check>: …` | the retry-worthy failures |
| `notes` | text | undecided states, verbatim for the user |

Fixes: a `wall thickness` failure means thicken the wall / shell in the LOL
(not lower the limit); a `connectivity` failure means overlap the pieces by
≥ 1 mm inside `union()` or add a connecting bar.

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
