//! 読めた LOL から作った mesh が空の時は、空の mesh を黙って返さずに `EmptyMesh` の誤りにする
//!
//! fixture: `polar_repeat` は区間評価が緩く形を包む箱を絞れない (探索の上限 ±500 mm に張り付き、
//! Preview の格子が形より粗くなって mesh が空になる) その演算の区間評価が直るまで、この形は
//! 「範囲を求められない」の誤りになる 直ったら最初の試験が red になるので、別の fixture に替える

use text_to_print_core::pipeline::{
    EmptyMesh, ExportFormat, Quality, export_3mf_without_analysis, export_mesh, preview_lol_to_mesh,
};
use text_to_print_core::violation::Lang;

fn empty(e: &anyhow::Error) -> Option<EmptyMesh> {
    e.downcast_ref::<EmptyMesh>().copied()
}

#[test]
fn unbounded_shape_is_an_error_not_an_empty_mesh() {
    let lol = "polar_repeat(6, translate(8, 0, 0, sphere(1.5)))";
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
