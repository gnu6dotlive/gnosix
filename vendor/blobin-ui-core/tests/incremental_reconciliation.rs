use blobin_ui_core::{KeyedChild, NodeProperties, Role, SurfaceId, UiError, UiSnapshot, UiTree};
use std::collections::BTreeMap;

fn p(label: &str) -> NodeProperties {
    NodeProperties::new(Role::Button, label)
}
fn keyed(key: &str, label: &str) -> KeyedChild {
    KeyedChild::new(key, p(label))
}
fn seed() -> (UiTree, SurfaceId, blobin_ui_core::NodeId) {
    let mut tree = UiTree::new();
    let surface = SurfaceId(11);
    let mut root = None;
    tree.transact(|tx| {
        tx.open_surface(surface)?;
        root = Some(tx.append_root(surface, NodeProperties::new(Role::Container, "root"))?);
        Ok(())
    })
    .unwrap();
    (tree, surface, root.unwrap())
}
fn normalized(snapshot: &UiSnapshot, root: blobin_ui_core::NodeId) -> Vec<(String, String)> {
    snapshot
        .get(root)
        .unwrap()
        .children()
        .iter()
        .map(|id| {
            let node = snapshot.get(*id).unwrap();
            (
                node.key().unwrap_or("").to_owned(),
                node.properties().label.clone(),
            )
        })
        .collect()
}

#[test]
fn keyed_reorder_retains_handles_and_descendants() {
    let (mut tree, _, root) = seed();
    let first = vec![keyed("a", "A"), keyed("b", "B"), keyed("c", "C")];
    let report = tree
        .transact(|tx| {
            let r = tx.reconcile_children(root, &first)?;
            assert_eq!(r.inserted, 3);
            Ok(())
        })
        .unwrap();
    assert!(report.changes[0].structural);
    let initial = tree.snapshot();
    let old = initial.get(root).unwrap().children().to_vec();
    let child_b = old[1];
    let mut grandchild = None;
    tree.transact(|tx| {
        grandchild = Some(tx.append_child(child_b, p("nested"))?);
        Ok(())
    })
    .unwrap();
    let second = vec![keyed("c", "C"), keyed("a", "A"), keyed("b", "B2")];
    let changed = tree
        .transact(|tx| {
            let r = tx.reconcile_children(root, &second)?;
            assert_eq!(r.retained, 3);
            assert_eq!(r.inserted, 0);
            assert_eq!(r.removed, 0);
            assert_eq!(r.updated, 1);
            assert!(r.moved >= 2);
            Ok(())
        })
        .unwrap();
    let now = tree.snapshot();
    assert_eq!(now.get(root).unwrap().children(), &[old[2], old[0], old[1]]);
    assert_eq!(now.get(old[1]).unwrap().children(), &[grandchild.unwrap()]);
    assert_eq!(initial.get(root).unwrap().children(), old);
    assert!(changed.changes[0]
        .damage
        .iter()
        .any(|d| d.id == child_b && d.semantics && d.layout && d.paint));
    assert!(changed.changes[0]
        .damage
        .iter()
        .any(|d| d.id == root && d.children));
}

#[test]
fn equivalent_reconcile_is_no_op_even_with_many_children() {
    let (mut tree, surface, root) = seed();
    let specs: Vec<_> = (0..200)
        .map(|i| keyed(&format!("key-{i}"), &format!("label-{i}")))
        .collect();
    tree.transact(|tx| {
        tx.reconcile_children(root, &specs)?;
        Ok(())
    })
    .unwrap();
    let previous = tree.snapshot();
    let commit = tree
        .transact(|tx| {
            let report = tx.reconcile_children(root, &specs)?;
            assert_eq!(report.retained, 200);
            assert_eq!(report.inserted, 0);
            assert_eq!(report.moved, 0);
            assert_eq!(report.updated, 0);
            Ok(())
        })
        .unwrap();
    assert!(!commit.changed());
    assert_eq!(commit.revision, previous.revision());
    assert_eq!(
        tree.snapshot().surface_revision(surface),
        previous.surface_revision(surface)
    );
    assert_eq!(
        tree.snapshot().shared_pages_with(&previous),
        previous.page_count()
    );
}

#[test]
fn duplicate_empty_key_and_stale_parent_rejected_atomically() {
    let (mut tree, _, root) = seed();
    let revision = tree.revision();
    assert_eq!(
        tree.transact(|tx| {
            tx.reconcile_children(root, &[keyed("x", "1"), keyed("x", "2")])?;
            Ok(())
        })
        .unwrap_err(),
        UiError::DuplicateKey("x".into())
    );
    assert_eq!(
        tree.transact(|tx| {
            tx.reconcile_children(root, &[keyed("", "empty")])?;
            Ok(())
        })
        .unwrap_err(),
        UiError::EmptyKey
    );
    tree.transact(|tx| {
        let temporary = tx.append_child(root, p("rollback"))?;
        assert_eq!(
            tx.reconcile_children(temporary, &[keyed("x", "ok"), keyed("x", "collision")])
                .unwrap_err(),
            UiError::DuplicateKey("x".into())
        );
        Err(UiError::EmptyKey)
    })
    .unwrap_err();
    assert_eq!(tree.revision(), revision);
    assert!(tree.snapshot().get(root).unwrap().children().is_empty());
}

#[test]
fn independent_cow_pages_are_shared_between_snapshots() {
    let (mut tree, surface, root) = seed();
    let mut ids = Vec::new();
    tree.transact(|tx| {
        for i in 0..1024 {
            ids.push(tx.append_child(root, p(&format!("node-{i}")))?);
        }
        Ok(())
    })
    .unwrap();
    let prior = tree.snapshot();
    assert!(prior.page_count() >= 16);
    let target = ids[600];
    let commit = tree
        .transact(|tx| tx.set_properties(target, p("changed")))
        .unwrap();
    let now = tree.snapshot();
    assert_eq!(prior.get(target).unwrap().properties().label, "node-600");
    assert_eq!(now.get(target).unwrap().properties().label, "changed");
    assert_eq!(prior.node_count(), now.node_count());
    assert_eq!(prior.shared_pages_with(&now) + 1, prior.page_count());
    assert_eq!(commit.changes.len(), 1);
    assert_eq!(commit.changes[0].surface, surface);
    assert_eq!(commit.changes[0].dirty, vec![root, target]);
    assert!(!commit.changes[0].structural);
}

#[test]
fn property_domains_distinguish_reflow_from_paint() {
    let (mut tree, _, root) = seed();
    let mut child = None;
    tree.transact(|tx| {
        child = Some(tx.append_child(root, p("action"))?);
        Ok(())
    })
    .unwrap();
    let child = child.unwrap();
    let mut disabled = p("action");
    disabled.enabled = false;
    let only_paint = tree
        .transact(|tx| tx.set_properties(child, disabled))
        .unwrap();
    let damage = only_paint.changes[0]
        .damage
        .iter()
        .find(|x| x.id == child)
        .unwrap();
    assert!(damage.semantics && damage.paint);
    assert!(!damage.layout && !damage.children);
    let role_changed = tree
        .transact(|tx| tx.set_properties(child, NodeProperties::new(Role::Text, "text")))
        .unwrap();
    let damage = role_changed.changes[0]
        .damage
        .iter()
        .find(|x| x.id == child)
        .unwrap();
    assert!(damage.semantics && damage.paint && damage.layout);
}

#[test]
fn close_and_reopen_surface_uses_new_epoch_not_old_revision() {
    let (mut tree, surface, root) = seed();
    let epoch1 = tree.snapshot().surface_epoch(surface).unwrap();
    tree.transact(|tx| tx.close_surface(surface)).unwrap();
    assert!(tree.get(root).is_none());
    tree.transact(|tx| tx.open_surface(surface)).unwrap();
    let epoch2 = tree.snapshot().surface_epoch(surface).unwrap();
    assert!(epoch2 > epoch1);
    assert_eq!(tree.snapshot().surface_revision(surface), Some(1));
    assert!(tree.snapshot().roots(surface).unwrap().is_empty());
}

#[test]
fn aborted_surface_epoch_is_never_reused() {
    let mut tree = UiTree::new();
    let mut seen = None;
    assert_eq!(
        tree.transact(|tx| {
            tx.open_surface(SurfaceId(50))?;
            Err(UiError::UnknownSurface(SurfaceId(404)))
        })
        .unwrap_err(),
        UiError::UnknownSurface(SurfaceId(404))
    );
    let report = tree.transact(|tx| tx.open_surface(SurfaceId(50))).unwrap();
    seen.replace(report.changes[0].surface_epoch.unwrap());
    assert!(seen.unwrap() >= 2);
    assert_eq!(tree.snapshot().surface_epoch(SurfaceId(50)), seen);
}

#[test]
fn keyed_reconciliation_matches_independent_full_rebuild_oracle_400_steps() {
    let (mut tree, surface, root) = seed();
    let mut expected: Vec<(String, String)> = Vec::new();
    let mut stable: BTreeMap<String, blobin_ui_core::NodeId> = BTreeMap::new();
    let mut rng = 0x2026_1012_55aa_11bbu64;
    for step in 0..400usize {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        match rng % 4 {
            0 => {
                if expected.len() < 40 {
                    let key = format!("key-{step}");
                    expected.insert(
                        (rng as usize / 3) % (expected.len() + 1),
                        (key.clone(), key),
                    );
                }
            }
            1 if !expected.is_empty() => {
                expected.remove((rng as usize / 7) % expected.len());
            }
            2 if !expected.is_empty() => {
                let from = (rng as usize / 5) % expected.len();
                let to = (rng as usize / 11) % expected.len();
                let pair = expected.remove(from);
                expected.insert(to, pair);
            }
            3 if !expected.is_empty() => {
                let at = (rng as usize / 13) % expected.len();
                expected[at].1 = format!("updated-{step}");
            }
            _ => {}
        }
        let spec: Vec<_> = expected.iter().map(|(k, l)| keyed(k, l)).collect();
        let old_stable = stable.clone();
        tree.transact(|tx| {
            tx.reconcile_children(root, &spec)?;
            Ok(())
        })
        .unwrap();
        let snap = tree.snapshot();
        // Oracle is a complete rebuild of declarative key/label pairs, not a
        // replay of the implementation's reconcile algorithm or damage set.
        assert_eq!(normalized(&snap, root), expected, "step={step}");
        assert_eq!(snap.surface_revision(surface), Some(snap.revision()));
        stable = snap
            .get(root)
            .unwrap()
            .children()
            .iter()
            .map(|id| (snap.get(*id).unwrap().key().unwrap().to_owned(), *id))
            .collect();
        for (key, id) in old_stable {
            if let Some(new_id) = stable.get(&key) {
                assert_eq!(*new_id, id, "key {key} was spuriously replaced");
            } else {
                assert!(snap.get(id).is_none(), "removed id still addressable");
            }
        }
        assert_eq!(snap.node_count(), expected.len() + 1);
    }
}

#[test]
fn many_semantic_updates_in_one_transaction_coalesce_to_one_revision() {
    let (mut tree, surface, root) = seed();
    let mut child = None;
    tree.transact(|tx| {
        child = Some(tx.append_child(root, p("old"))?);
        Ok(())
    })
    .unwrap();
    let child = child.unwrap();
    let prior = tree.snapshot();
    let commit = tree
        .transact(|tx| {
            for i in 0..100 {
                tx.set_properties(child, p(&format!("burst-{i}")))?;
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(commit.revision, prior.revision() + 1);
    assert_eq!(commit.changes.len(), 1);
    assert_eq!(commit.changes[0].dirty, vec![root, child]);
    assert_eq!(commit.changes[0].damage.len(), 1);
    assert_eq!(
        commit.changes[0].surface_revision,
        Some(prior.surface_revision(surface).unwrap() + 1)
    );
    assert_eq!(
        tree.snapshot().get(child).unwrap().properties().label,
        "burst-99"
    );
    assert_eq!(prior.get(child).unwrap().properties().label, "old");
}

#[test]
fn removing_keyed_subtree_produces_tombstones_not_repaint_claims() {
    let (mut tree, _, root) = seed();
    tree.transact(|tx| {
        tx.reconcile_children(root, &[keyed("keep", "K"), keyed("drop", "D")])?;
        Ok(())
    })
    .unwrap();
    let old = tree.snapshot();
    let drop_id = old.get(root).unwrap().children()[1];
    let mut nested = None;
    tree.transact(|tx| {
        nested = Some(tx.append_child(drop_id, p("nested"))?);
        Ok(())
    })
    .unwrap();
    let deleted = tree
        .transact(|tx| {
            let change = tx.reconcile_children(root, &[keyed("keep", "K")])?;
            assert_eq!(change.removed, 1);
            Ok(())
        })
        .unwrap();
    assert_eq!(deleted.changes[0].removed.len(), 2);
    assert!(deleted.changes[0].removed.contains(&drop_id));
    assert!(deleted.changes[0].removed.contains(&nested.unwrap()));
    assert!(!deleted.changes[0].dirty.contains(&drop_id));
    assert!(!deleted.changes[0].damage.iter().any(|d| d.id == drop_id));
    assert!(tree.get(drop_id).is_none());
    assert_eq!(tree.snapshot().get(root).unwrap().children().len(), 1);
    assert_eq!(old.get(root).unwrap().children().len(), 2);
}

#[test]
fn one_surface_updates_never_invalidate_another_surface_or_epoch() {
    let mut tree = UiTree::new();
    let mut roots = Vec::new();
    let a = SurfaceId(101);
    let b = SurfaceId(102);
    tree.transact(|tx| {
        tx.open_surface(a)?;
        tx.open_surface(b)?;
        roots.push(tx.append_root(a, p("A"))?);
        roots.push(tx.append_root(b, p("B"))?);
        Ok(())
    })
    .unwrap();
    let initial = tree.snapshot();
    let update = tree
        .transact(|tx| {
            tx.reconcile_children(roots[0], &[keyed("x", "X"), keyed("y", "Y")])?;
            Ok(())
        })
        .unwrap();
    assert_eq!(update.changes.len(), 1);
    assert_eq!(update.changes[0].surface, a);
    assert_eq!(tree.snapshot().surface_epoch(b), initial.surface_epoch(b));
    assert_eq!(
        tree.snapshot().surface_revision(b),
        initial.surface_revision(b)
    );
    tree.transact(|tx| {
        tx.close_surface(a)?;
        tx.open_surface(a)?;
        Ok(())
    })
    .unwrap();
    assert_eq!(tree.snapshot().surface_epoch(b), initial.surface_epoch(b));
    assert_eq!(
        tree.snapshot().surface_revision(b),
        initial.surface_revision(b)
    );
    assert_ne!(tree.snapshot().surface_epoch(a), initial.surface_epoch(a));
}
