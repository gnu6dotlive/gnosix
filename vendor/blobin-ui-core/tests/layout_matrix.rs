use blobin_ui_core::layout::{
    compute, Axis, Insets, LayoutError, LayoutStyles, Length, Rect, Size, Style, Viewport,
};
use blobin_ui_core::{NodeId, NodeProperties, Role, SurfaceId, UiTree};

fn p(s: &str) -> NodeProperties {
    NodeProperties::new(Role::Container, s)
}
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.002, "{a} != {b}");
}
fn fixture() -> (UiTree, SurfaceId, NodeId, NodeId, NodeId, LayoutStyles) {
    let mut tree = UiTree::new();
    let surface = SurfaceId(7);
    let mut ids = Vec::new();
    tree.transact(|tx| {
        tx.open_surface(surface)?;
        let root = tx.append_root(surface, p("root"))?;
        ids.push(root);
        ids.push(tx.append_child(root, p("rail"))?);
        ids.push(tx.append_child(root, p("work"))?);
        Ok(())
    })
    .unwrap();
    let mut styles = LayoutStyles::new();
    let mut root_style = Style::fill();
    root_style.axis = Axis::Row;
    root_style.gap = 8.0;
    root_style.padding = Insets::uniform(12.0);
    root_style.clip_children = true;
    styles.set(&tree.snapshot(), ids[0], root_style).unwrap();
    let mut rail = Style {
        width: Length::Px(180.0),
        ..Style::default()
    };
    rail.min.width = 120.0;
    rail.flex_shrink = 1.0;
    styles.set(&tree.snapshot(), ids[1], rail).unwrap();
    let mut work = Style {
        width: Length::Px(300.0),
        ..Style::default()
    };
    work.min.width = 160.0;
    work.flex_grow = 1.0;
    work.flex_shrink = 1.0;
    styles.set(&tree.snapshot(), ids[2], work).unwrap();
    (tree, surface, ids[0], ids[1], ids[2], styles)
}
fn vp(w: f32, h: f32, dpr: f32, epoch: u64) -> Viewport {
    Viewport::new(Size::new(w, h), dpr, epoch)
}

#[test]
fn row_layout_flex_grow_exact_and_padding_oracle() {
    let (tree, surface, root, rail, work, styles) = fixture();
    let l = compute(&tree.snapshot(), &styles, surface, vp(800.0, 400.0, 1.0, 1)).unwrap();
    let a = l.get(rail).unwrap().rect;
    let b = l.get(work).unwrap().rect;
    assert_eq!(l.get(root).unwrap().rect, Rect::new(0.0, 0.0, 800.0, 400.0));
    assert_eq!(a, Rect::new(12.0, 12.0, 180.0, 376.0));
    assert_eq!(b, Rect::new(200.0, 12.0, 588.0, 376.0));
    assert_eq!(l.paint_order, vec![root, rail, work]);
    assert_eq!(l.get(work).unwrap().physical.width, 588);
    assert!(l.get(rail).unwrap().visible.contains(12.0, 13.0));
    assert!(!l.get(rail).unwrap().visible.contains(192.0, 13.0));
}

#[test]
fn shrink_respects_minima_and_clips_excess() {
    let (tree, surface, root, rail, work, styles) = fixture();
    let small = compute(&tree.snapshot(), &styles, surface, vp(400.0, 200.0, 1.0, 1)).unwrap();
    close(small.get(rail).unwrap().rect.width, 138.0);
    close(small.get(work).unwrap().rect.width, 230.0);
    let tiny = compute(&tree.snapshot(), &styles, surface, vp(280.0, 200.0, 1.0, 2)).unwrap();
    assert_eq!(tiny.get(rail).unwrap().rect.width, 120.0);
    assert_eq!(tiny.get(work).unwrap().rect.width, 160.0);
    let work_box = tiny.get(work).unwrap();
    assert!(work_box.visible.width < work_box.rect.width);
    assert!(work_box.visible.x + work_box.visible.width <= tiny.get(root).unwrap().rect.width);
}

#[test]
fn pixel_coverage_uses_rounded_edges_not_rounded_dimensions() {
    let mut tree = UiTree::new();
    let surface = SurfaceId(3);
    let mut ids = Vec::new();
    tree.transact(|tx| {
        tx.open_surface(surface)?;
        ids.push(tx.append_root(surface, p("root"))?);
        ids.push(tx.append_child(ids[0], p("box"))?);
        Ok(())
    })
    .unwrap();
    let mut styles = LayoutStyles::new();
    let mut root = Style::fill();
    root.padding = Insets::uniform(3.25);
    styles.set(&tree.snapshot(), ids[0], root).unwrap();
    let mut child = Style {
        width: Length::Px(10.25),
        ..Style::default()
    };
    child.height = Length::Px(20.5);
    styles.set(&tree.snapshot(), ids[1], child).unwrap();
    for dpr in [1.0, 1.25, 1.5, 2.0, 2.75] {
        let l = compute(&tree.snapshot(), &styles, surface, vp(100.0, 60.0, dpr, 1)).unwrap();
        let v = l.get(ids[1]).unwrap();
        assert_eq!(v.rect, Rect::new(3.25, 3.25, 10.25, 20.5));
        let expect_left = (3.25 * dpr).floor() as i32;
        let expect_right = (13.5 * dpr).ceil() as i32;
        let expect_top = (3.25 * dpr).floor() as i32;
        let expect_bottom = (23.75 * dpr).ceil() as i32;
        assert_eq!(v.physical.x, expect_left);
        assert_eq!(v.physical.y, expect_top);
        assert_eq!(v.physical.width, (expect_right - expect_left) as u32);
        assert_eq!(v.physical.height, (expect_bottom - expect_top) as u32);
        assert!(v.physical.contains(expect_left, expect_top));
        assert!(!v.physical.contains(expect_right, expect_top));
    }
}

#[test]
fn nested_clip_applies_to_overflow_but_does_not_clip_unrelated_sibling() {
    let mut tree = UiTree::new();
    let surface = SurfaceId(2);
    let mut ids = Vec::new();
    tree.transact(|tx| {
        tx.open_surface(surface)?;
        let root = tx.append_root(surface, p("root"))?;
        let frame = tx.append_child(root, p("frame"))?;
        let outside = tx.append_child(root, p("sibling"))?;
        let inner = tx.append_child(frame, p("inner"))?;
        let deep = tx.append_child(inner, p("deep"))?;
        ids = vec![root, frame, outside, inner, deep];
        Ok(())
    })
    .unwrap();
    let mut styles = LayoutStyles::new();
    let mut root = Style::fill();
    root.axis = Axis::Row;
    root.gap = 6.0;
    styles.set(&tree.snapshot(), ids[0], root).unwrap();
    let mut frame = Style {
        width: Length::Px(100.0),
        ..Style::default()
    };
    frame.height = Length::Px(60.0);
    frame.clip_children = true;
    styles.set(&tree.snapshot(), ids[1], frame).unwrap();
    let mut sibling = Style {
        width: Length::Px(40.0),
        ..Style::default()
    };
    sibling.height = Length::Px(40.0);
    styles.set(&tree.snapshot(), ids[2], sibling).unwrap();
    let mut inner = Style {
        width: Length::Px(150.0),
        ..Style::default()
    };
    inner.height = Length::Px(100.0);
    inner.clip_children = false;
    styles.set(&tree.snapshot(), ids[3], inner).unwrap();
    let mut deep = Style {
        width: Length::Px(130.0),
        ..Style::default()
    };
    deep.height = Length::Px(70.0);
    styles.set(&tree.snapshot(), ids[4], deep).unwrap();
    let result = compute(&tree.snapshot(), &styles, surface, vp(300.0, 120.0, 1.0, 1)).unwrap();
    assert_eq!(result.get(ids[3]).unwrap().rect.width, 150.0);
    assert_eq!(result.get(ids[3]).unwrap().visible.width, 100.0);
    assert_eq!(result.get(ids[4]).unwrap().visible.width, 100.0);
    assert_eq!(result.get(ids[4]).unwrap().visible.height, 60.0);
    assert_eq!(result.get(ids[2]).unwrap().visible.width, 40.0);
    assert_eq!(result.get(ids[2]).unwrap().rect.x, 106.0);
}

#[test]
fn scale_resize_style_revision_and_tree_revision_reject_stale_geometry() {
    let (mut tree, surface, _, rail, _, mut styles) = fixture();
    let original = vp(720.0, 480.0, 1.25, 4);
    let l = compute(&tree.snapshot(), &styles, surface, original).unwrap();
    assert!(l.is_current(&tree.snapshot(), &styles, original));
    assert!(!l.is_current(&tree.snapshot(), &styles, vp(720.0, 480.0, 1.5, 4)));
    assert!(!l.is_current(&tree.snapshot(), &styles, vp(720.0, 480.0, 1.25, 5)));
    assert!(!l.is_current(&tree.snapshot(), &styles, vp(721.0, 480.0, 1.25, 4)));
    let old = tree.snapshot();
    tree.transact(|tx| tx.set_properties(rail, p("rail changed")))
        .unwrap();
    assert!(!l.is_current(&tree.snapshot(), &styles, original));
    assert!(l.is_current(&old, &styles, original));
    let newer = compute(&tree.snapshot(), &styles, surface, original).unwrap();
    let next = Style {
        width: Length::Px(120.0),
        ..Style::default()
    };
    styles.set(&tree.snapshot(), rail, next).unwrap();
    assert!(!newer.is_current(&tree.snapshot(), &styles, original));
}

#[test]
fn separate_surface_style_and_tree_updates_do_not_invalidate_other_window() {
    let (mut tree, a, _, rail, _, mut styles) = fixture();
    let b = SurfaceId(800);
    let mut root_b = None;
    tree.transact(|tx| {
        tx.open_surface(b)?;
        root_b = Some(tx.append_root(b, p("second window"))?);
        Ok(())
    })
    .unwrap();
    styles
        .set(&tree.snapshot(), root_b.unwrap(), Style::fill())
        .unwrap();
    let viewport = vp(640.0, 480.0, 1.5, 12);
    let b_layout = compute(&tree.snapshot(), &styles, b, viewport).unwrap();
    let a_layout = compute(&tree.snapshot(), &styles, a, viewport).unwrap();
    tree.transact(|tx| tx.set_properties(rail, p("updated only A")))
        .unwrap();
    let rail_style = Style {
        width: Length::Px(200.0),
        ..Style::default()
    };
    styles.set(&tree.snapshot(), rail, rail_style).unwrap();
    assert!(b_layout.is_current(&tree.snapshot(), &styles, viewport));
    assert!(!a_layout.is_current(&tree.snapshot(), &styles, viewport));
    assert_eq!(b_layout.get(root_b.unwrap()).unwrap().rect.width, 640.0);
}

#[test]
fn invalid_style_viewports_and_unknown_nodes_reject_without_committing_styles() {
    let (mut tree, surface, root, _, _, mut styles) = fixture();
    let before = styles.revision(surface);
    let invalid = Style {
        gap: f32::NAN,
        ..Style::default()
    };
    assert_eq!(
        styles.set(&tree.snapshot(), root, invalid).unwrap_err(),
        LayoutError::InvalidStyle
    );
    let mut invalid = Style::default();
    invalid.min.width = 200.0;
    invalid.max.width = 100.0;
    assert_eq!(
        styles.set(&tree.snapshot(), root, invalid).unwrap_err(),
        LayoutError::InvalidStyle
    );
    let invalid = Style {
        width: Length::Fraction(1.1),
        ..Style::default()
    };
    assert_eq!(
        styles.set(&tree.snapshot(), root, invalid).unwrap_err(),
        LayoutError::InvalidStyle
    );
    assert_eq!(styles.revision(surface), before);
    for view in [
        vp(f32::NAN, 100.0, 1.0, 1),
        vp(-1.0, 100.0, 1.0, 1),
        vp(100.0, 100.0, 0.0, 1),
        vp(100.0, 100.0, f32::INFINITY, 1),
    ] {
        assert_eq!(
            compute(&tree.snapshot(), &styles, surface, view).unwrap_err(),
            LayoutError::InvalidViewport
        );
    }
    assert_eq!(
        compute(
            &tree.snapshot(),
            &styles,
            SurfaceId(999),
            vp(100.0, 100.0, 1.0, 1)
        )
        .unwrap_err(),
        LayoutError::UnknownSurface(SurfaceId(999))
    );
    tree.transact(|tx| tx.remove(root)).unwrap();
    assert_eq!(
        styles.remove(&tree.snapshot(), root).unwrap_err(),
        LayoutError::UnknownNode(root)
    );
}

#[test]
fn layout_is_deterministic_across_resolution_and_dpr_matrix() {
    let (tree, surface, root, rail, work, styles) = fixture();
    for width in [320.0, 400.0, 512.0, 640.0, 1280.0, 1920.0] {
        for height in [240.0, 384.0, 720.0, 1080.0] {
            for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
                let view = vp(width, height, scale, 1);
                let a = compute(&tree.snapshot(), &styles, surface, view).unwrap();
                let b = compute(&tree.snapshot(), &styles, surface, view).unwrap();
                assert_eq!(a.paint_order, b.paint_order);
                assert_eq!(a.boxes, b.boxes);
                let rail_rect = a.get(rail).unwrap();
                let work_rect = a.get(work).unwrap();
                close(rail_rect.rect.x, 12.0);
                close(
                    work_rect.rect.x,
                    rail_rect.rect.x + rail_rect.rect.width + 8.0,
                );
                assert!(work_rect.rect.width >= 160.0);
                assert_eq!(a.get(root).unwrap().rect.width, width);
                assert_eq!(a.stamp.viewport_bits[2], scale.to_bits());
            }
        }
    }
}

#[test]
fn independent_analytic_oracle_for_fixed_plus_one_growing_child_512_cases() {
    let (tree, surface, _, rail, work, styles) = fixture();
    // With >= 512 viewport width: available main = width - (2*12) - 8;
    // fixed rail = 180; baseline body = 300; body alone absorbs all slack.
    // This is a separate arithmetic oracle, not a call into layout internals.
    let mut seed = 0xA726_0345_u64;
    for _ in 0..512 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let width = 512.0 + ((seed % 10000) as f32) * 0.125;
        let dpr = [1.0, 1.25, 1.5, 2.0][(seed % 4) as usize];
        let result = compute(&tree.snapshot(), &styles, surface, vp(width, 600.0, dpr, 1)).unwrap();
        close(result.get(rail).unwrap().rect.width, 180.0);
        close(result.get(work).unwrap().rect.width, width - 212.0);
        close(result.get(work).unwrap().rect.x, 200.0);
        let body = result.get(work).unwrap();
        let left = (200.0 * dpr).floor() as i32;
        let right = ((width - 12.0) * dpr).ceil() as i32;
        assert_eq!(body.physical.x, left);
        assert_eq!(body.physical.width, (right - left) as u32);
    }
}

#[test]
fn closing_and_reopening_a_surface_invalidates_previous_layout_epoch() {
    let (mut tree, surface, _, _, _, styles) = fixture();
    let viewport = vp(640.0, 480.0, 1.0, 1);
    let before = compute(&tree.snapshot(), &styles, surface, viewport).unwrap();
    tree.transact(|tx| tx.close_surface(surface)).unwrap();
    assert!(!before.is_current(&tree.snapshot(), &styles, viewport));
    tree.transact(|tx| tx.open_surface(surface)).unwrap();
    assert!(!before.is_current(&tree.snapshot(), &styles, viewport));
    let after = compute(&tree.snapshot(), &styles, surface, viewport).unwrap();
    assert_ne!(before.stamp.surface_epoch, after.stamp.surface_epoch);
}

#[test]
fn column_fractional_height_and_flex_growth_use_parent_content_box() {
    let mut tree = UiTree::new();
    let s = SurfaceId(77);
    let mut ids = Vec::new();
    tree.transact(|tx| {
        tx.open_surface(s)?;
        let root = tx.append_root(s, p("column"))?;
        ids.push(root);
        ids.push(tx.append_child(root, p("top"))?);
        ids.push(tx.append_child(root, p("bottom"))?);
        Ok(())
    })
    .unwrap();
    let mut styles = LayoutStyles::new();
    let mut root = Style::fill();
    root.padding = Insets::uniform(10.0);
    root.gap = 5.0;
    styles.set(&tree.snapshot(), ids[0], root).unwrap();
    let mut top = Style {
        width: Length::Fraction(1.0),
        ..Style::default()
    };
    top.height = Length::Fraction(0.25);
    styles.set(&tree.snapshot(), ids[1], top).unwrap();
    let mut bottom = Style::default();
    bottom.min.height = 40.0;
    bottom.flex_grow = 1.0;
    styles.set(&tree.snapshot(), ids[2], bottom).unwrap();
    let result = compute(&tree.snapshot(), &styles, s, vp(200.0, 300.0, 1.0, 1)).unwrap();
    assert_eq!(
        result.get(ids[1]).unwrap().rect,
        Rect::new(10.0, 10.0, 180.0, 70.0)
    );
    assert_eq!(
        result.get(ids[2]).unwrap().rect,
        Rect::new(10.0, 85.0, 180.0, 205.0)
    );
}

#[test]
fn zero_sized_viewport_yields_empty_coverage() {
    let (tree, surface, root, _, _, styles) = fixture();
    let result = compute(&tree.snapshot(), &styles, surface, vp(0.0, 0.0, 1.5, 11)).unwrap();
    assert_eq!(result.get(root).unwrap().physical.width, 0);
    assert_eq!(result.get(root).unwrap().physical.height, 0);
    assert_eq!(result.get(root).unwrap().visible.width, 0.0);
}

#[test]
fn overdeep_tree_rejected_before_painting() {
    let mut tree = UiTree::new();
    let s = SurfaceId(10);
    let mut root = None;
    tree.transact(|tx| {
        tx.open_surface(s)?;
        let first = tx.append_root(s, p("root"))?;
        root = Some(first);
        let mut current = first;
        for _ in 0..260 {
            current = tx.append_child(current, p("nested"))?;
        }
        Ok(())
    })
    .unwrap();
    let mut styles = LayoutStyles::new();
    styles
        .set(&tree.snapshot(), root.unwrap(), Style::fill())
        .unwrap();
    assert_eq!(
        compute(&tree.snapshot(), &styles, s, vp(200.0, 200.0, 1.0, 1)).unwrap_err(),
        LayoutError::DepthLimit
    );
}

#[test]
fn excessive_offscreen_coordinate_fails_closed_not_integer_overflow() {
    let mut tree = UiTree::new();
    let s = SurfaceId(10);
    let mut root = None;
    tree.transact(|tx| {
        tx.open_surface(s)?;
        root = Some(tx.append_root(s, p("root"))?);
        for _ in 0..20 {
            tx.append_child(root.unwrap(), p("huge"))?;
        }
        Ok(())
    })
    .unwrap();
    let snapshot = tree.snapshot();
    let mut styles = LayoutStyles::new();
    let mut root_style = Style::fill();
    root_style.axis = Axis::Row;
    styles.set(&snapshot, root.unwrap(), root_style).unwrap();
    let children = snapshot.get(root.unwrap()).unwrap().children();
    for child in children {
        let mut fixed = Style {
            width: Length::Px(100_000.0),
            ..Style::default()
        };
        fixed.min.width = 100_000.0;
        fixed.max.width = 100_000.0;
        fixed.flex_shrink = 0.0;
        styles.set(&snapshot, *child, fixed).unwrap();
    }
    assert_eq!(
        compute(&snapshot, &styles, s, vp(100.0, 100.0, 16.0, 1)).unwrap_err(),
        LayoutError::NonFiniteGeometry
    );
}
