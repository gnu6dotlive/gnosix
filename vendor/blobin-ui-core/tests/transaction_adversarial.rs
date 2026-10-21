use blobin_ui_core::{NodeId, NodeProperties, Role, SurfaceId, UiCommit, UiError, UiTree};

fn p(s: &str) -> NodeProperties {
    NodeProperties::new(Role::Button, s)
}

#[test]
fn aborted_handle_can_never_become_a_valid_handle() {
    let mut tree = UiTree::new();
    tree.transact(|tx| tx.open_surface(SurfaceId(3))).unwrap();
    let mut leaked = None;
    assert_eq!(
        tree.transact(|tx| {
            leaked = Some(tx.append_root(SurfaceId(3), p("not committed"))?);
            Err(UiError::UnknownSurface(SurfaceId(404)))
        })
        .unwrap_err(),
        UiError::UnknownSurface(SurfaceId(404))
    );
    let invalid_handle = leaked.unwrap();
    assert_eq!(tree.revision(), 1);
    assert!(tree.get(invalid_handle).is_none());
    let mut replacement = None;
    tree.transact(|tx| {
        replacement = Some(tx.append_root(SurfaceId(3), p("real"))?);
        Ok(())
    })
    .unwrap();
    assert_ne!(replacement.unwrap(), invalid_handle);
    assert_eq!(replacement.unwrap().slot, invalid_handle.slot);
    assert!(tree.get(invalid_handle).is_none());
}

#[test]
fn created_and_removed_in_the_same_transaction_is_not_dirty() {
    let mut tree = UiTree::new();
    let change = tree
        .transact(|tx| {
            tx.open_surface(SurfaceId(5))?;
            let n = tx.append_root(SurfaceId(5), p("transient"))?;
            tx.remove(n)?;
            Ok(())
        })
        .unwrap();
    assert!(change.changes[0].dirty.is_empty());
    assert_eq!(change.changes[0].removed.len(), 1);
    assert_eq!(tree.snapshot().node_count(), 0);
}

#[test]
fn failed_transaction_cannot_invalidate_a_different_surface() {
    let mut tree = UiTree::new();
    let mut handles = Vec::new();
    tree.transact(|tx| {
        tx.open_surface(SurfaceId(10))?;
        tx.open_surface(SurfaceId(11))?;
        handles.push(tx.append_root(SurfaceId(10), p("left"))?);
        handles.push(tx.append_root(SurfaceId(11), p("right"))?);
        Ok(())
    })
    .unwrap();
    let old = tree.snapshot();
    assert!(tree
        .transact(|tx| {
            tx.set_properties(handles[0], p("changed"))?;
            tx.remove(handles[1])?;
            tx.insert_child(handles[1], 0, p("invalid"))?;
            Ok(())
        })
        .is_err());
    assert_eq!(tree.snapshot().revision(), old.revision());
    assert_eq!(tree.snapshot().surface_revision(SurfaceId(10)), Some(1));
    assert_eq!(tree.snapshot().surface_revision(SurfaceId(11)), Some(1));
    assert_eq!(tree.get(handles[0]).unwrap().properties().label, "left");
    assert_eq!(tree.get(handles[1]).unwrap().properties().label, "right");
}

fn seeded_replay(seed: u64) -> (Vec<UiCommit>, Vec<NodeId>, u64) {
    let mut rng = seed;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let mut tree = UiTree::new();
    let mut roots = Vec::new();
    let opening = tree
        .transact(|tx| {
            tx.open_surface(SurfaceId(1))?;
            tx.open_surface(SurfaceId(2))?;
            roots.push(tx.append_root(SurfaceId(1), p("workspace"))?);
            roots.push(tx.append_root(SurfaceId(2), p("independent"))?);
            Ok(())
        })
        .unwrap();
    let mut commits = vec![opening];
    let mut children = Vec::new();
    for step in 0..600 {
        let action = (next() % 5) as usize;
        let index = if children.is_empty() {
            0
        } else {
            next() as usize % children.len()
        };
        let result = match action {
            0 | 1 => {
                let at = next() as usize % (children.len() + 1);
                let mut inserted = None;
                let commit = tree
                    .transact(|tx| {
                        inserted =
                            Some(tx.insert_child(roots[0], at, p(&format!("node-{step}")))?);
                        Ok(())
                    })
                    .unwrap();
                children.insert(at, inserted.unwrap());
                commit
            }
            2 if !children.is_empty() => {
                let removed = children.remove(index);
                tree.transact(|tx| tx.remove(removed)).unwrap()
            }
            3 if !children.is_empty() => {
                let at = next() as usize % children.len();
                let id = children.remove(index);
                children.insert(at, id);
                tree.transact(|tx| tx.reorder(id, at)).unwrap()
            }
            4 if !children.is_empty() => tree
                .transact(|tx| tx.set_properties(children[index], p(&format!("step-{step}"))))
                .unwrap(),
            _ => tree.transact(|_| Ok(())).unwrap(),
        };
        commits.push(result);
        let snapshot = tree.snapshot();
        assert_eq!(
            snapshot.get(roots[0]).unwrap().children(),
            children.as_slice()
        );
        assert_eq!(snapshot.surface_revision(SurfaceId(2)), Some(1));
        assert!(snapshot.get(roots[1]).is_some());
        for id in &children {
            assert_eq!(snapshot.get(*id).unwrap().parent(), Some(roots[0]));
            assert_eq!(snapshot.get(*id).unwrap().surface(), SurfaceId(1));
        }
    }
    (commits, children, tree.revision())
}

#[test]
fn deterministic_seeded_mutation_replay_with_600_steps() {
    let first = seeded_replay(0x5eeb_d012_2026);
    let second = seeded_replay(0x5eeb_d012_2026);
    assert_eq!(first, second);
}

#[test]
fn empty_transaction_is_a_pure_no_op() {
    let mut tree = UiTree::new();
    let outcome = tree.transact(|_| Ok(())).unwrap();
    assert!(!outcome.changed());
    assert_eq!(outcome.revision, 0);
    assert_eq!(tree.snapshot().node_count(), 0);
}
