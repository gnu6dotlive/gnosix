use blobin_ui_core::{NodeId, NodeProperties, Role, SurfaceId, UiError, UiTree};

fn props(label: &str) -> NodeProperties {
    NodeProperties::new(Role::Container, label)
}

fn populated() -> (UiTree, SurfaceId, NodeId, NodeId, NodeId) {
    let mut tree = UiTree::new();
    let surface = SurfaceId(1);
    let mut ids = Vec::new();
    tree.transact(|tx| {
        tx.open_surface(surface)?;
        let root = tx.append_root(surface, props("root"))?;
        let first = tx.append_child(root, props("first"))?;
        let second = tx.append_child(root, props("second"))?;
        ids = vec![root, first, second];
        Ok(())
    })
    .unwrap();
    (tree, surface, ids[0], ids[1], ids[2])
}

#[test]
fn creates_ordered_tree_with_unique_handles() {
    let (tree, surface, root, first, second) = populated();
    let snapshot = tree.snapshot();
    assert_eq!(tree.revision(), 1);
    assert_eq!(snapshot.node_count(), 3);
    assert_eq!(snapshot.roots(surface), Some(&[root][..]));
    assert_eq!(snapshot.get(root).unwrap().children(), &[first, second]);
    assert_eq!(snapshot.get(first).unwrap().parent(), Some(root));
    assert_eq!(snapshot.surface_revision(surface), Some(1));
}

#[test]
fn snapshot_is_immutable_after_successive_transactions() {
    let (mut tree, surface, root, _, _) = populated();
    let before = tree.snapshot();
    tree.transact(|tx| {
        tx.append_child(root, props("later"))?;
        Ok(())
    })
    .unwrap();
    assert_eq!(before.revision(), 1);
    assert_eq!(before.get(root).unwrap().children().len(), 2);
    assert_eq!(tree.snapshot().get(root).unwrap().children().len(), 3);
    assert_eq!(tree.snapshot().surface_revision(surface), Some(2));
}

#[test]
fn invalid_child_index_rolls_back_insert_and_revision() {
    let (mut tree, _, root, _, _) = populated();
    let original = tree.snapshot();
    let error = tree
        .transact(|tx| {
            tx.append_child(root, props("would rollback"))?;
            tx.insert_child(root, usize::MAX, props("bad"))?;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(
        error,
        UiError::InvalidIndex {
            index: usize::MAX,
            length: 3
        }
    );
    assert_eq!(tree.revision(), original.revision());
    assert_eq!(tree.snapshot().node_count(), original.node_count());
    assert_eq!(
        tree.snapshot().get(root).unwrap().children(),
        original.get(root).unwrap().children()
    );
}

#[test]
fn custom_aborted_transaction_does_not_escape() {
    let (mut tree, surface, _, _, _) = populated();
    let err = tree
        .transact(|tx| {
            tx.append_root(surface, props("new root"))?;
            Err(UiError::UnknownSurface(SurfaceId(999)))
        })
        .unwrap_err();
    assert_eq!(err, UiError::UnknownSurface(SurfaceId(999)));
    assert_eq!(tree.revision(), 1);
    assert_eq!(tree.snapshot().roots(surface).unwrap().len(), 1);
}

#[test]
fn unmodified_properties_and_no_op_reorder_do_not_bump_revision() {
    let (mut tree, surface, root, first, _) = populated();
    let result = tree
        .transact(|tx| {
            tx.set_properties(first, props("first"))?;
            tx.reorder(first, 0)?;
            Ok(())
        })
        .unwrap();
    assert!(!result.changed());
    assert_eq!(result.revision, 1);
    assert_eq!(tree.snapshot().surface_revision(surface), Some(1));
    assert_eq!(tree.get(root).unwrap().children()[0], first);
}

#[test]
fn changing_one_leaf_marks_only_leaf_and_ancestors() {
    let (mut tree, surface, root, first, second) = populated();
    let update = tree
        .transact(|tx| {
            tx.set_properties(first, NodeProperties::new(Role::Button, "submit"))?;
            Ok(())
        })
        .unwrap();
    assert!(update.changed());
    assert_eq!(update.revision, 2);
    assert_eq!(update.changes.len(), 1);
    assert_eq!(update.changes[0].surface, surface);
    assert_eq!(update.changes[0].dirty, vec![root, first]);
    assert!(!update.changes[0].dirty.contains(&second));
    assert!(!update.changes[0].structural);
    assert_eq!(tree.get(first).unwrap().properties().role, Role::Button);
}

#[test]
fn insert_child_at_middle_preserves_existing_ids() {
    let (mut tree, _, root, first, second) = populated();
    let mut inserted = None;
    let update = tree
        .transact(|tx| {
            inserted = Some(tx.insert_child(root, 1, props("middle"))?);
            Ok(())
        })
        .unwrap();
    let mid = inserted.unwrap();
    assert_eq!(tree.get(root).unwrap().children(), &[first, mid, second]);
    assert!(update.changes[0].structural);
    assert_eq!(update.changes[0].dirty, vec![root, mid]);
    assert_eq!(tree.get(second).unwrap().id(), second);
}

#[test]
fn reorder_changes_sibling_order_without_changing_node_identity() {
    let (mut tree, _, root, first, second) = populated();
    let update = tree.transact(|tx| tx.reorder(first, 1)).unwrap();
    assert_eq!(tree.get(root).unwrap().children(), &[second, first]);
    assert_eq!(tree.get(first).unwrap().id(), first);
    assert!(update.changes[0].structural);
}

#[test]
fn invalid_reorder_is_atomic() {
    let (mut tree, _, root, first, second) = populated();
    assert_eq!(
        tree.transact(|tx| tx.reorder(first, 2)).unwrap_err(),
        UiError::InvalidIndex {
            index: 2,
            length: 2
        }
    );
    assert_eq!(tree.get(root).unwrap().children(), &[first, second]);
    assert_eq!(tree.revision(), 1);
}

#[test]
fn descendant_removal_invalidates_all_and_reuses_only_new_generation() {
    let (mut tree, surface, root, first, second) = populated();
    let mut grandchild = None;
    tree.transact(|tx| {
        grandchild = Some(tx.append_child(first, props("grandchild"))?);
        Ok(())
    })
    .unwrap();
    let grandchild = grandchild.unwrap();
    let previous = tree.snapshot();
    let deleted = tree.transact(|tx| tx.remove(first)).unwrap();
    assert!(tree.get(first).is_none());
    assert!(tree.get(grandchild).is_none());
    assert_eq!(tree.get(root).unwrap().children(), &[second]);
    assert_eq!(deleted.changes[0].surface, surface);
    assert_eq!(deleted.changes[0].removed, vec![first, grandchild]);
    assert!(deleted.changes[0].dirty.contains(&root));
    assert!(previous.get(first).is_some());
    assert!(previous.get(grandchild).is_some());
    let mut recycled = None;
    tree.transact(|tx| {
        recycled = Some(tx.append_child(root, props("new"))?);
        Ok(())
    })
    .unwrap();
    let next = recycled.unwrap();
    assert!(next.slot == first.slot || next.slot == grandchild.slot);
    assert!(tree.get(first).is_none());
    assert!(tree.get(grandchild).is_none());
    if next.slot == first.slot {
        assert!(next.generation > first.generation);
    }
    if next.slot == grandchild.slot {
        assert!(next.generation > grandchild.generation);
    }
}

#[test]
fn separate_surfaces_have_independent_revision_and_lifetimes() {
    let mut tree = UiTree::new();
    let left = SurfaceId(10);
    let right = SurfaceId(20);
    let mut roots = Vec::new();
    tree.transact(|tx| {
        tx.open_surface(left)?;
        tx.open_surface(right)?;
        roots.push(tx.append_root(left, props("left"))?);
        roots.push(tx.append_root(right, props("right"))?);
        Ok(())
    })
    .unwrap();
    let prior = tree.snapshot();
    let change = tree
        .transact(|tx| tx.set_properties(roots[0], props("updated left")))
        .unwrap();
    assert_eq!(change.changes.len(), 1);
    assert_eq!(change.changes[0].surface, left);
    assert_eq!(tree.snapshot().surface_revision(left), Some(2));
    assert_eq!(tree.snapshot().surface_revision(right), Some(1));
    let closed = tree.transact(|tx| tx.close_surface(left)).unwrap();
    assert!(closed.changes[0].closed);
    assert_eq!(closed.changes[0].surface_revision, None);
    assert!(tree.get(roots[0]).is_none());
    assert_eq!(tree.get(roots[1]).unwrap().properties().label, "right");
    assert_eq!(tree.snapshot().surface_revision(right), Some(1));
    assert!(prior.get(roots[0]).is_some());
}

#[test]
fn surface_cannot_be_duplicated_or_targeted_before_opening() {
    let mut tree = UiTree::new();
    assert_eq!(
        tree.transact(|tx| {
            tx.append_root(SurfaceId(42), props("x"))?;
            Ok(())
        })
        .unwrap_err(),
        UiError::UnknownSurface(SurfaceId(42))
    );
    tree.transact(|tx| tx.open_surface(SurfaceId(42))).unwrap();
    assert_eq!(
        tree.transact(|tx| tx.open_surface(SurfaceId(42)))
            .unwrap_err(),
        UiError::SurfaceExists(SurfaceId(42))
    );
    assert_eq!(tree.revision(), 1);
}

#[test]
fn closed_surface_ids_are_not_aliases_for_old_nodes() {
    let (mut tree, surface, root, first, _) = populated();
    tree.transact(|tx| tx.close_surface(surface)).unwrap();
    tree.transact(|tx| {
        tx.open_surface(surface)?;
        tx.append_root(surface, props("new root"))?;
        Ok(())
    })
    .unwrap();
    assert_eq!(tree.get(root), None);
    assert_eq!(tree.get(first), None);
    let new_root = tree.snapshot().roots(surface).unwrap()[0];
    assert_ne!(new_root, root);
    assert_eq!(tree.snapshot().surface_revision(surface), Some(1));
}

#[test]
fn invalid_stale_handle_cannot_modify_current_component() {
    let (mut tree, _, root, first, _) = populated();
    tree.transact(|tx| tx.remove(first)).unwrap();
    tree.transact(|tx| {
        tx.append_child(root, props("replacement"))?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        tree.transact(|tx| tx.set_properties(first, props("illegal")))
            .unwrap_err(),
        UiError::UnknownNode(first)
    );
}

#[test]
fn root_order_can_be_changed_without_a_parent() {
    let mut tree = UiTree::new();
    let surface = SurfaceId(9);
    let mut roots = Vec::new();
    tree.transact(|tx| {
        tx.open_surface(surface)?;
        roots.push(tx.append_root(surface, props("one"))?);
        roots.push(tx.append_root(surface, props("two"))?);
        Ok(())
    })
    .unwrap();
    tree.transact(|tx| tx.reorder(roots[0], 1)).unwrap();
    assert_eq!(
        tree.snapshot().roots(surface),
        Some(&[roots[1], roots[0]][..])
    );
}

#[test]
fn snapshots_are_send_and_sync() {
    fn requires_send_sync<T: Send + Sync>() {}
    requires_send_sync::<blobin_ui_core::UiSnapshot>();
}
