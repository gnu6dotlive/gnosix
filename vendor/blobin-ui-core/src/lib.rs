//! Backend-independent UI identity, state and atomic component transactions.
//!
//! UI-G02/A+B state, UI-G03 layout, UI-G04/A input routing, BLOB-AC-01 gestures,
//! BLOB-AC-03 list cursor.
//! This crate deliberately does not own painting, animation or GPU scheduling.
//! Its only external dependency is std.
//! Transactions copy pages on write; immutable snapshots share untouched storage.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

/// Renderer-independent transactional pointer gestures (BLOB-AC-01).
pub mod gestures;
/// Event routing and focus contracts (UI-G04/A); platform E2E is separate.
pub mod input;
/// Deterministic 2D logical layout, DPI coverage and nested clipping (UI-G03).
pub mod layout;
/// Keyboard cursor and keep-visible scroll over uniform rows (BLOB-AC-03).
pub mod list;

/// A presentation/input namespace; surface identifiers are chosen by a host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceId(pub u64);

/// Slot plus generation; a deleted node's handle is invalid even if its slot is reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId {
    pub slot: u32,
    pub generation: u32,
}

/// Logical role for tree semantics (not a platform accessibility implementation).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    Container,
    Text,
    Button,
    Custom(String),
}

/// Semantic properties only: neither geometry nor GPU resources live here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeProperties {
    pub role: Role,
    pub label: String,
    pub enabled: bool,
}

impl NodeProperties {
    pub fn new(role: Role, label: impl Into<String>) -> Self {
        Self {
            role,
            label: label.into(),
            enabled: true,
        }
    }
}

/// Read-only node data. Children are in insertion/paint order; last is topmost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiNode {
    id: NodeId,
    surface: SurfaceId,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    properties: NodeProperties,
    key: Option<String>,
}

impl UiNode {
    pub fn id(&self) -> NodeId {
        self.id
    }
    pub fn surface(&self) -> SurfaceId {
        self.surface
    }
    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }
    pub fn properties(&self) -> &NodeProperties {
        &self.properties
    }

    /// Stable sibling reconciliation key. None for imperatively created nodes.
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }
}

#[derive(Clone, Debug)]
struct Surface {
    epoch: u64,
    revision: u64,
    roots: Vec<NodeId>,
}

#[derive(Clone, Debug)]
struct Slot {
    generation: u32,
    node: Option<UiNode>,
}

/// Fixed-size copy-on-write slot pages; mutating one node never copies every node.
const PAGE_SIZE: usize = 64;

#[derive(Clone)]
struct Page {
    entries: Vec<Slot>,
}

/// Persistent free-slot stack. Cloning a transaction does not clone its free list.
#[derive(Clone)]
struct ReusableSlot {
    slot: u32,
    next: Option<Arc<ReusableSlot>>,
}

#[derive(Clone, Default)]
struct State {
    revision: u64,
    surfaces: Arc<BTreeMap<SurfaceId, Surface>>,
    pages: Vec<Arc<Page>>,
    slot_count: usize,
    active_nodes: usize,
    reusable: Option<Arc<ReusableSlot>>,
}

impl State {
    fn entry(&self, slot: usize) -> Option<&Slot> {
        if slot >= self.slot_count {
            return None;
        }
        self.pages
            .get(slot / PAGE_SIZE)?
            .entries
            .get(slot % PAGE_SIZE)
    }

    fn entry_mut(&mut self, slot: usize) -> Option<&mut Slot> {
        if slot >= self.slot_count {
            return None;
        }
        Arc::make_mut(self.pages.get_mut(slot / PAGE_SIZE)?)
            .entries
            .get_mut(slot % PAGE_SIZE)
    }

    fn get(&self, id: NodeId) -> Result<&UiNode, UiError> {
        let entry = self
            .entry(id.slot as usize)
            .ok_or(UiError::UnknownNode(id))?;
        if entry.generation != id.generation {
            return Err(UiError::UnknownNode(id));
        }
        entry.node.as_ref().ok_or(UiError::UnknownNode(id))
    }

    fn get_mut(&mut self, id: NodeId) -> Result<&mut UiNode, UiError> {
        let entry = self
            .entry_mut(id.slot as usize)
            .ok_or(UiError::UnknownNode(id))?;
        if entry.generation != id.generation {
            return Err(UiError::UnknownNode(id));
        }
        entry.node.as_mut().ok_or(UiError::UnknownNode(id))
    }

    fn allocate(
        &mut self,
        surface: SurfaceId,
        parent: Option<NodeId>,
        properties: NodeProperties,
        generation: u32,
        key: Option<String>,
    ) -> Result<NodeId, UiError> {
        let slot = if let Some(head) = self.reusable.take() {
            self.reusable = head.next.clone();
            head.slot
        } else {
            let slot = u32::try_from(self.slot_count).map_err(|_| UiError::IdExhausted)?;
            if self.slot_count % PAGE_SIZE == 0 {
                self.pages.push(Arc::new(Page {
                    entries: Vec::new(),
                }));
            }
            self.slot_count += 1;
            slot
        };
        let id = NodeId { slot, generation };
        let page = Arc::make_mut(&mut self.pages[slot as usize / PAGE_SIZE]);
        let offset = slot as usize % PAGE_SIZE;
        if offset == page.entries.len() {
            page.entries.push(Slot {
                generation,
                node: None,
            });
        }
        let entry = &mut page.entries[offset];
        entry.generation = generation;
        entry.node = Some(UiNode {
            id,
            surface,
            parent,
            children: Vec::new(),
            properties,
            key,
        });
        self.active_nodes += 1;
        Ok(id)
    }

    fn delete_slot(&mut self, id: NodeId) -> Result<(), UiError> {
        let entry = self
            .entry_mut(id.slot as usize)
            .ok_or(UiError::UnknownNode(id))?;
        if entry.generation != id.generation || entry.node.is_none() {
            return Err(UiError::UnknownNode(id));
        }
        entry.node = None;
        let recyclable = entry.generation < u32::MAX;
        self.active_nodes -= 1;
        if recyclable {
            self.reusable = Some(Arc::new(ReusableSlot {
                slot: id.slot,
                next: self.reusable.take(),
            }));
        }
        Ok(())
    }
}

/// Explicit failures never leave a partially-applied tree transaction behind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiError {
    SurfaceExists(SurfaceId),
    UnknownSurface(SurfaceId),
    UnknownNode(NodeId),
    InvalidIndex { index: usize, length: usize },
    IdExhausted,
    RevisionExhausted,
    DuplicateKey(String),
    EmptyKey,
}

impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for UiError {}

/// A keyed child spec. Keys must be non-empty and unique among siblings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyedChild {
    pub key: String,
    pub properties: NodeProperties,
}
impl KeyedChild {
    pub fn new(key: impl Into<String>, properties: NodeProperties) -> Self {
        Self {
            key: key.into(),
            properties,
        }
    }
}

/// Counts direct-child operations, not recursively removed descendants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    pub retained: usize,
    pub inserted: usize,
    pub removed: usize,
    pub moved: usize,
    pub updated: usize,
}

/// Direct invalidation hints; the existing dirty ancestor set is preserved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeDamage {
    pub id: NodeId,
    pub semantics: bool,
    pub layout: bool,
    pub paint: bool,
    pub children: bool,
}

#[derive(Clone, Copy, Default)]
struct DamageFlags {
    semantics: bool,
    layout: bool,
    paint: bool,
    children: bool,
}

/// Per-surface invalidation, with removed-node tombstones (never redrawn).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceChange {
    pub surface: SurfaceId,
    pub surface_revision: Option<u64>,
    /// Monotonic generation of the surface lifetime, even across close/reopen.
    pub surface_epoch: Option<u64>,
    pub dirty: Vec<NodeId>,
    pub removed: Vec<NodeId>,
    pub structural: bool,
    pub opened: bool,
    pub closed: bool,
    /// Direct, domain-specific dirty reasons; separate from ancestor invalidation.
    pub damage: Vec<NodeDamage>,
}

/// A complete atomic commit. Empty changes imply a no-op and unchanged revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiCommit {
    pub revision: u64,
    pub changes: Vec<SurfaceChange>,
}

impl UiCommit {
    pub fn changed(&self) -> bool {
        !self.changes.is_empty()
    }
}

#[derive(Default)]
struct Delta {
    dirty: BTreeSet<NodeId>,
    removed: BTreeSet<NodeId>,
    structural: bool,
    opened: bool,
    closed: bool,
    damage: BTreeMap<NodeId, DamageFlags>,
}

/// Mutable workspace valid only inside UiTree::transact.
pub struct UiTransaction<'a> {
    state: &'a mut State,
    changes: &'a mut BTreeMap<SurfaceId, Delta>,
    generation_counter: &'a mut u32,
    surface_epoch_counter: &'a mut u64,
}

impl UiTransaction<'_> {
    fn allocate_node(
        &mut self,
        surface: SurfaceId,
        parent: Option<NodeId>,
        properties: NodeProperties,
    ) -> Result<NodeId, UiError> {
        self.allocate_keyed(surface, parent, properties, None)
    }

    fn allocate_keyed(
        &mut self,
        surface: SurfaceId,
        parent: Option<NodeId>,
        properties: NodeProperties,
        key: Option<String>,
    ) -> Result<NodeId, UiError> {
        // Reservations survive even aborted transactions so no failed-txn
        // handle can alias a node allocated by a later successful transaction.
        let generation = self
            .generation_counter
            .checked_add(1)
            .ok_or(UiError::IdExhausted)?;
        *self.generation_counter = generation;
        self.state
            .allocate(surface, parent, properties, generation, key)
    }

    fn delta(&mut self, surface: SurfaceId) -> &mut Delta {
        self.changes.entry(surface).or_default()
    }

    fn mark_chain(&mut self, id: NodeId) -> Result<(), UiError> {
        let mut current = Some(id);
        while let Some(handle) = current {
            let node = self.state.get(handle)?;
            let parent = node.parent;
            let surface = node.surface;
            self.delta(surface).dirty.insert(handle);
            current = parent;
        }
        Ok(())
    }

    fn structure(&mut self, surface: SurfaceId) {
        self.delta(surface).structural = true;
    }

    fn mark_structure_owner(&mut self, id: NodeId) -> Result<(), UiError> {
        let surface = self.state.get(id)?.surface;
        let damage = self.delta(surface).damage.entry(id).or_default();
        damage.children = true;
        damage.layout = true;
        damage.paint = true;
        self.mark_chain(id)
    }

    fn mark_property_change(
        &mut self,
        id: NodeId,
        previous: &NodeProperties,
        next: &NodeProperties,
    ) -> Result<(), UiError> {
        let surface = self.state.get(id)?.surface;
        let damage = self.delta(surface).damage.entry(id).or_default();
        damage.semantics = true;
        damage.paint = true;
        if previous.label != next.label || previous.role != next.role {
            damage.layout = true;
        }
        self.mark_chain(id)
    }

    pub fn open_surface(&mut self, surface: SurfaceId) -> Result<(), UiError> {
        if self.state.surfaces.contains_key(&surface) {
            return Err(UiError::SurfaceExists(surface));
        }
        let epoch = self
            .surface_epoch_counter
            .checked_add(1)
            .ok_or(UiError::RevisionExhausted)?;
        *self.surface_epoch_counter = epoch;
        Arc::make_mut(&mut self.state.surfaces).insert(
            surface,
            Surface {
                epoch,
                revision: 0,
                roots: Vec::new(),
            },
        );
        let delta = self.delta(surface);
        delta.opened = true;
        delta.structural = true;
        Ok(())
    }

    pub fn append_root(
        &mut self,
        surface: SurfaceId,
        properties: NodeProperties,
    ) -> Result<NodeId, UiError> {
        if !self.state.surfaces.contains_key(&surface) {
            return Err(UiError::UnknownSurface(surface));
        }
        let id = self.allocate_node(surface, None, properties)?;
        Arc::make_mut(&mut self.state.surfaces)
            .get_mut(&surface)
            .expect("verified")
            .roots
            .push(id);
        self.mark_chain(id)?;
        self.structure(surface);
        Ok(id)
    }

    pub fn insert_child(
        &mut self,
        parent: NodeId,
        index: usize,
        properties: NodeProperties,
    ) -> Result<NodeId, UiError> {
        let node = self.state.get(parent)?;
        let surface = node.surface;
        let length = node.children.len();
        if index > length {
            return Err(UiError::InvalidIndex { index, length });
        }
        let id = self.allocate_node(surface, Some(parent), properties)?;
        self.state.get_mut(parent)?.children.insert(index, id);
        self.mark_structure_owner(parent)?;
        self.mark_chain(id)?;
        self.structure(surface);
        Ok(id)
    }

    pub fn append_child(
        &mut self,
        parent: NodeId,
        properties: NodeProperties,
    ) -> Result<NodeId, UiError> {
        let index = self.state.get(parent)?.children.len();
        self.insert_child(parent, index, properties)
    }

    /// Only semantic value changes invalidate the node and its ancestors.
    pub fn set_properties(
        &mut self,
        id: NodeId,
        properties: NodeProperties,
    ) -> Result<(), UiError> {
        if self.state.get(id)?.properties == properties {
            return Ok(());
        }
        let previous = self.state.get(id)?.properties.clone();
        self.state.get_mut(id)?.properties = properties.clone();
        self.mark_property_change(id, &previous, &properties)
    }

    /// Reorder within its existing parent, or its surface root list.
    pub fn reorder(&mut self, id: NodeId, index: usize) -> Result<(), UiError> {
        let node = self.state.get(id)?;
        let parent = node.parent;
        let surface = node.surface;
        let children = if let Some(parent) = parent {
            &self.state.get(parent)?.children
        } else {
            &self
                .state
                .surfaces
                .get(&surface)
                .expect("node has surface")
                .roots
        };
        let length = children.len();
        if index >= length {
            return Err(UiError::InvalidIndex { index, length });
        }
        let previous = children
            .iter()
            .position(|candidate| *candidate == id)
            .expect("invariant: node occurs in parent's children");
        if previous == index {
            return Ok(());
        }
        let siblings = if let Some(parent) = parent {
            &mut self.state.get_mut(parent)?.children
        } else {
            &mut Arc::make_mut(&mut self.state.surfaces)
                .get_mut(&surface)
                .expect("verified")
                .roots
        };
        siblings.remove(previous);
        siblings.insert(index, id);
        self.mark_chain(id)?;
        if let Some(parent) = parent {
            self.mark_structure_owner(parent)?;
        }
        self.structure(surface);
        Ok(())
    }

    /// Reconcile a parent's immediate children by stable unique keys.
    ///
    /// Keeps matching NodeIds (and descendants) across reorder, applies
    /// semantic changes to matching nodes, creates new keys, and recursively
    /// removes old keys and unkeyed children. Duplicate keys fail atomically.
    /// This operation is intentionally independent of layout/rendering.
    pub fn reconcile_children(
        &mut self,
        parent: NodeId,
        desired: &[KeyedChild],
    ) -> Result<ReconcileReport, UiError> {
        let mut keys = BTreeSet::new();
        for child in desired {
            if child.key.is_empty() {
                return Err(UiError::EmptyKey);
            }
            if !keys.insert(child.key.as_str()) {
                return Err(UiError::DuplicateKey(child.key.clone()));
            }
        }
        let surface = self.state.get(parent)?.surface;
        let old = self.state.get(parent)?.children.clone();
        let mut old_keys = BTreeMap::new();
        for id in &old {
            if let Some(key) = self.state.get(*id)?.key() {
                old_keys.insert(key.to_owned(), *id);
            }
        }
        let mut report = ReconcileReport::default();
        let mut next = Vec::with_capacity(desired.len());
        for child in desired {
            if let Some(id) = old_keys.get(&child.key).copied() {
                report.retained += 1;
                if self.state.get(id)?.properties != child.properties {
                    self.set_properties(id, child.properties.clone())?;
                    report.updated += 1;
                }
                next.push(id);
            } else {
                let id = self.allocate_keyed(
                    surface,
                    Some(parent),
                    child.properties.clone(),
                    Some(child.key.clone()),
                )?;
                self.mark_chain(id)?;
                report.inserted += 1;
                next.push(id);
            }
        }
        let keep: BTreeSet<NodeId> = next.iter().copied().collect();
        for id in &old {
            if !keep.contains(id) {
                self.remove(*id)?;
                report.removed += 1;
            }
        }
        report.moved = next
            .iter()
            .enumerate()
            .filter(|(i, id)| {
                old.iter()
                    .position(|item| item == *id)
                    .is_some_and(|at| at != *i)
            })
            .count();
        if self.state.get(parent)?.children != next {
            self.state.get_mut(parent)?.children = next;
            self.mark_structure_owner(parent)?;
            self.structure(surface);
        }
        Ok(report)
    }

    /// Recursive disposal. Removed identities cannot be targeted after commit.
    pub fn remove(&mut self, id: NodeId) -> Result<(), UiError> {
        let node = self.state.get(id)?;
        let parent = node.parent;
        let surface = node.surface;
        if let Some(parent) = parent {
            self.mark_structure_owner(parent)?;
            let siblings = &mut self.state.get_mut(parent)?.children;
            siblings.retain(|child| *child != id);
        } else {
            Arc::make_mut(&mut self.state.surfaces)
                .get_mut(&surface)
                .expect("node has surface")
                .roots
                .retain(|root| *root != id);
        }
        let mut stack = vec![id];
        while let Some(next) = stack.pop() {
            let children = self.state.get(next)?.children.clone();
            stack.extend(children);
            self.state.delete_slot(next)?;
            self.delta(surface).removed.insert(next);
        }
        self.structure(surface);
        Ok(())
    }

    /// Closes only this surface and recursively releases its nodes.
    pub fn close_surface(&mut self, surface: SurfaceId) -> Result<(), UiError> {
        let roots = self
            .state
            .surfaces
            .get(&surface)
            .ok_or(UiError::UnknownSurface(surface))?
            .roots
            .clone();
        for root in roots {
            self.remove(root)?;
        }
        Arc::make_mut(&mut self.state.surfaces).remove(&surface);
        let delta = self.delta(surface);
        delta.dirty.clear();
        delta.closed = true;
        delta.structural = true;
        Ok(())
    }
}

/// Frozen, persistent copy-on-write snapshot. Holding a snapshot never blocks
/// updates, and untouched node pages are shared instead of being recopied.
#[derive(Clone)]
pub struct UiSnapshot(Arc<State>);

impl UiSnapshot {
    pub fn revision(&self) -> u64 {
        self.0.revision
    }

    pub fn surface_revision(&self, surface: SurfaceId) -> Option<u64> {
        self.0
            .surfaces
            .get(&surface)
            .map(|surface| surface.revision)
    }

    /// Generation distinguishes a reopened surface from its previous lifetime.
    pub fn surface_epoch(&self, surface: SurfaceId) -> Option<u64> {
        self.0.surfaces.get(&surface).map(|surface| surface.epoch)
    }

    pub fn roots(&self, surface: SurfaceId) -> Option<&[NodeId]> {
        self.0
            .surfaces
            .get(&surface)
            .map(|surface| surface.roots.as_slice())
    }

    pub fn get(&self, id: NodeId) -> Option<&UiNode> {
        self.0.get(id).ok()
    }

    pub fn surface_ids(&self) -> impl Iterator<Item = SurfaceId> + '_ {
        self.0.surfaces.keys().copied()
    }

    pub fn node_count(&self) -> usize {
        self.0.active_nodes
    }

    /// Diagnostic proof of structural sharing; no mutable storage is exposed.
    pub fn shared_pages_with(&self, other: &Self) -> usize {
        self.0
            .pages
            .iter()
            .zip(other.0.pages.iter())
            .filter(|(a, b)| Arc::ptr_eq(a, b))
            .count()
    }

    pub fn page_count(&self) -> usize {
        self.0.pages.len()
    }
}

/// Headless, platform-independent component tree.
#[derive(Default)]
pub struct UiTree {
    state: State,
    generation_counter: u32,
    surface_epoch_counter: u64,
}

impl UiTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn revision(&self) -> u64 {
        self.state.revision
    }

    pub fn get(&self, id: NodeId) -> Option<&UiNode> {
        self.state.get(id).ok()
    }

    pub fn snapshot(&self) -> UiSnapshot {
        // O(number of pages + number of surfaces) pointers, not O(number of nodes).
        UiSnapshot(Arc::new(self.state.clone()))
    }

    /// A failed closure or operation never publishes any partial mutations.
    ///
    /// UI-G02/B clones page references and the persistent recycle head, not
    /// the complete node graph. Edited 64-slot pages use Arc::make_mut.
    /// This is page-granular copy-on-write, not yet a fully persistent keyed DAG.
    pub fn transact<F>(&mut self, operation: F) -> Result<UiCommit, UiError>
    where
        F: FnOnce(&mut UiTransaction<'_>) -> Result<(), UiError>,
    {
        let mut prospective = self.state.clone();
        let mut changes = BTreeMap::new();
        operation(&mut UiTransaction {
            state: &mut prospective,
            changes: &mut changes,
            generation_counter: &mut self.generation_counter,
            surface_epoch_counter: &mut self.surface_epoch_counter,
        })?;
        if changes.is_empty() {
            return Ok(UiCommit {
                revision: self.state.revision,
                changes: Vec::new(),
            });
        }
        prospective.revision = prospective
            .revision
            .checked_add(1)
            .ok_or(UiError::RevisionExhausted)?;
        let mut entries = Vec::with_capacity(changes.len());
        for (surface, delta) in changes {
            let surface_revision =
                if let Some(entry) = Arc::make_mut(&mut prospective.surfaces).get_mut(&surface) {
                    entry.revision = entry
                        .revision
                        .checked_add(1)
                        .ok_or(UiError::RevisionExhausted)?;
                    Some(entry.revision)
                } else {
                    None
                };
            entries.push(SurfaceChange {
                surface,
                surface_revision,
                surface_epoch: prospective.surfaces.get(&surface).map(|s| s.epoch),
                dirty: delta.dirty.difference(&delta.removed).copied().collect(),
                removed: delta.removed.iter().copied().collect(),
                structural: delta.structural,
                opened: delta.opened,
                closed: delta.closed,
                damage: delta
                    .damage
                    .into_iter()
                    .filter(|(id, _)| !delta.removed.contains(id))
                    .map(|(id, d)| NodeDamage {
                        id,
                        semantics: d.semantics,
                        layout: d.layout,
                        paint: d.paint,
                        children: d.children,
                    })
                    .collect(),
            });
        }
        self.state = prospective;
        Ok(UiCommit {
            revision: self.state.revision,
            changes: entries,
        })
    }
}
