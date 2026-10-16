//! 読めた LOL から作った mesh が空の時は、空の mesh を黙って返さずに `EmptyMesh` の誤りにする
//!
//! fixture: `gyroid` は全空間に広がる面で、形を包む箱が無い (探索の上限 ±500 mm に張り付く)
//! この大きさ (周期 3、厚み 1) では Preview の格子が面を捉えられず mesh が空になるので、
//! 「範囲を求められない」の誤りになる (近い値の `gyroid(3, 0.5)` / `gyroid(6, 1)` も同じ)
//! 以前の fixture の `polar_repeat` は alice-sdf 6.1.0 から解析的な外接箱を持つので書き出せる
//! (`polar_repeat_is_bounded_and_exports`)

use text_to_print_core::pipeline::{
    EmptyMesh, ExportFormat, Quality, export_3mf_without_analysis, export_mesh, preview_lol_to_mesh,
};
use text_to_print_core::violation::Lang;

fn empty(e: &anyhow::Error) -> Option<EmptyMesh> {
    e.downcast_ref::<EmptyMesh>().copied()
}

#[test]
fn unbounded_shape_is_an_error_not_an_empty_mesh() {
    let lol = "gyroid(3, 1)";
    let e = preview_lol_to_mesh(lol, Quality::Preview).expect_err("空の mesh を返した");
    assert_eq!(empty(&e), Some(EmptyMesh { aabb_pinned: true }), "{e}");
    let dir = tempfile::tempdir().unwrap();
    let e = export_3mf_without_analysis(lol, dir.path(), Quality::Preview).expect_err("書き出した");
    assert_eq!(empty(&e), Some(EmptyMesh { aabb_pinned: true }));
    let e = export_mesh(lol, dir.path(), ExportFormat::ThreeMf, Quality::Preview)
        .expect_err("書き出した");
    assert!(empty(&e).is_some(), "{e}");
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        0,
        "空の 3MF を書いた"
    );
}

#[test]
fn polar_repeat_is_bounded_and_exports() {
    let lol = "polar_repeat(6, translate(8, 0, 0, sphere(1.5)))";
    let mesh = preview_lol_to_mesh(lol, Quality::Preview).expect("書き出せない");
    assert!(!mesh.indices.is_empty(), "空の mesh");
}

#[test]
fn shape_with_no_material_is_an_error() {
    // 小さい箱から大きい箱を引く = 材料が残らない
    let e = preview_lol_to_mesh("subtract(box3d(1, 1, 1), box3d(5, 5, 5))", Quality::Preview)
        .expect_err("空の mesh を返した");
    assert!(empty(&e).is_some(), "{e}");
}

#[test]
fn messages_are_plain_in_both_languages() {
    for pinned in [true, false] {
        let m = EmptyMesh {
            aabb_pinned: pinned,
        };
        let (ja, en) = (m.message(Lang::Ja), m.message(Lang::En));
        assert!(!ja.is_empty() && !en.is_empty() && ja != en);
        assert_eq!(m.to_string(), en);
        assert!(!en.contains("aabb") && !en.contains("interval"), "{en}");
    }
}

#[test]
fn ordinary_shapes_are_unaffected() {
    let m = preview_lol_to_mesh("box3d(5, 5, 2.5)", Quality::Preview).expect("box");
    assert!(!m.indices.is_empty());
}

/// 誤検出が無いこと: 正当に小さい形と、正当に大きい (探索の上限 ±500 mm 未満の) 形は mesh を作る
///
/// mesher は bounds の各軸を決まった数の格子 (Preview 96 / High 192 / Ultra 256、`Quality::mesh_resolution`)
/// に分ける = cell の大きさは形を包む箱に比例する そのため箱が形に合っていれば小さい形も大きい形も
/// 格子に乗る 空になるのは箱が形より大きく膨らんだ時 (区間評価で絞れない演算) だけ
#[test]
fn tiny_and_large_valid_shapes_are_not_empty() {
    let shapes = [
        ("box3d(0.3, 0.3, 0.3)", "0.6 mm 角の箱"),
        ("sphere(0.5)", "半径 0.5 mm の球"),
        ("box3d(450, 5, 5)", "長さ 900 mm の棒 (上限 ±500 未満)"),
        (
            "union(sphere(2), translate(480, 0, 0, sphere(2)))",
            "480 mm 離れた 2 つの球",
        ),
    ];
    let mut checked = 0;
    for (lol, what) in shapes {
        for q in [Quality::Preview, Quality::High] {
            match preview_lol_to_mesh(lol, q) {
                Ok(m) => assert!(!m.indices.is_empty(), "{what} ({q:?}) が空"),
                Err(e) => panic!("{what} ({q:?}) が誤りになった (誤検出): {e}"),
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 8);
}
