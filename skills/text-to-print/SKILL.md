---
name: text-to-print
description: Generate 3D-printable parts from natural language as LOL DSL (a parametric SDF "law", not a mesh), check them against FDM design rules (wall thickness p05, hole diameter, bridge span, support ratio, watertightness, build orientation), export 3MF / STL / STEP / G-code, and statically validate G-code against a printer's motion bounds — all locally via the `ttp` CLI with JSON output. Use when the user asks for a printable part, a bracket / holder / enclosure / mount, a Bambu Lab 3MF or G-code, whether a shape is printable, or wants a design checked or reoriented before slicing. Pairs with the `alice-print lan-send` CLI for sending validated G-code to a Bambu printer over LAN.
---

# text-to-print

Provenance: maintained in [ext-sakamoro/text-to-print](https://github.com/ext-sakamoro/text-to-print).
Use the installed local skill files as the runtime source of truth; the
repository link is only for provenance and release review.

This skill drives the **headless `ttp` CLI** (crate `text-to-print-core`). The
desktop app is not required. Everything runs locally; nothing is uploaded.

## What you produce

You author **LOL DSL** — a one-expression parametric description of the part
(SDF primitives + CSG + transforms, mm units, Z-up). `ttp` turns it into a
mesh / 3MF / G-code. Write the law, not the mesh: never hand-author STL or
coordinates lists. See `references/lol-dsl-quickref.md` for the grammar,
product shortcuts (`phone_stand`, `gridfinity_bin`, `vesa_mount`, …) and
fastener helpers (`screw_hole`, `counterbore`, …).

## Setup

```bash
# from the text-to-print workspace (sibling ALICE-* crates must be checked out)
cargo build --release -p text-to-print-core --bin ttp
export PATH="$PWD/target/release:$PATH"
```

## Workflow

1. **Write LOL** from the user's spec. Defaults when unspecified: FDM, walls
   ≥ 2 mm, holes ≥ 3 mm, part sits on the bed (`translate(..., z_half, ...)` so
   Z ≥ 0), cylinders needing a vertical axis wrapped in `rotate(90, 0, 0, …)`.
   Save to `part.lol`.
2. **Check** before exporting — this is fact-first: measurements plus
   pass / fail against cited limits.

   ```bash
   ttp check --lol part.lol
   ```

   Read `dfam.findings` in severity order (watertight → scale → wall →
   feature → hole → bridge → support). Exit code 3 means at least one `fail`
   or a safety violation. Treat `need more info` as "not proven", never as
   pass. If `dfam.orientation_hint` is present, tell the user the better
   build direction (it is a rotation to apply, not something `ttp` applies
   for you).
3. **Repair the LOL, not the mesh.** Each `fail` message carries the measured
   value, the cited limit and a location. Thicken walls, enlarge holes,
   shorten spans (add a rib / chamfer), then re-run `check` until no `fail`
   remains. Do not "fix" a finding by lowering a limit.
4. **Export**:

   ```bash
   ttp export --lol part.lol --out ./out --format 3mf --quality high   # Bambu Studio / MakerWorld
   ttp export --lol part.lol --out ./out --format step                 # Fusion 360 / FreeCAD
   ttp export --lol part.lol --out ./out --format gcode                # direct print (H2D preset)
   ```

   The JSON `path` is the artifact. 3MF carries the DfAM summary; G-code
   carries layer count / filament / time.
5. **Validate G-code** before any printer handoff (never skip):

   ```bash
   ttp validate --gcode ./out/<id>.gcode --bed h2d      # h2d | h2d-dual | x1c | a1-mini
   ```

   `ok: true` = movement + extrusion + temperature commands present and every
   absolute X / Y / Z inside the machine's motion bounds. It is **not** proof
   the file is safe on hardware; still review profile / filament / start-end
   G-code with the user.
6. **Hand off** the validated `.gcode` path to `alice-print lan-send`
   (ALICE-Print crate, features `cli,lan-net`; dry-run by default, `--execute`
   to upload, `--confirm-start-print` to start) when the user wants to print
   over LAN, or the `.3mf` path to the user for Bambu Studio.

## Reporting

Report findings with restrained labels only: `✅ pass` / `❌ fail` /
`❓ need more info` / `ℹ advisory`. Quote the measured number and the cited
limit for every finding. Say "watertight fail" is a mesher property (not a
design error) and that slicers usually auto-repair it — do not send the user
chasing their LOL for it.

## Limits

- FDM limits are conservative published defaults (ISO/ASTM 52910 family); a
  user's machine / material datasheet overrides them — say which you used.
- `check` meshes at preview resolution; features under ~0.5 mm may be missed.
  Report that as `need more info`, not pass.
- `ttp` never uploads, slices for other printers, or starts prints.
