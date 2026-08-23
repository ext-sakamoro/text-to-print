# CUSTOMIZER_HOWTO.md — 新 archetype を text-to-print customizer に追加する手順

## この doc について

text-to-print の **customizer 経路** (パラメータ入力可能な template) に新 archetype を追加する完全 step-by-step ガイド 4-layer に分けた実装 pattern + Gridfinity walkthrough + チェックリスト + 実装罠 catalog

**対象読者**: text-to-print / ALICE-LOL に新機能追加する開発者 (自分、または将来の外部貢献者)

**関連 memory**: `success_text_to_print_customizer_pattern.md` (AI 用サマリ、詳細は本 doc)

---

## 前提: 3-layer template UX 構造

text-to-print には **3 経路** の生成 flow があり、customizer 経路 (C) を拡張する:

| Layer | 経路 | 特性 | 実装場所 |
|--|--|--|--|
| **A** | 固定 preset button | click → 決定論的固定 param → 1 秒生成 | `TEMPLATE_CATEGORIES` (prompt.rs) |
| **C** | customizer slider | param 手動指定 → LOL 動的組立 → 1 秒生成 | `show_prompt_customizer()` (prompt.rs) + `CustomizerState` (state.rs) |
| **B** | LLM 自然言語 | 自由入力 → GBNF constrained LLM → 2-8 分生成、非決定 | 既存 prompt 欄 + `start_generation()` |

**新 archetype 追加 = 4 layer + 2 optional** の実装:

```
[Layer 3] text-to-print prompt.rs      ← show_xxx_customizer() 関数 + dispatch (経路 C)
              ↓
[Layer 2] text-to-print state.rs       ← XxxUiState struct + to_lol()
              ↓
[Layer 1] ALICE-LOL runtime_parser.rs  ← "xxx" => { ... } dispatch
              ↓
[Layer 0] ALICE-LOL pattern_sdf.rs     ← XxxSpec struct + xxx(&spec) -> SdfNode fn

[Layer L] text-to-print lol.gbnf       ← name_Nf に "xxx" 追加 (経路 B、LLM 自然言語)
[Layer L] text-to-print system_prompt.md ← PRODUCT SHORTCUTS section に用途例
```

**判断軸**: 「LOL DSL primitive として `xxx(...)` を書けば mesh が出る」→ Layer 0-1 実装
「UI から slider 経由で `xxx(...)` を生成したい」→ Layer 2-3 実装

Layer 0-1 だけ実装すれば LLM 経路 (B) 経由で使える (user が自然言語で「Gridfinity 3x3, 6U」と書けば LLM が `gridfinity_bin(3, 3, 6)` を吐く) UI (Layer 2-3) は経路 C の追加

---

## 実装 pattern (4-step)

### Step 1: ALICE-LOL primitive 追加 (Layer 0)

**File**: `~/ALICE-LOL/alice-lol/src/stdlib/hardsurface/pattern_sdf.rs`

**追加内容**:
- `XxxSpec` struct (dimensional param + material 非依存)
- `XxxSpec::default_yyy()` const fn (代表的 preset 1 個以上)
- `pub fn xxx(spec: &XxxSpec) -> SdfNode` (SdfNode 直接構築)
- test module に 2-3 test (`matches!` on final variant + `eval` sanity)

**Template**:

```rust
// ────────────────────────────────────────────────────────
// N. xxx (organizer-gridfinity-desk § X.Y または参照 spec 出典)
// ────────────────────────────────────────────────────────

/// XXX の寸法仕様 (説明)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct XxxSpec {
    /// param 1 (mm、default XX)
    pub param_1: f32,
    /// param 2 (mm、default YY)
    pub param_2: f32,
    // ...
}

impl XxxSpec {
    /// 代表 default (context 説明)
    #[must_use]
    pub const fn default_yyy() -> Self {
        Self {
            param_1: XX.0,
            param_2: YY.0,
        }
    }
}

/// XXX (説明、構造 outline)
///
/// 構造:
/// - Outer: `RoundedBox` (`X×Y×Z = ...`)
/// - Cavity: ...
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{xxx, XxxSpec};
/// let node = xxx(&XxxSpec::default_yyy());
/// ```
#[must_use]
pub fn xxx(spec: &XxxSpec) -> SdfNode {
    // ... SdfNode 直接構築、既存 helpers (rounded_box/box3d/cylinder/translate/union/subtract) 活用
}
```

**共通 helpers 利用可** (pattern_sdf.rs top で定義):
- `rounded_box(hx, hy, hz, r)` / `box3d(hx, hy, hz)` / `cylinder(radius, half_height)`
- `translate(child, offset)` / `union(a, b)` / `smooth_union(a, b, k)` / `subtract(a, b)`

**必須注意**:
- **`Cylinder` は Y-axis alignment** (半径は XZ 平面、高さは Y 軸方向) 「上下貫通穴」= Y-axis cylinder ✓、「前後貫通穴」= 要 rotate
- 内部 cavity 位置 offset は `translate` の第 2 引数 (Y-up 想定なら Y 方向)
- ALL param は spec に含める (material 非依存で dimensional のみ) 応力計算等は user 側で pre-compute

**test template**:
```rust
#[test]
fn xxx_default_is_subtraction() {
    let node = xxx(&XxxSpec::default_yyy());
    assert!(matches!(node, SdfNode::Subtraction { .. })); // 最終 variant で判定
}

#[test]
fn xxx_material_point_is_inside() {
    let node = xxx(&XxxSpec::default_yyy());
    // 材料内部と想定される点で eval < 0 を確認
    assert!(eval(&node, Vec3::new(x, y, z)) < 0.0);
}
```

---

### Step 2: runtime_parser dispatch (Layer 1)

**File**: `~/ALICE-LOL/alice-lol/src/runtime_parser.rs`

**追加内容**:
- `Parser::parse_expr()` の match 内に `"xxx" => { ... }` dispatch
- test module に 1-2 test (`parse_lol("xxx(...)")` + eval sanity)

**Template**:

```rust
// dispatch (Parser::parse_expr() 内)
"xxx" => {
    // xxx(param_1, param_2, ...) N param signature
    let (p1, p2) = self.parse_2f()?; // or parse_1f/3f/4f/7f
    let spec = crate::stdlib::hardsurface::pattern_sdf::XxxSpec {
        param_1: p1,
        param_2: p2,
        // その他 field は default 値
    };
    Ok(crate::stdlib::hardsurface::pattern_sdf::xxx(&spec))
}
```

**parse_Nf helper**:
- `parse_1f() -> Result<f32, ParseError>`
- `parse_2f() -> Result<(f32, f32), ParseError>`
- `parse_3f() -> Result<(f32, f32, f32), ParseError>`
- `parse_4f() -> Result<(f32, f32, f32, f32), ParseError>`
- `parse_7f() -> Result<(f32, ..., f32), ParseError>` (Phase C で追加)

**param なし版**:
```rust
"xxx" => {
    self.expect_rparen()?;
    Ok(crate::stdlib::hardsurface::pattern_sdf::xxx(
        &crate::stdlib::hardsurface::pattern_sdf::XxxSpec::default_yyy(),
    ))
}
```

**sentinel パターン** (optional param 用):
```rust
// Option<f32> を expose する場合、0 sentinel → None
let cable_hole_dia = if chd > 0.0 { Some(chd) } else { None };
// Option<(u32, u32)> なら (0, 0) sentinel → None
let dividers = if divx >= 1.0 && divy >= 1.0 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some((divx as u32, divy as u32))
} else {
    None
};
```

**f32 → u32 変換**:
```rust
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
let value = float_val as u32;
```

**test template**:
```rust
#[test]
fn test_xxx_default() {
    let node = parse_lol("xxx(param_1, param_2)").unwrap();
    assert!(matches!(node, SdfNode::Subtraction { .. }));
}

#[test]
fn test_xxx_eval_correctly() {
    use alice_sdf::eval;
    let node = parse_lol("xxx(param_1, param_2)").unwrap();
    let d = eval(&node, Vec3::new(0.1, 0.1, 0.1));
    assert!(d.is_finite(), "xxx: non-finite SDF");
}
```

**ここまで実装で LLM 経路 (B) 経由で使えるようになる** UI (Layer 2-3) は追加機能

---

### Step 3: text-to-print UiState (Layer 2)

**File**: `~/text-to-print/crates/app/src/state.rs`

**追加内容**:
- `XxxUiState` struct + `Default` + `to_lol()` method
- `CustomizerState` に field 追加
- test module import + Default sanity test

**Template**:

```rust
/// XXX customizer UI state (`xxx(param_1, param_2)`)
#[derive(Debug, Clone, Copy)]
pub struct XxxUiState {
    /// param 1 (mm、default XX、range MIN-MAX)
    pub param_1: f32,
    /// param 2 (mm、default YY、range MIN-MAX)
    pub param_2: f32,
}

impl Default for XxxUiState {
    fn default() -> Self {
        Self {
            param_1: XX.0,
            param_2: YY.0,
        }
    }
}

impl XxxUiState {
    pub fn to_lol(self) -> String {
        format!("xxx({}, {})", self.param_1, self.param_2)
    }
}
```

**CustomizerState 拡張**:
```rust
#[derive(Debug, Clone, Default)]
pub struct CustomizerState {
    // ... 既存 field
    pub xxx: XxxUiState,  // 追加
}
```

**注意**: 既存 struct literal (`GridfinityUiState { units_x: 3, ... }`) を書いた既存 test/code を壊す可能性がある `..Default::default()` を追加 or all-field 明示

**test template**:
```rust
#[test]
fn xxx_default_is_yyy() {
    let x = XxxUiState::default();
    assert!((x.param_1 - XX.0).abs() < 1e-6);
    assert!((x.param_2 - YY.0).abs() < 1e-6);
    assert_eq!(x.to_lol(), "xxx(XX, YY)");
}
```

---

### Step 4: show_xxx_customizer (Layer 3)

**File**: `~/text-to-print/crates/app/src/ui/prompt.rs`

**追加内容**:
- `show_xxx_customizer(ui, state)` 関数
- `show_prompt_customizer` から dispatch (`ui.separator(); show_xxx_customizer(ui, state);`)

**Template**:

```rust
/// XXX customizer (`param_1 × param_2 × ...`)
///
/// 説明 + default 値の意味
fn show_xxx_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🏷 XXX (説明)").strong());

    let x = &mut state.customizer_state.xxx;
    ui.horizontal(|ui| {
        ui.label("param 1 (mm):");
        ui.add(egui::Slider::new(&mut x.param_1, MIN..=MAX).step_by(STEP));
    });
    ui.horizontal(|ui| {
        ui.label("param 2 (mm):");
        ui.add(egui::Slider::new(&mut x.param_2, MIN..=MAX).step_by(STEP));
    });

    // realtime 計算値表示 (optional)
    // ui.label(format!("外形寸法: {mm:.1}mm"));

    let x_copy = *x;
    let label = format!("XXX {}×{}mm", x_copy.param_1, x_copy.param_2);
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input = format!("[customizer] {label}");
        state.prompt_focused_once = false;
        start_generation_from_lol(state, x_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}
```

**show_prompt_customizer への dispatch 追加**:
```rust
fn show_prompt_customizer(ui: &mut egui::Ui, state: &mut AppState, is_generating: bool) {
    ui.collapsing("カスタマイザー (...)", |ui| {
        ui.add_enabled_ui(!is_generating, |ui| {
            // ... 既存
            ui.separator();
            show_xxx_customizer(ui, state);  // 追加
        });
    });
}
```

**egui widget cheat sheet**:
- `Slider::new(&mut val, MIN..=MAX)` = 数値 slider (`u32`/`f32`)
- `Slider::new(...).step_by(STEP)` = f32 step (0.5 / 1.0 / 10.0 等)
- `Slider::new(...).text("suffix")` = 値横の説明
- `Checkbox::new(&mut bool, "label")` = boolean toggle
- `ui.add_enabled_ui(condition, |ui| { ... })` = condition false で grey-out
- `ui.collapsing("title", |ui| { ... })` = 折り畳み section

---

## Walkthrough: Gridfinity bin (初出、Phase 1 → C)

以下、Gridfinity bin を **完全実装** した diff の要約 (実装済み、参考として)

### Step 1: `pattern_sdf.rs` (既存 `pattern_sdf::gridfinity_bin` 参照)

- `GridfinitySpec` struct: `units_x, units_y, height_u, dividers, wall_thickness, floor_thickness` (6 field)
- `GridfinitySpec::default_2x2()` const fn = 2×2×6U 単 cavity
- `gridfinity_bin(&spec) -> SdfNode` 実装: `RoundedBox` outer - `Box3d` cavity or `Union` of `dx * dy` cavities

### Step 2: `runtime_parser.rs` dispatch

**Basic 版** (3 param):
```rust
"gridfinity_bin" => {
    let (ux, uy, hu) = self.parse_3f()?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let spec = GridfinitySpec {
        units_x: ux as u32, units_y: uy as u32, height_u: hu as u32,
        dividers: None, wall_thickness: 1.2, floor_thickness: 1.5,
    };
    Ok(gridfinity_bin(&spec))
}
```

**Advanced 版** (7 param、Phase C で追加):
```rust
"gridfinity_bin_ex" => {
    let (ux, uy, hu, divx, divy, wall, floor) = self.parse_7f()?;
    let dividers = if divx >= 1.0 && divy >= 1.0 {
        Some((divx as u32, divy as u32))
    } else { None };
    let wall_thickness = if wall > 0.0 { wall } else { 1.2 };
    let floor_thickness = if floor > 0.0 { floor } else { 1.5 };
    // ... spec 構築 + gridfinity_bin 呼び出し
}
```

### Step 3: `state.rs` `GridfinityUiState`

- Basic 3 field (units_x/units_y/height_u) + Advanced 5 field (use_dividers/dividers_x/y/wall/floor)
- `to_lol()` が use_dividers or 非 default wall/floor 検出時に `gridfinity_bin_ex` を出力、それ以外は basic

### Step 4: `prompt.rs` `show_gridfinity_customizer`

- 3 slider (units_x, units_y, height_u)
- 折り畳み「詳細設定」内に advanced (checkbox + dividers slider + wall/floor slider)
- realtime 外形寸法表示 (`{ext_x_mm:.1} × {ext_y_mm:.1} × {ext_h_mm:.1}mm`)
- click → `start_generation_from_lol`

---

### Step 5 (optional): GBNF grammar 追加 (Layer L、LLM 経路 B 対応)

`~/text-to-print/crates/llm/src/lol.gbnf` の適切な category に primitive 名を追加:

```gbnf
# param 数に応じて追加先が変わる
name_1f ::= ... | "xxx"    # 1 param
name_2f ::= ... | "xxx"    # 2 param
name_3f ::= ... | "xxx"    # 3 param
name_7f ::= ... | "xxx"    # 7 param (gridfinity_bin_ex 等)
name_no_arg ::= ... | "xxx"  # 0 param (preset shortcut)
```

**test 追加** (`~/text-to-print/crates/llm/src/grammar_lol.rs`):
```rust
// lol_gbnf_includes_high_level_primitives の must_have に "xxx" 追加
```

### Step 6 (optional): system_prompt.md に SHORTCUT 例追加 (LLM 用途学習)

`~/text-to-print/crates/llm/src/system_prompt.md` の PRODUCT SHORTCUTS section table に 1 行追加:
```
| user says | LOL DSL | 説明 |
|--|--|--|
| ○○ / xxx | `xxx(p1, p2, p3)` | (default 値の説明) |
```

**test 追加** (`~/text-to-print/crates/llm/src/prompt.rs`):
```rust
// system_prompt_teaches_high_level_shortcuts の array に "xxx" 追加
```

**Step 5-6 の効果**: user が「○○ 作って」と自然言語 (経路 B) 入力時に LLM が `xxx(...)` を出せるようになる

**Step 5-6 skip した場合**: 経路 A/C (preset/customizer) のみ有効、LLM 経路では基礎 primitive で組み立てる (LLM が新 archetype を知らない)

---

## 追加チェックリスト (新 archetype 実装前に self-check)

- [ ] 座標系 (Y-up? Z-up?) を決めた既存 codebase は Y-up (cylinder native) 混在許容
- [ ] `Cylinder` を使う場合、Y-axis alignment を意識 (offset 方向 / test 点)
- [ ] Optional param 用に sentinel (`0.0` = None) を採用するか判断
- [ ] `parse_Nf` helper が既存 (N=1,2,3,4,7) 未対応 N は追加
- [ ] Spec struct は dimensional param のみ (material / stress 計算は user 側)
- [ ] `#[must_use] pub fn xxx()` に doctest 追加
- [ ] test 2 個 (variant 判定 + eval sanity)
- [ ] `f32 as u32` 変換で `#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]`
- [ ] `..Default::default()` を既存 struct literal 使用 test に追加 (field 追加時のみ)
- [ ] CustomizerState に new field 追加後、text-to-print state.rs test module import 更新
- [ ] `show_prompt_customizer` に `ui.separator(); show_xxx_customizer(ui, state);` dispatch 追加
- [ ] slider の `MIN..=MAX` range は spec doc の実用範囲に合わせる (organizer-gridfinity-desk 等)
- [ ] realtime 計算値表示 (外形寸法 / 収納枚数目安 等) を label で出す
- [ ] `cargo test --workspace --exclude text-to-print-worker` 全 pass
- [ ] `cargo clippy --package text-to-print --tests -- -D warnings` 0 warning
- [ ] `cargo fmt --check` clean
- [ ] ALICE-LOL 側は `cargo test --lib` + `cargo clippy --lib --tests`
- [ ] Bambu H2D 単一プリント想定なら UI slider max ≤ 280mm (bed 315mm 内)
- [ ] **LLM 経路 B にも対応させたい場合**: `lol.gbnf` + `system_prompt.md` 更新 (Step 5-6)
- [ ] GBNF 変更後: `cargo test --package text-to-print-llm grammar_lol` で GBNF syntax pass 確認

---

## 罠 catalog (実装で踏んだ、再発予防)

### 1. Cylinder Y-axis alignment (最重要、Phase B で pen_cup fix 事案)

`SdfNode::Cylinder { radius, half_height }` は **Y-axis alignment**:
- 半径 = XZ 平面での距離 (`length(point.xz)`)
- 高さ = Y 軸方向 (`abs(point.y)`)

Z-axis と勘違いすると `translate` offset が誤方向 (Z ではなく Y にすべき)、mesh 破綻・test fail

**対策**:
- Cylinder-based archetype (pen_cup / monitor_riser の cable hole 等) は **Y-up で統一**
- Z-up 世界で使いたい時は `SdfNode::Rotate { rotation: Quat::from_rotation_x(PI/2) }` で XY→XZ に回転 (or `box3d` 経路で回避、rectangular hole が許容できるなら)
- 参考: `~/ALICE-SDF/src/compiled/transpiler_common.rs` の `SdfNode::Cylinder` 実装

### 2. `clippy::similar_names` (pedantic のみ、既存 style)

`base_hx / base_hy / base_hz` パターンは `clippy::similar_names` 警告を出すが、`pattern_sdf.rs` codebase 全体の既存 style (`tray_hx / tray_hy / tray_hz`、`bp_hx / bp_hy / bp_hz` 等) 既存 style に従うので新 archetype でも許容

**default `-D warnings` (pedantic なし) では通る** pedantic を有効化する CI では `#[allow(clippy::similar_names)]` を fn 上に付ける手もあるが、既存関数と一貫させるなら省略でよい

### 3. Box3d 内部 cavity 位置の off-by-one (business_card_holder test で hit)

`inner_hx = 45.5` `outer_hx = 47.0` の場合、wall は `45.5 < |x| < 47.0` にある `x = 45` はまだ cavity 内、wall テスト点は `x = 46` 等 wall 中央を選ぶ (テスト用の material 判定は wall の真ん中で)

### 4. `..Default::default()` の必要性 (field 追加時)

新 field 追加時、既存 struct literal (`GridfinityUiState { units_x: 3, units_y: 4, height_u: 6 }`) は全 field 埋めていないと E0063 error

既存 test を書き換える時は:
```rust
let g = GridfinityUiState {
    units_x: 3,
    units_y: 4,
    height_u: 6,
    ..Default::default()  // ← 追加
};
```

or all-field 明示 (advanced field も全部書く)

---

## 既存 52 archetype 一覧 (2026-08-24 Sprint 17 更新)

**organizer-gridfinity-desk PART 1 + PART 2 完全 cover + household 3 + hobby-diy 4 + tools 3 + electronics 3 + bathroom-garage 3 + kitchen 3 + printer 3 + drawer-wall 3 + mix 3 + mix2 3 + mix3 3 + mix4 3 + mix5 3 + mix6 3 archetype** (各既実装 category 残 mix 継続)

| # | LOL DSL | basic param | default 値 | 出典 § |
|--|--|--|--|--|
| 1 | `gridfinity_bin(ux, uy, hu)` | 3 | 2×2×6U (84×84×46mm) | PART 1 全体 |
| 1' | `gridfinity_bin_ex(ux, uy, hu, divx, divy, wall, floor)` | 7 | above + `dividers=None`, `wall=1.2`, `floor=1.5` | PART 1 advanced |
| 2 | `sticky_note_holder(pad_w, pad_d, height)` | 3 | 76×76×30mm | § 2.7 |
| 3 | `business_card_holder(card_w, card_h, slot_thickness)` | 3 | 91×55×22mm (JP meishi) | § 2.6 |
| 4 | `pen_cup(inner_dia, height)` | 2 | Ø75×100mm | § 2.2 |
| 5 | `phone_stand(slot_w, back_h, cable_dia)` | 3 | 14/100/18mm | § 2.9 |
| 6 | `headphone_holder(arm_length, headband_width, mount_width)` | 3 | 80/50/100mm | § 2.5 |
| 7 | `under_desk_mount(desk_thickness, clamp_width, screw_dia)` | 3 | 25/40/4mm | § 2.4 |
| 8 | `desk_shelf(shelf_width, shelf_depth, leg_height)` | 3 | 400×200×100mm | § 2.3 |
| 9 | `monitor_riser(width, depth, height)` | 3 | 250×180×90mm | § 2.1 (簡易版) |
| 10 | `coaster(diameter, thickness)` | 2 | Ø95×5mm (round) | household § 7 |
| 11 | `tissue_box_cover(internal_l, internal_w, internal_h)` | 3 | 231×116×53mm (US rect) | household § 1 |
| 12 | `storage_box(internal_l, internal_w, internal_h)` | 3 | 150×100×60mm (medium、基本形 lid なし) | household § 3 |
| 13 | `cable_clip(cable_diameter, length)` | 2 | Ø7×L28mm (HDMI) | hobby-diy § 2 |
| 14 | `led_channel(strip_width, length)` | 2 | 10×300mm (WS2812B) | hobby-diy § 3 |
| 15 | `card_tray(card_w, card_h, depth)` | 3 | 63×88×30mm (Poker、finger notch r=9) | hobby-diy § 6 |
| 16 | `token_well(dia, depth, count)` | 3 | Ø20×深20mm × 4 (dice) | hobby-diy § 6 |
| 17 | `wrench_holder(min_mm, max_mm, count)` | 3 | 8-19mm × 6 (Metric、等間隔補間) | tools § 1 |
| 18 | `socket_rail(post_dia, post_height, count)` | 3 | Ø12.4×H22mm × 6 (1/2" drive) | tools § 2 |
| 19 | `hex_bit_holder(rows, cols, spacing)` | 3 | 5×4 grid @ 12mm (20 hole、1/4" bit 固定) | tools § 3 |
| 20 | `raspi_case(pcb_w, pcb_d, internal_h)` | 3 | 85×56×25mm (RPi 5、Active Cooler、4 standoff + port opening) | electronics § 1 |
| 21 | `esp32_enclosure(pcb_w, pcb_d, internal_h)` | 3 | 51.6×28.4×15mm (ESP32 DevKit V1、USB opening) | electronics § 2 |
| 22 | `battery_18650_holder(count, wall, floor)` | 3 | 4 cell × 2.5mm wall × 0 floor (through、Ø18.6×L68 固定) | electronics § 3 |
| 23 | `toothbrush_holder(count, hole_diameter, height)` | 3 | 4本 × Ø15×H70mm (manual、top 開口) | bathroom § 7.1 |
| 24 | `drill_bit_holder(min_mm, max_mm, count)` | 3 | 3-13mm × 11 hole (Metric、hole 円形、linear interp) | garage § 8.1 |
| 25 | `pliers_rack(slot_count, slot_width, slot_depth)` | 3 | 6 slot × W15×D60mm (combi pliers) | garage § 8.4 |
| 26 | `spice_rack(count, jar_diameter, jar_height)` | 3 | 6 jars × Ø48×H100mm (std spice、shelf + recess + lip) | kitchen § 6.1 |
| 27 | `egg_tray(rows, cols, cup_depth)` | 3 | 4×3 × 深18mm (12-egg standard、cup Ø40 固定、2D grid) | kitchen § 6.5 |
| 28 | `utensil_caddy(count, compartment_dia, height)` | 3 | 4 compartment × Ø65×H130mm (spatula/ladle/whisk/tongs) | kitchen § 6.8 |
| 29 | `filament_spool_holder(spool_od, spool_width, bore_dia)` | 3 | 1kg Ø200×W68×bore52mm (base plate + 垂直 peg、Z-up direct) | printer § 9.1 |
| 30 | `nozzle_holder(count, hole_diameter, depth)` | 3 | 8 hole × Ø8×D6mm (E3D V6 / Bambu M6) | printer § 9.5 |
| 31 | `build_plate_rack(slot_count, slot_spacing, height)` | 3 | 5 slot × spacing 15 × H200mm (build plate 5mm 厚、Ender/Bambu 235) | printer § 9.6 |
| 32 | `cutlery_tray(slot_count, slot_width, slot_length)` | 3 | 3 slot × W35 × L220mm (fork/knife/spoon、drawer 引き出し) | drawer § 3.2 |
| 33 | `pill_organizer(rows, cols, cell_size)` | 3 | 7×2 × cell 20mm (weekly AM/PM 14 cell、2D grid rect) | drawer § 3.6 |
| 34 | `magnetic_strip(magnet_count, magnet_diameter, spacing)` | 3 | 8 magnet × Ø6 × 30mm spacing (kitchen knife rail) | wall § 4.6 |
| 35 | `hairdryer_holder(barrel_diameter, holster_depth, wall_thickness)` | 3 | Ø85 × D110mm (Dyson Supersonic、大径 cylindrical holster) | bathroom § 7.7 |
| 36 | `kcup_holder(rows, cols, capsule_diameter)` | 3 | 4×3 = 12 × Ø53mm (K-Cup standard、2D grid) | kitchen § 6.7 |
| 37 | `hex_key_holder(count, min_key_mm, max_key_mm)` | 3 | 9-piece 1.5-10mm Metric (block-style Allen key、linear interp) | garage § 8.2 |
| 38 | `wrap_holder(roll_diameter, roll_width, wall_thickness)` | 3 | Ø55 × W305mm (12" foil、**新 pattern: 半円 cradle**) | kitchen § 6.2 |
| 39 | `sock_divider(cell_count, cell_width, height)` | 3 | 4 cell × W80×H89mm (**新 pattern: frame + partitions**) | drawer § 3.7 |
| 40 | `soap_tray(tray_length, tray_width, drain_slot_count)` | 3 | L200×W90 × 6 drain (dual-bottle、tray + drain slots) | bathroom § 7.3 |

**Bamboo canonical 既存 4 archetype** (PART 1 系、pattern_sdf.rs 既存)

| # | LOL DSL | param | 出典 |
|--|--|--|--|
| 10 | `wall_hook()` | 0 (spec default) | Bamboo `generators/hook.rs` |
| 11 | `drawer_organizer()` | 0 (spec default) | Bamboo `generators/drawer.rs` |
| 12 | `shelf_divider()` | 0 (spec default = 560×250×120 field-tested) | Bamboo `generators/shelf_divider.rs` |

**thin_sdf / skadis_sdf 既存 primitive** (別 module、customizer 未接続、preset button 経由)

- `shopping_cart_coin(dia, thickness)`, `skadis_panel(size, thickness, corner_r)`, `skadis_hook_{s,j,l}()`, `skadis_container()`, `skadis_clip()`, `skadis_shelf()`, `skadis_elastic_cord()`

---

## 次の archetype 候補 (未実装)

**ALICE-Bamboo/docs/patterns/** 他 doc に基づく候補:

- ~~household.md~~ ✅ 3 archetype 完了 (Sprint 4、coaster / tissue_box_cover / storage_box) 残 = lid + hinge (Print-in-Place / filament pin / living hinge) は future sprint
- ~~hobby-diy.md~~ ✅ 4 archetype 完了 (Sprint 5、cable_clip / led_channel / card_tray / token_well) 残 = gear (§ 4、involute 歯型 SDF) / bearing_mount (§ 4、608 press-fit) / model train scale accessory (§ 5) は future sprint
- ~~tools.md~~ ✅ 3 archetype 完了 (Sprint 6、wrench_holder / socket_rail / hex_bit_holder) 残 = caliper_holder (§ 4、shape 特化) / vise (§ 5、thread SDF) / battery_holder (§ 6、type 別 dims) は future sprint
- ~~electronics-enclosure.md~~ ✅ 3 archetype 完了 (Sprint 7、raspi_case / esp32_enclosure / battery_18650_holder) 残 = phone_dock (§ 4、MagSafe/USB-C shape 特化) / outdoor_enclosure (§ 5、IP sealing + gasket groove) / led_hub_box (§ 6、antenna keep-out + light pipe) は future sprint
- ~~organizer-bathroom-garage.md~~ ✅ 3 archetype 完了 (Sprint 8、toothbrush_holder / drill_bit_holder / pliers_rack) 残 = bathroom 7 (razor / soap_tray / shower_caddy / towel_hook / cotton_dispenser / hairdryer / tp_holder) + garage 6 (hex_key / tape_dispenser / driver_rack / sandpaper / brush / clamp_rack) は future sprint
- ~~organizer-cable-kitchen.md~~ ✅ 3 archetype 完了 (Sprint 9、spice_rack / egg_tray / utensil_caddy) 残 = cable § 5 は既存 hobby-diy cable_clip と重複 skip、kitchen § 6 残 6 (wrap_holder / bag_clip_org / can_rack / cutting_board_rack / kcup_holder / sink_caddy) は future sprint
- ~~organizer-printer-modular.md~~ ✅ 3 archetype 完了 (Sprint 10、filament_spool_holder / nozzle_holder / build_plate_rack) 残 = § 9 残 5 (dry_box / tool_holder / sd_card_holder / swatch_holder / allen_key_holder) + § 10 modular connection systems (Gridfinity/Multiboard/SKADIS/Honeycomb/Lego/Dovetail/T-Slot 等は protocol 仕様書、既存 SKADIS panel / gridfinity_bin で cover 済) は future sprint
- ~~organizer-drawer-wall.md~~ ✅ 3 archetype 完了 (Sprint 11、cutlery_tray / pill_organizer / magnetic_strip) 残 = drawer 4 (chopstick_holder / jewelry 4 sub / makeup / sock_divider) + wall 6 (SKADIS/Multiboard/French Cleat/Pegboard/T-Slot/Over-Door) は future sprint (§ 4 は protocol 仕様 系が多い)
- ~~**Sprint 12 ミックス**~~ ✅ 3 archetype 完了 (bathroom § 7.7 hairdryer_holder + kitchen § 6.7 kcup_holder + garage § 8.2 hex_key_holder) 各既実装 category の残から pick、toys-articulated.md は Print-in-Place SDF が複雑すぎるため skip
- ~~**Sprint 13 ミックス 2**~~ ✅ 3 archetype 完了 (kitchen § 6.2 wrap_holder + drawer § 3.7 sock_divider + bathroom § 7.3 soap_tray) **新 pattern 2 個**: 半円 cradle (X-axis cyl rotate + subtract from top) + frame with partitions (outer - cavity + partition wall union)
- **organizer-drawer-wall.md** (406 行): カトラリー / 箸 / ジュエリー / 化粧品 / SKADIS / Multiboard → 追加 SKADIS accessory / Multiboard 互換
- **organizer-cable-kitchen.md** (374 行): ケーブル / ルーター / スパイスラック / K-Cup / 卵ホルダー → `spice_rack` / `kcup_holder` / `egg_holder`
- **organizer-bathroom-garage.md** (507 行): 歯ブラシ / 電動歯ブラシ / トイレットペーパー / ドライヤー / ドリルビット / ソケットレール → `toothbrush_holder` / `drill_bit_organizer`
- **organizer-printer-modular.md** (475 行): 3D プリンタ周辺 / モジュラー接続 10 系統 → `filament_spool_holder` / `nozzle_organizer`
- **toys-articulated.md** (325 行): 関節ドラゴン / Print-in-Place / BJD / フィジェット → 別種 primitive 群 (Print-in-Place = hinge + snap-fit、既存 pattern と全く別 domain)

**優先度判断軸**:
- MakerWorld ダウンロード実績あり (人気 archetype)
- 単純な box/cylinder 組み合わせで実装可能 (Print-in-Place / articulated は高難度)
- ALICE-Bamboo に既存 canonical 実装が近い (spec doc + pattern_scores.json)

**次 sprint 推奨**: 各既実装 category の残 archetype ミックス継続 (Sprint 13+) 高難度候補 (skip 済): caliper_holder / vise / battery_holder / phone_dock / gear / shower_caddy / tape_dispenser / cutting_board_rack / dry_box / makeup_organizer / toys-articulated (Print-in-Place 全般) (individually 単発 sprint、SDF marching cubes 制約で customizer 1 primitive 困難)

---

## 参考リンク

- **canonical spec 出典**: `~/ALICE-Bamboo/docs/patterns/*.md` (10 file、4,594 行)
- **canonical scores**: `~/ALICE-Bamboo/pattern_scores.json`
- **既存 pattern_sdf 実装**: `~/ALICE-LOL/alice-lol/src/stdlib/hardsurface/pattern_sdf.rs`
- **既存 runtime_parser dispatch**: `~/ALICE-LOL/alice-lol/src/runtime_parser.rs` (Phase 5.1 + Phase B/B2 sections)
- **CustomizerState + UI**: `~/text-to-print/crates/app/src/state.rs` + `crates/app/src/ui/prompt.rs`
- **project 軸**: `~/text-to-print/CLAUDE.md` § 「絶対規定 3 つ」 + 「anti-pattern 5 個」
- **ALICE 三相原理**: `~/CLAUDE.md` § 「ALICE 三相原理 (Data → Law → Intent)」 (customizer 経路 = Phase 2 Law)
