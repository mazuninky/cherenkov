//! [`Session`], [`SessionId`], and the [`SessionRegistry`].
//!
//! The registry is the hub's view of currently-connected clients. It is
//! sharded by [`DashMap`] for low-tail-latency reads and writes on the hot
//! subscribe / unsubscribe / fan-out paths (`docs/plan.md` §7).
//!
//! Two indices are maintained side by side:
//!
//! * `id → Session` — primary lookup by session id.
//! * `channel → Vec<SessionId>` — reverse index used by transports and
//!   admin tooling to enumerate the local subscribers of a channel.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use cherenkov_protocol::ServerFrame;
use dashmap::DashMap;
use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;

use crate::SessionClaims;

/// Opaque, monotonically-increasing identifier for a connected session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(pub u64);

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A single connected client.
///
/// Sessions are created by transports (e.g. on WebSocket upgrade) and then
/// registered with [`SessionRegistry`]. The transport spawns a forwarder
/// that reads from [`Session::outbox`] and writes to the wire; the hub
/// pushes [`ServerFrame`]s into the outbox.
#[derive(Debug)]
pub struct Session {
    id: SessionId,
    outbox: mpsc::Sender<ServerFrame>,
    /// Spawned forwarder tasks per channel, one per active subscription.
    /// Dropping a session aborts all of them.
    subscriptions: DashMap<String, JoinHandle<()>>,
    /// Authenticated claims, set by [`crate::Hub::handle_connect`]. The
    /// hub also seeds anonymous claims for every newly-opened session
    /// when the configured authenticator allows anonymous access.
    claims: ArcSwapOption<SessionClaims>,
    /// Triggered by [`crate::Hub::kick_session`] to ask the owning
    /// transport to tear down the underlying connection.
    shutdown: Arc<Notify>,
}

impl Session {
    /// Construct a new session bound to `outbox`. Claims are unset until
    /// [`Session::set_claims`] is called.
    #[must_use]
    pub fn new(id: SessionId, outbox: mpsc::Sender<ServerFrame>) -> Self {
        Self {
            id,
            outbox,
            subscriptions: DashMap::new(),
            claims: ArcSwapOption::new(None),
            shutdown: Arc::new(Notify::new()),
        }
    }

    /// Notifier that fires when the hub asks this session to shut down.
    /// Transports await it alongside their normal read loop and tear
    /// down the underlying connection when it triggers.
    #[must_use]
    pub fn shutdown_notifier(&self) -> Arc<Notify> {
        self.shutdown.clone()
    }

    /// Trigger the shutdown notifier so any awaiting transport tears
    /// down the connection.
    pub fn signal_shutdown(&self) {
        self.shutdown.notify_waiters();
    }

    /// Set the session claims. Returns the previous value, if any.
    pub fn set_claims(&self, claims: Arc<SessionClaims>) -> Option<Arc<SessionClaims>> {
        self.claims.swap(Some(claims))
    }

    /// Current claims, or `None` if the session has not yet connected.
    #[must_use]
    pub fn claims(&self) -> Option<Arc<SessionClaims>> {
        self.claims.load_full()
    }

    /// Session id.
    #[must_use]
    pub fn id(&self) -> SessionId {
        self.id
    }

    /// Sender half of the outbound frame channel; transports drain this
    /// into the wire.
    #[must_use]
    pub fn outbox(&self) -> &mpsc::Sender<ServerFrame> {
        &self.outbox
    }

    /// Channels this session is currently subscribed to.
    #[must_use]
    pub fn channels(&self) -> Vec<String> {
        self.subscriptions.iter().map(|e| e.key().clone()).collect()
    }

    /// Returns true if the session is already subscribed to `channel`.
    pub(crate) fn has_subscription(&self, channel: &str) -> bool {
        self.subscriptions.contains_key(channel)
    }

    pub(crate) fn insert_subscription(
        &self,
        channel: String,
        forwarder: JoinHandle<()>,
    ) -> Result<(), JoinHandle<()>> {
        // Use the entry API so the existence check and insert happen under
        // a single per-shard write lock; a `contains_key` + `insert` pair
        // would race when two `handle_subscribe` calls land for the same
        // session-channel concurrently and could orphan a forwarder task.
        use dashmap::mapref::entry::Entry;
        match self.subscriptions.entry(channel) {
            Entry::Occupied(_) => Err(forwarder),
            Entry::Vacant(slot) => {
                slot.insert(forwarder);
                Ok(())
            }
        }
    }

    pub(crate) fn remove_subscription(&self, channel: &str) -> Option<JoinHandle<()>> {
        self.subscriptions.remove(channel).map(|(_, task)| task)
    }

    /// Abort every subscription forwarder owned by this session.
    pub(crate) fn abort_all(&self) {
        // `retain(|_, _| false)` clears the map and lets us call abort on
        // each removed task.
        self.subscriptions.retain(|_, task| {
            task.abort();
            false
        });
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.abort_all();
    }
}

/// Sharded registry of active sessions plus a reverse channel index.
#[derive(Debug, Default)]
pub struct SessionRegistry {
    next_id: AtomicU64,
    sessions: DashMap<SessionId, Arc<Session>>,
    channels: DashMap<String, Vec<SessionId>>,
}

impl SessionRegistry {
    /// Construct an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a fresh, monotonic [`SessionId`].
    pub fn next_id(&self) -> SessionId {
        SessionId(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Register `session`.
    ///
    /// Returns the registered handle. Replacing an existing session with the
    /// same id is a programming error and triggers a `debug_assert!` in dev
    /// builds.
    pub fn register(&self, session: Arc<Session>) -> Arc<Session> {
        let id = session.id();
        debug_assert!(
            !self.sessions.contains_key(&id),
            "session {id} registered twice",
        );
        self.sessions.insert(id, session.clone());
        session
    }

    /// Fetch a session by id.
    #[must_use]
    pub fn get(&self, id: &SessionId) -> Option<Arc<Session>> {
        self.sessions.get(id).map(|e| e.clone())
    }

    /// Remove a session and its channel-index entries; aborts any in-flight
    /// subscription forwarders.
    pub fn deregister(&self, id: &SessionId) -> Option<Arc<Session>> {
        let removed = self.sessions.remove(id).map(|(_, s)| s);
        if removed.is_some() {
            // Scrub the reverse channel index in one pass.
            self.channels.retain(|_, ids| {
                ids.retain(|sid| sid != id);
                !ids.is_empty()
            });
        }
        removed
    }

    /// Currently-registered session count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// True if no sessions are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Snapshot of every currently-registered session.
    ///
    /// Returns owned [`Arc`] handles so callers may iterate without
    /// holding any DashMap shard lock. Order is unspecified.
    #[must_use]
    pub fn snapshot(&self) -> Vec<Arc<Session>> {
        self.sessions.iter().map(|e| e.value().clone()).collect()
    }

    /// Snapshot of every registered [`SessionId`].
    #[must_use]
    pub fn ids(&self) -> Vec<SessionId> {
        self.sessions.iter().map(|e| *e.key()).collect()
    }

    /// Sessions currently subscribed to `channel`.
    ///
    /// Returns a snapshot — callers may iterate without holding any locks.
    #[must_use]
    pub fn subscribers_of(&self, channel: &str) -> Vec<SessionId> {
        self.channels
            .get(channel)
            .map(|e| e.value().clone())
            .unwrap_or_default()
    }

    pub(crate) fn add_subscription(&self, id: SessionId, channel: &str) {
        self.channels
            .entry(channel.to_owned())
            .or_default()
            .push(id);
    }

    pub(crate) fn remove_subscription(&self, id: &SessionId, channel: &str) {
        let mut should_remove = false;
        if let Some(mut entry) = self.channels.get_mut(channel) {
            entry.retain(|sid| sid != id);
            should_remove = entry.is_empty();
        }
        if should_remove {
            self.channels.remove(channel);
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::*;

    fn make_session(reg: &SessionRegistry) -> Arc<Session> {
        let (tx, _rx) = mpsc::channel(16);
        Arc::new(Session::new(reg.next_id(), tx))
    }

    #[test]
    fn ids_are_monotonic() {
        let reg = SessionRegistry::new();
        let a = reg.next_id();
        let b = reg.next_id();
        assert!(b.0 > a.0);
    }

    #[test]
    fn register_and_get_round_trip() {
        let reg = SessionRegistry::new();
        let s = make_session(&reg);
        let id = s.id();
        reg.register(s);

        let fetched = reg.get(&id).expect("registered session is fetchable");
        assert_eq!(fetched.id(), id);
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn deregister_clears_reverse_index() {
        let reg = SessionRegistry::new();
        let s = make_session(&reg);
        let id = s.id();
        reg.register(s);
        reg.add_subscription(id, "rooms.lobby");
        assert_eq!(reg.subscribers_of("rooms.lobby"), vec![id]);

        reg.deregister(&id);
        assert!(reg.is_empty());
        assert!(reg.subscribers_of("rooms.lobby").is_empty());
    }

    #[test]
    fn add_and_remove_subscription_updates_reverse_index() {
        let reg = SessionRegistry::new();
        let s = make_session(&reg);
        let id = s.id();
        reg.register(s);

        reg.add_subscription(id, "ch");
        assert_eq!(reg.subscribers_of("ch"), vec![id]);

        reg.remove_subscription(&id, "ch");
        assert!(reg.subscribers_of("ch").is_empty());
    }
}
