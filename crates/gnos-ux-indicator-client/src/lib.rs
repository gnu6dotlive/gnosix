#![deny(dead_code, unused_variables, unused_imports, unused_must_use)]

//! Observer for the `gnos.ux.Indicator1` publisher contract.
//!
//! An application that owns a piece of presentation state about itself — a
//! recorder that knows it is recording, a downloader that knows its progress —
//! publishes that state on the session bus. This crate discovers those
//! publishers and observes them. It never writes: the publisher owns the fact,
//! the host only reads it.
//!
//! The contract is deliberately one-directional:
//!
//! * a publisher owns `gnos.ux.Indicator1.<publisher-id>` and serves
//!   `/gnos/ux/Indicator1`;
//! * the interface exposes properties and **no mutating method**, so observing
//!   a publisher grants no authority over it;
//! * every published value carries a strictly monotonic generation, so an
//!   update that is not newer than what the host already holds is discarded
//!   rather than rendered;
//! * a publisher that disappears is removed from the registry, so the host
//!   fails closed instead of keeping a stale indicator on screen.
//!
//! Several publishers may exist at once. The registry keeps them all; the
//! product layer decides which one it presents.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures::channel::mpsc as async_mpsc;
use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::message::Type as MessageType;
use zbus::zvariant::Value;

/// The interface every indicator publisher implements.
pub const INDICATOR_INTERFACE: &str = "gnos.ux.Indicator1";

/// The object path every indicator publisher serves its properties on.
pub const INDICATOR_OBJECT_PATH: &str = "/gnos/ux/Indicator1";

/// The bus name prefix a publisher owns its well-known name under.
pub const INDICATOR_BUS_NAME_PREFIX: &str = "gnos.ux.Indicator1.";

/// The bus name namespace a host watches for publishers.
///
/// The namespace carries no trailing dot: the bus rejects a namespace that
/// ends with one, and the match it expresses is the same — a publisher name
/// continues the namespace with a dot.
pub const INDICATOR_BUS_NAME_NAMESPACE: &str = "gnos.ux.Indicator1";

/// The control surface a host uses to ask a publisher to act.
pub const INDICATOR_CONTROL_INTERFACE: &str = "gnos.ux.Indicator1Control";

const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
const DBUS_INTERFACE: &str = "org.freedesktop.DBus";

/// The state a publisher can be observed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndicatorState {
    Idle,
    Recording,
    Transcribing,
}

impl IndicatorState {
    /// The wire spelling of this state.
    pub fn as_str(self) -> &'static str {
        match self {
            IndicatorState::Idle => "idle",
            IndicatorState::Recording => "recording",
            IndicatorState::Transcribing => "transcribing",
        }
    }

    /// Parse a wire spelling. Unknown spellings are refused rather than
    /// guessed: an unknown state is a contract violation, not an idle shell.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "idle" => Some(IndicatorState::Idle),
            "recording" => Some(IndicatorState::Recording),
            "transcribing" => Some(IndicatorState::Transcribing),
            _ => None,
        }
    }

    /// Whether this state is worth presenting to the user.
    pub fn is_active(self) -> bool {
        !matches!(self, IndicatorState::Idle)
    }
}

/// Everything the host knows about one publisher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndicatorSnapshot {
    /// Stable identity of the publisher, e.g. `handy`.
    pub publisher_id: String,
    /// Human label a host may display.
    pub title: String,
    /// Icon name a host may resolve in its own icon theme.
    pub icon_name: String,
    /// The current state.
    pub state: IndicatorState,
    /// `active` while the state is worth presenting, `inactive` otherwise.
    pub status: String,
    /// Seconds spent in the current state.
    pub elapsed_seconds: u32,
    /// The live transcript of the current utterance, if any.
    pub text: String,
    /// The model's current hypothesis about the words that follow.
    pub tentative: String,
    /// Strictly monotonic counter, incremented on every published change.
    pub generation: u64,
    /// Monotonic timestamp of the last published change.
    pub updated_at_ms: u64,
}

impl IndicatorSnapshot {
    /// Whether this publisher is currently worth presenting.
    pub fn is_active(&self) -> bool {
        self.state.is_active()
    }
}

/// One observed publisher, with the identity the bus gave us.
#[derive(Clone, Debug)]
struct PublisherEntry {
    well_known_name: String,
    snapshot: IndicatorSnapshot,
}

#[derive(Default)]
struct RegistryInner {
    publishers: Mutex<BTreeMap<String, PublisherEntry>>,
    /// The publisher that holds the surface while it talks.
    holder: Mutex<Option<String>>,
    revision: AtomicU64,
    subscribers: Mutex<Vec<async_mpsc::UnboundedSender<u64>>>,
}

impl RegistryInner {
    fn publish_revision(&self) {
        let revision = self.revision.fetch_add(1, Ordering::AcqRel) + 1;
        let mut subscribers = self
            .subscribers
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        subscribers.retain(|subscriber| subscriber.unbounded_send(revision).is_ok());
    }
}

/// The observed publishers, shared between the observer and the product layer.
#[derive(Clone, Default)]
pub struct IndicatorRegistry {
    inner: Arc<RegistryInner>,
}

impl IndicatorRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Every observed publisher, ordered by publisher id.
    pub fn publishers(&self) -> Vec<IndicatorSnapshot> {
        let publishers = self
            .inner
            .publishers
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        publishers
            .values()
            .map(|entry| entry.snapshot.clone())
            .collect()
    }

    /// The publisher the product presents, if any is active.
    ///
    /// Several publishers may be active at once. The product presents one, and
    /// the choice is deterministic: the first active publisher by id.
    pub fn active_publisher(&self) -> Option<IndicatorSnapshot> {
        let publishers = self
            .inner
            .publishers
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        // The holder keeps the surface while it is talking: a second
        // publisher that starts meanwhile does not interrupt the dictation in
        // progress.
        let holder = self
            .inner
            .holder
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        if let Some(name) = holder
            && let Some(entry) = publishers.get(&name)
            && entry.snapshot.is_active()
        {
            return Some(entry.snapshot.clone());
        }
        // Otherwise, whoever published last is the one presented: the bus's
        // order of arrival does not decide what is on screen.
        let (name, snapshot) = publishers
            .iter()
            .filter(|(_, entry)| entry.snapshot.is_active())
            .max_by_key(|(_, entry)| entry.snapshot.updated_at_ms)
            .map(|(name, entry)| (name.clone(), entry.snapshot.clone()))?;
        drop(publishers);
        *self
            .inner
            .holder
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(name);
        Some(snapshot)
    }

    /// The well-known name of the publisher a host acts on.
    ///
    /// The active publisher if there is one — a host that clicks a recording
    /// wants to stop that recording. Otherwise the publisher that published
    /// last: the button starts a dictation, and the application that owns the
    /// microphone is the one that published at all, idle or not.
    pub fn control_target(&self) -> Option<String> {
        let publishers = self
            .inner
            .publishers
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let active = publishers
            .values()
            .filter(|entry| entry.snapshot.is_active())
            .max_by_key(|entry| entry.snapshot.updated_at_ms);
        let target = active.or_else(|| {
            publishers
                .values()
                .max_by_key(|entry| entry.snapshot.generation)
        })?;
        Some(target.well_known_name.clone())
    }

    /// The revision of the registry, incremented on every change.
    pub fn revision(&self) -> u64 {
        self.inner.revision.load(Ordering::Acquire)
    }

    /// Subscribe to registry changes. Each message carries the new revision.
    pub fn subscribe(&self) -> Result<async_mpsc::UnboundedReceiver<u64>, String> {
        let (sender, receiver) = async_mpsc::unbounded();
        let mut subscribers = self
            .inner
            .subscribers
            .lock()
            .map_err(|_| "indicator_subscribers_poisoned".to_owned())?;
        subscribers.push(sender);
        Ok(receiver)
    }

    /// Record a publisher that appeared, reading its published state.
    fn upsert_publisher(
        &self,
        well_known_name: &str,
        unique_name: &str,
        snapshot: IndicatorSnapshot,
    ) {
        let mut snapshot = snapshot;
        // The publisher names itself in its own property; the bus name is the
        // fallback when it does not.
        if snapshot.publisher_id.is_empty() {
            snapshot.publisher_id = publisher_id_of(well_known_name);
        }
        let mut publishers = self
            .inner
            .publishers
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        publishers.insert(
            unique_name.to_owned(),
            PublisherEntry {
                well_known_name: well_known_name.to_owned(),
                snapshot,
            },
        );
        drop(publishers);
        self.inner.publish_revision();
    }

    /// Forget a publisher that disappeared. The host then shows nothing, which
    /// is the fail-closed behaviour a vanished publisher deserves.
    fn remove_publisher(&self, well_known_name: &str) {
        let mut publishers = self
            .inner
            .publishers
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let before = publishers.len();
        publishers.retain(|_, entry| entry.well_known_name != well_known_name);
        let changed = publishers.len() != before;
        if changed {
            let mut holder = self
                .inner
                .holder
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if holder
                .as_ref()
                .is_some_and(|name| !publishers.contains_key(name))
            {
                *holder = None;
            }
        }
        drop(publishers);
        if changed {
            self.inner.publish_revision();
        }
    }

    /// Apply a published property change to the publisher that sent it.
    fn apply_properties(&self, unique_name: &str, changed: &HashMap<String, Value<'_>>) {
        let mut publishers = self
            .inner
            .publishers
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let Some(entry) = publishers.get_mut(unique_name) else {
            return;
        };
        // A change that violates the contract, or that carries no generation,
        // is refused whole: the freshness fence needs the generation, and a
        // partial application would leave a half-updated state on screen.
        if !change_matches_the_contract(changed) || !changed.contains_key("Generation") {
            return;
        }
        let mut next = entry.snapshot.clone();
        let mut touched = false;
        if let Some(value) = changed_string(changed, "PublisherId") {
            next.publisher_id = value;
            touched = true;
        }
        if let Some(value) = changed_string(changed, "Title") {
            next.title = value;
            touched = true;
        }
        if let Some(value) = changed_string(changed, "IconName") {
            next.icon_name = value;
            touched = true;
        }
        if let Some(value) = changed_string(changed, "State") {
            // An unknown spelling is refused: the previous state is kept
            // rather than silently becoming idle.
            if let Some(state) = IndicatorState::parse(&value) {
                next.state = state;
                touched = true;
            }
        }
        if let Some(value) = changed_string(changed, "Status") {
            next.status = value;
            touched = true;
        }
        if let Some(value) = changed_u32(changed, "ElapsedSeconds") {
            next.elapsed_seconds = value;
            touched = true;
        }
        if let Some(value) = changed_string(changed, "Text") {
            next.text = value;
            touched = true;
        }
        if let Some(value) = changed_string(changed, "Tentative") {
            next.tentative = value;
            touched = true;
        }
        if let Some(value) = changed_u64(changed, "Generation") {
            next.generation = value;
            touched = true;
        }
        if let Some(value) = changed_u64(changed, "UpdatedAt") {
            next.updated_at_ms = value;
            touched = true;
        }
        if !touched {
            return;
        }
        // A publisher that republishes an older generation is out of order; the
        // host keeps what it already holds instead of rendering a resurrected
        // state.
        if next.generation <= entry.snapshot.generation {
            return;
        }
        entry.snapshot = next;
        drop(publishers);
        self.inner.publish_revision();
    }
}

/// Read a string property from a published change.
fn changed_string(changed: &HashMap<String, Value<'_>>, name: &str) -> Option<String> {
    changed.get(name)?.downcast_ref::<String>().ok()
}

/// Read a 32-bit property from a published change.
fn changed_u32(changed: &HashMap<String, Value<'_>>, name: &str) -> Option<u32> {
    changed.get(name)?.downcast_ref::<u32>().ok()
}

/// Read a 64-bit property from a published change.
fn changed_u64(changed: &HashMap<String, Value<'_>>, name: &str) -> Option<u64> {
    changed.get(name)?.downcast_ref::<u64>().ok()
}

/// Whether a published change carries only properties of the expected type.
///
/// A change that violates the contract is refused whole. Applying the
/// readable part of it would leave the host showing a half-updated state —
/// an elapsed time that never moves next to a text that does.
fn change_matches_the_contract(changed: &HashMap<String, Value<'_>>) -> bool {
    let string = [
        "PublisherId",
        "Title",
        "IconName",
        "State",
        "Status",
        "Text",
    ];
    let number32 = ["ElapsedSeconds"];
    let number64 = ["Generation", "UpdatedAt"];
    string.iter().all(|name| match changed.get(*name) {
        None => true,
        Some(value) => value.downcast_ref::<String>().is_ok(),
    }) && number32.iter().all(|name| match changed.get(*name) {
        None => true,
        Some(value) => value.downcast_ref::<u32>().is_ok(),
    }) && number64.iter().all(|name| match changed.get(*name) {
        None => true,
        Some(value) => value.downcast_ref::<u64>().is_ok(),
    })
}

/// The publisher id carried by a well-known name.
fn publisher_id_of(well_known_name: &str) -> String {
    well_known_name
        .strip_prefix(INDICATOR_BUS_NAME_PREFIX)
        .unwrap_or(well_known_name)
        .to_owned()
}

/// Read the published state of a publisher.
///
/// The properties are read one by one rather than through `GetAll`: a typed
/// read keeps the wire values typed, and a publisher that has already gone
/// away fails the read instead of yielding an empty snapshot.
fn read_snapshot(
    connection: &Connection,
    well_known_name: &str,
) -> Result<IndicatorSnapshot, String> {
    let proxy = Proxy::new(
        connection,
        well_known_name,
        INDICATOR_OBJECT_PATH,
        INDICATOR_INTERFACE,
    )
    .map_err(|error| error.to_string())?;
    let publisher_id = proxy
        .get_property::<String>("PublisherId")
        .map_err(|error| error.to_string())?;
    // The generation is the freshness fence. A publisher that does not carry
    // one cannot be fenced, so it is refused rather than watched.
    proxy
        .get_property::<u64>("Generation")
        .map_err(|error| error.to_string())?;
    Ok(IndicatorSnapshot {
        publisher_id,
        title: proxy.get_property::<String>("Title").unwrap_or_default(),
        icon_name: proxy.get_property::<String>("IconName").unwrap_or_default(),
        state: proxy
            .get_property::<String>("State")
            .ok()
            .as_deref()
            .and_then(IndicatorState::parse)
            .unwrap_or(IndicatorState::Idle),
        status: proxy.get_property::<String>("Status").unwrap_or_default(),
        tentative: proxy
            .get_property::<String>("Tentative")
            .unwrap_or_default(),
        elapsed_seconds: proxy
            .get_property::<u32>("ElapsedSeconds")
            .unwrap_or_default(),
        text: proxy.get_property::<String>("Text").unwrap_or_default(),
        generation: proxy.get_property::<u64>("Generation").unwrap_or_default(),
        updated_at_ms: proxy.get_property::<u64>("UpdatedAt").unwrap_or_default(),
    })
}

/// Read the initial state of every publisher already on the bus.
fn discover_existing(connection: &Connection, registry: &IndicatorRegistry) -> Result<(), String> {
    let proxy = Proxy::new(
        connection,
        DBUS_INTERFACE,
        "/org/freedesktop/DBus",
        DBUS_INTERFACE,
    )
    .map_err(|error| error.to_string())?;
    let names: Vec<String> = proxy
        .call("ListNames", &())
        .map_err(|error| error.to_string())?;
    for name in names {
        if !name.starts_with(INDICATOR_BUS_NAME_PREFIX) {
            continue;
        }
        let Ok(owner) = proxy.call::<_, _, String>("GetNameOwner", &(name.clone(),)) else {
            continue;
        };
        let snapshot = match read_snapshot(connection, &name) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                println!("GNOS_UX_INDICATOR_READ_FAILED name={name} reason={error}");
                continue;
            }
        };
        println!(
            "GNOS_UX_INDICATOR_DISCOVERED publisher={} name={} state={}",
            snapshot.publisher_id,
            name,
            snapshot.state.as_str()
        );
        registry.upsert_publisher(&name, &owner, snapshot);
    }
    Ok(())
}

/// Watch for publishers appearing and disappearing.
fn observe_discovery(connection: &Connection, registry: &IndicatorRegistry) -> Result<(), String> {
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .interface(DBUS_INTERFACE)
        .map_err(|error| error.to_string())?
        .member("NameOwnerChanged")
        .map_err(|error| error.to_string())?
        .arg0ns(INDICATOR_BUS_NAME_NAMESPACE)
        .map_err(|error| error.to_string())?
        .build();
    let stream = MessageIterator::for_match_rule(rule, connection, Some(64))
        .map_err(|error| error.to_string())?;
    for message in stream {
        let Ok(message) = message else { continue };
        // The bus carries the owner as a plain string; an empty owner means
        // the name has no owner any more.
        let Ok((name, _previous_owner, new_owner)) =
            message.body().deserialize::<(String, String, String)>()
        else {
            continue;
        };
        if !name.starts_with(INDICATOR_BUS_NAME_PREFIX) {
            continue;
        }
        if new_owner.is_empty() {
            println!("GNOS_UX_INDICATOR_REMOVED name={name}");
            registry.remove_publisher(&name);
            continue;
        }
        match read_snapshot(connection, &name) {
            Ok(snapshot) => {
                println!(
                    "GNOS_UX_INDICATOR_DISCOVERED publisher={} name={} state={}",
                    snapshot.publisher_id,
                    name,
                    snapshot.state.as_str()
                );
                registry.upsert_publisher(&name, &new_owner, snapshot);
            }
            Err(error) => {
                println!("GNOS_UX_INDICATOR_READ_FAILED name={name} reason={error}");
            }
        }
    }
    Ok(())
}

/// Watch the published state of every publisher.
fn observe_properties(connection: &Connection, registry: &IndicatorRegistry) -> Result<(), String> {
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .interface(PROPERTIES_INTERFACE)
        .map_err(|error| error.to_string())?
        .member("PropertiesChanged")
        .map_err(|error| error.to_string())?
        .path(INDICATOR_OBJECT_PATH)
        .map_err(|error| error.to_string())?
        .build();
    let stream = MessageIterator::for_match_rule(rule, connection, Some(256))
        .map_err(|error| error.to_string())?;
    for message in stream {
        let Ok(message) = message else { continue };
        let Some(sender) = message.header().sender().map(|sender| sender.to_string()) else {
            continue;
        };
        let body = message.body();
        let Ok((interface, changed, _invalidated)) =
            body.deserialize::<(String, HashMap<String, Value>, Vec<String>)>()
        else {
            continue;
        };
        if interface != INDICATOR_INTERFACE {
            continue;
        }
        registry.apply_properties(&sender, &changed);
    }
    Ok(())
}

/// Start observing publishers on the session bus.
///
/// The observer owns no authority: it reads published properties and publishes
/// them to the registry. A failure to reach the bus leaves the registry empty,
/// which the product presents as "nothing to show".
pub fn observe(registry: IndicatorRegistry) -> Vec<std::thread::JoinHandle<()>> {
    let discovery_registry = registry.clone();
    let properties_registry = registry.clone();
    vec![
        std::thread::spawn(move || {
            let Ok(connection) = Connection::session() else {
                eprintln!("GNOS_UX_INDICATOR_SESSION_BUS_UNAVAILABLE");
                return;
            };
            if let Err(error) = discover_existing(&connection, &discovery_registry) {
                eprintln!("GNOS_UX_INDICATOR_DISCOVERY_FAILED {error}");
            }
            if let Err(error) = observe_discovery(&connection, &discovery_registry) {
                eprintln!("GNOS_UX_INDICATOR_DISCOVERY_STOPPED {error}");
            }
        }),
        std::thread::spawn(move || {
            let Ok(connection) = Connection::session() else {
                eprintln!("GNOS_UX_INDICATOR_SESSION_BUS_UNAVAILABLE");
                return;
            };
            if let Err(error) = observe_properties(&connection, &properties_registry) {
                eprintln!("GNOS_UX_INDICATOR_PROPERTIES_STOPPED {error}");
            }
        }),
    ]
}

/// Ask a publisher to toggle its dictation.
///
/// The operation set is one: the same toggle the keyboard shortcut carries.
/// A host that only observes never calls this.
pub fn toggle_publisher(well_known_name: &str) -> Result<(), String> {
    let connection = Connection::session().map_err(|error| error.to_string())?;
    let proxy = Proxy::new(
        &connection,
        well_known_name,
        INDICATOR_OBJECT_PATH,
        INDICATOR_CONTROL_INTERFACE,
    )
    .map_err(|error| error.to_string())?;
    proxy.call("Toggle", &()).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publisher_id_is_the_suffix_of_the_well_known_name() {
        assert_eq!(publisher_id_of("gnos.ux.Indicator1.handy"), "handy");
        assert_eq!(publisher_id_of("gnos.ux.Indicator1.other"), "other");
    }

    #[test]
    fn unknown_states_are_refused() {
        assert_eq!(IndicatorState::parse("idle"), Some(IndicatorState::Idle));
        assert_eq!(
            IndicatorState::parse("recording"),
            Some(IndicatorState::Recording)
        );
        assert_eq!(
            IndicatorState::parse("transcribing"),
            Some(IndicatorState::Transcribing)
        );
        assert_eq!(IndicatorState::parse("sleeping"), None);
    }

    #[test]
    fn an_empty_registry_presents_nothing() {
        let registry = IndicatorRegistry::new();
        assert!(registry.publishers().is_empty());
        assert!(registry.active_publisher().is_none());
    }

    #[test]
    fn a_vanished_publisher_is_forgotten() {
        let registry = IndicatorRegistry::new();
        let snapshot = IndicatorSnapshot {
            publisher_id: "handy".to_owned(),
            title: "Handy".to_owned(),
            icon_name: "audio-input-microphone".to_owned(),
            state: IndicatorState::Recording,
            status: "active".to_owned(),
            elapsed_seconds: 3,
            text: "bonjour".to_owned(),
            tentative: String::new(),
            generation: 7,
            updated_at_ms: 1_000,
        };
        registry.upsert_publisher("gnos.ux.Indicator1.handy", ":1.42", snapshot);
        assert_eq!(
            registry.active_publisher().map(|s| s.publisher_id),
            Some("handy".to_owned())
        );
        registry.remove_publisher("gnos.ux.Indicator1.handy");
        assert!(registry.active_publisher().is_none());
    }

    #[test]
    fn an_older_generation_is_discarded() {
        let registry = IndicatorRegistry::new();
        let snapshot = IndicatorSnapshot {
            publisher_id: "handy".to_owned(),
            title: "Handy".to_owned(),
            icon_name: "audio-input-microphone".to_owned(),
            state: IndicatorState::Recording,
            status: "active".to_owned(),
            elapsed_seconds: 3,
            text: "bonjour".to_owned(),
            tentative: String::new(),
            generation: 7,
            updated_at_ms: 1_000,
        };
        registry.upsert_publisher("gnos.ux.Indicator1.handy", ":1.42", snapshot);
        let mut changed: HashMap<String, Value<'static>> = HashMap::new();
        changed.insert("Generation".to_owned(), Value::new(5u64));
        changed.insert("State".to_owned(), Value::new("idle".to_owned()));
        registry.apply_properties(":1.42", &changed);
        let held = registry.active_publisher().expect("publisher is held");
        assert_eq!(held.state, IndicatorState::Recording);
        assert_eq!(held.generation, 7);
    }

    #[test]
    fn a_change_that_breaks_the_contract_is_discarded_whole() {
        let registry = IndicatorRegistry::new();
        let snapshot = IndicatorSnapshot {
            publisher_id: "handy".to_owned(),
            title: "Handy".to_owned(),
            icon_name: "audio-input-microphone".to_owned(),
            state: IndicatorState::Recording,
            status: "active".to_owned(),
            elapsed_seconds: 3,
            text: "bonjour".to_owned(),
            tentative: String::new(),
            generation: 7,
            updated_at_ms: 1_000,
        };
        registry.upsert_publisher("gnos.ux.Indicator1.handy", ":1.42", snapshot);
        // The elapsed time is a 32-bit property. A publisher that sends it as
        // 64 bits breaks the contract, and the whole change is refused: the
        // host must not show a text that moved next to an elapsed time that
        // did not.
        let mut changed: HashMap<String, Value<'static>> = HashMap::new();
        changed.insert("Generation".to_owned(), Value::new(9u64));
        changed.insert("Text".to_owned(), Value::new("bonjour le monde".to_owned()));
        changed.insert("ElapsedSeconds".to_owned(), Value::new(4u64));
        registry.apply_properties(":1.42", &changed);
        let held = registry.active_publisher().expect("publisher is held");
        assert_eq!(held.generation, 7);
        assert_eq!(held.text, "bonjour");
        assert_eq!(held.elapsed_seconds, 3);
    }

    #[test]
    fn a_change_without_a_generation_is_discarded() {
        let registry = IndicatorRegistry::new();
        let snapshot = IndicatorSnapshot {
            publisher_id: "handy".to_owned(),
            title: "Handy".to_owned(),
            icon_name: "audio-input-microphone".to_owned(),
            state: IndicatorState::Recording,
            status: "active".to_owned(),
            elapsed_seconds: 3,
            text: "bonjour".to_owned(),
            tentative: String::new(),
            generation: 7,
            updated_at_ms: 1_000,
        };
        registry.upsert_publisher("gnos.ux.Indicator1.handy", ":1.42", snapshot);
        // Without a generation there is no freshness fence, so the change
        // cannot be placed in time and is refused.
        let mut changed: HashMap<String, Value<'static>> = HashMap::new();
        changed.insert("Text".to_owned(), Value::new("bonjour le monde".to_owned()));
        registry.apply_properties(":1.42", &changed);
        let held = registry.active_publisher().expect("publisher is held");
        assert_eq!(held.text, "bonjour");
        assert_eq!(held.generation, 7);
    }

    fn snapshot_of(publisher_id: &str, generation: u64, updated_at_ms: u64) -> IndicatorSnapshot {
        IndicatorSnapshot {
            publisher_id: publisher_id.to_owned(),
            title: publisher_id.to_owned(),
            icon_name: "microphone".to_owned(),
            state: IndicatorState::Recording,
            status: "active".to_owned(),
            elapsed_seconds: 1,
            text: String::new(),
            tentative: String::new(),
            generation,
            updated_at_ms,
        }
    }

    #[test]
    fn a_second_publisher_does_not_take_the_surface_from_the_one_talking() {
        let registry = IndicatorRegistry::new();
        registry.upsert_publisher(
            "gnos.ux.Indicator1.handy",
            ":1.1",
            snapshot_of("handy", 7, 100),
        );
        assert_eq!(
            registry.active_publisher().map(|s| s.publisher_id),
            Some("handy".to_owned())
        );
        // A second application starts talking, and published more recently.
        registry.upsert_publisher(
            "gnos.ux.Indicator1.demo",
            ":1.2",
            snapshot_of("demo", 1, 200),
        );
        // The dictation in progress is not interrupted.
        assert_eq!(
            registry.active_publisher().map(|s| s.publisher_id),
            Some("handy".to_owned())
        );
        // Once the holder goes idle, the surface goes to whoever published last.
        let mut idle = snapshot_of("handy", 9, 300);
        idle.state = IndicatorState::Idle;
        idle.status = "inactive".to_owned();
        registry.upsert_publisher("gnos.ux.Indicator1.handy", ":1.1", idle);
        assert_eq!(
            registry.active_publisher().map(|s| s.publisher_id),
            Some("demo".to_owned())
        );
    }

    #[test]
    fn a_newer_generation_is_applied() {
        let registry = IndicatorRegistry::new();
        let snapshot = IndicatorSnapshot {
            publisher_id: "handy".to_owned(),
            title: "Handy".to_owned(),
            icon_name: "audio-input-microphone".to_owned(),
            state: IndicatorState::Recording,
            status: "active".to_owned(),
            elapsed_seconds: 3,
            text: "bonjour".to_owned(),
            tentative: String::new(),
            generation: 7,
            updated_at_ms: 1_000,
        };
        registry.upsert_publisher("gnos.ux.Indicator1.handy", ":1.42", snapshot);
        let mut changed: HashMap<String, Value<'static>> = HashMap::new();
        changed.insert("Generation".to_owned(), Value::new(9u64));
        changed.insert("State".to_owned(), Value::new("transcribing".to_owned()));
        changed.insert("Text".to_owned(), Value::new("bonjour le monde".to_owned()));
        registry.apply_properties(":1.42", &changed);
        let held = registry.active_publisher().expect("publisher is held");
        assert_eq!(held.state, IndicatorState::Transcribing);
        assert_eq!(held.text, "bonjour le monde");
        assert_eq!(held.generation, 9);
    }
}
