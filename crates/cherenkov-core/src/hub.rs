//! [`Hub`] — the central router that ties [`Session`]s, [`ChannelKind`]s,
//! and a [`Broker`] together.
//!
//! At M1 the hub is namespace-agnostic: it routes every channel through a
//! single registered [`ChannelKind`]. Multi-kind routing (e.g. one kind for
//! `pubsub:*` and another for `crdt:*`) is reserved for a later milestone.

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use cherenkov_protocol::{
    ConnectOk as ProtocolConnectOk, Publication, ServerFrame, SubscribeOk as ProtocolSubscribeOk,
    UnsubscribeOk as ProtocolUnsubscribeOk,
};
use futures::StreamExt;
use tokio::sync::mpsc;
use tracing::{debug, instrument, warn};

use crate::{
    AclAction, AclChecker, AclDecision, AclError, AllowAllAcl, AllowAllAuthenticator,
    AllowAllValidator, Authenticator, Broker, ChannelKind, HubError, SchemaValidator, Session,
    SessionClaims, SessionId, SessionRegistry, Transport,
};

/// Builder for a [`Hub`]: declare the channel kind, broker, optional schema
/// validator, optional authenticator + ACL checker, and the transports that
/// should be served when [`HubBuilder::build`] is called.
#[derive(Default)]
pub struct HubBuilder {
    kind: Option<Arc<dyn ChannelKind>>,
    namespace_kinds: HashMap<String, Arc<dyn ChannelKind>>,
    broker: Option<Arc<dyn Broker>>,
    validator: Option<Arc<dyn SchemaValidator>>,
    authenticator: Option<Arc<dyn Authenticator>>,
    acl: Option<Arc<dyn AclChecker>>,
    transports: Vec<Box<dyn Transport>>,
}

impl HubBuilder {
    /// Construct an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the default [`ChannelKind`] used for any channel whose
    /// namespace was not registered with
    /// [`HubBuilder::with_channel_kind_for`].
    #[must_use]
    pub fn with_channel_kind(mut self, kind: Arc<dyn ChannelKind>) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Route channels in `namespace` (the part before the first `.`) to
    /// `kind`. Channels in unregistered namespaces fall back to the
    /// default kind from [`HubBuilder::with_channel_kind`].
    #[must_use]
    pub fn with_channel_kind_for(
        mut self,
        namespace: impl Into<String>,
        kind: Arc<dyn ChannelKind>,
    ) -> Self {
        self.namespace_kinds.insert(namespace.into(), kind);
        self
    }

    /// Register the [`Broker`] to use for cross-node and local fan-out.
    #[must_use]
    pub fn with_broker(mut self, broker: Arc<dyn Broker>) -> Self {
        self.broker = Some(broker);
        self
    }

    /// Register the [`SchemaValidator`] consulted on every `Publish` before
    /// the channel kind sees the data.
    ///
    /// If unset, the hub builds with [`AllowAllValidator`], which accepts
    /// every payload — matching the M1 behavior for backwards compatibility.
    #[must_use]
    pub fn with_schema_validator(mut self, validator: Arc<dyn SchemaValidator>) -> Self {
        self.validator = Some(validator);
        self
    }

    /// Register the [`Authenticator`] consulted on every `Connect` frame.
    ///
    /// If unset, the hub builds with [`AllowAllAuthenticator`], which
    /// accepts every token as an anonymous session and lets transports
    /// skip the connect handshake entirely.
    #[must_use]
    pub fn with_authenticator(mut self, auth: Arc<dyn Authenticator>) -> Self {
        self.authenticator = Some(auth);
        self
    }

    /// Register the [`AclChecker`] consulted on every `Subscribe` and
    /// `Publish` before the channel kind sees the data.
    ///
    /// If unset, the hub builds with [`AllowAllAcl`], which permits every
    /// action — matching the M1 / M2 behavior for backwards compatibility.
    #[must_use]
    pub fn with_acl_checker(mut self, acl: Arc<dyn AclChecker>) -> Self {
        self.acl = Some(acl);
        self
    }

    /// Register a [`Transport`] alongside the hub. Transports are returned
    /// from [`HubBuilder::build`] in [`HubBuilt::transports`] for the
    /// caller to drive via [`Transport::serve`].
    #[must_use]
    pub fn with_transport(mut self, transport: Box<dyn Transport>) -> Self {
        self.transports.push(transport);
        self
    }

    /// Finalize the builder.
    ///
    /// # Errors
    ///
    /// Returns an error if no channel kind or broker has been registered.
    pub fn build(self) -> Result<HubBuilt, &'static str> {
        let kind = self.kind.ok_or("no channel kind registered")?;
        let broker = self.broker.ok_or("no broker registered")?;
        let validator = self
            .validator
            .unwrap_or_else(|| Arc::new(AllowAllValidator));
        let authenticator: Arc<dyn Authenticator> = self
            .authenticator
            .unwrap_or_else(|| Arc::new(AllowAllAuthenticator));
        let acl = self.acl.unwrap_or_else(|| Arc::new(AllowAllAcl));
        let hub = Hub {
            sessions: Arc::new(SessionRegistry::new()),
            kind,
            namespace_kinds: Arc::new(self.namespace_kinds),
            broker,
            validator,
            authenticator,
            acl,
        };
        Ok(HubBuilt {
            hub,
            transports: self.transports,
        })
    }
}

/// Output of [`HubBuilder::build`]: the [`Hub`] plus the registered
/// transports awaiting [`Transport::serve`].
pub struct HubBuilt {
    /// The constructed hub.
    pub hub: Hub,
    /// Transports registered via [`HubBuilder::with_transport`], in order
    /// of registration.
    pub transports: Vec<Box<dyn Transport>>,
}

impl HubBuilt {
    /// Destructure into `(hub, transports)`.
    #[must_use]
    pub fn split(self) -> (Hub, Vec<Box<dyn Transport>>) {
        (self.hub, self.transports)
    }
}

/// The central message router.
///
/// `Hub` is cheap to clone — internally it holds `Arc`s — and is the type
/// transports interact with for every inbound frame.
#[derive(Clone)]
pub struct Hub {
    sessions: Arc<SessionRegistry>,
    kind: Arc<dyn ChannelKind>,
    namespace_kinds: Arc<HashMap<String, Arc<dyn ChannelKind>>>,
    broker: Arc<dyn Broker>,
    validator: Arc<dyn SchemaValidator>,
    authenticator: Arc<dyn Authenticator>,
    acl: Arc<dyn AclChecker>,
}

/// The part of a channel name before the first `.`, used to look up the
/// per-namespace channel kind in the hub. Channels without a `.` use the
/// whole channel name as the namespace.
#[must_use]
pub fn namespace_of(channel: &str) -> &str {
    channel.split_once('.').map_or(channel, |(ns, _)| ns)
}

impl Hub {
    /// Access the session registry; transports use this to register a
    /// session on connect and to deregister it on disconnect.
    #[must_use]
    pub fn sessions(&self) -> &Arc<SessionRegistry> {
        &self.sessions
    }

    /// Allocate a fresh session id, build a [`Session`] bound to `outbox`,
    /// and register it.
    ///
    /// If the configured [`Authenticator`] allows anonymous access (the
    /// default with [`AllowAllAuthenticator`]), the session is seeded with
    /// anonymous claims so transports may skip the connect handshake. Real
    /// authenticators leave the session in the unconnected state until
    /// [`Hub::handle_connect`] succeeds.
    pub fn open_session(&self, outbox: mpsc::Sender<ServerFrame>) -> Arc<Session> {
        let id = self.sessions.next_id();
        let session = Arc::new(Session::new(id, outbox));
        if self.authenticator.allow_anonymous() {
            session.set_claims(Arc::new(SessionClaims::anonymous()));
        }
        self.sessions.register(session.clone());
        metrics::counter!("cherenkov_sessions_opened_total").increment(1);
        metrics::gauge!("cherenkov_sessions_active").set(self.sessions.len() as f64);
        debug!(session_id = %id, "session opened");
        session
    }

    /// Whether this hub requires every session to send a successful
    /// `Connect` before issuing other frames.
    #[must_use]
    pub fn requires_connect(&self) -> bool {
        !self.authenticator.allow_anonymous()
    }

    /// Handle an inbound `Connect` frame.
    ///
    /// # Errors
    ///
    /// Returns [`HubError::SessionGone`] if `session_id` is not registered,
    /// [`HubError::AlreadyConnected`] if the session has already connected,
    /// or [`HubError::Auth`] if the authenticator rejects the token.
    #[instrument(level = "debug", skip(self, token), fields(session_id = session_id.0, request_id))]
    pub async fn handle_connect(
        &self,
        session_id: SessionId,
        request_id: u64,
        token: &str,
    ) -> Result<ProtocolConnectOk, HubError> {
        let session = self
            .sessions
            .get(&session_id)
            .ok_or(HubError::SessionGone {
                session_id: session_id.0,
            })?;
        // Re-authentication is not supported in v1 — the existing claims
        // may already gate in-flight forwarders.
        if session.claims().is_some() && !self.authenticator.allow_anonymous() {
            return Err(HubError::AlreadyConnected {
                session_id: session_id.0,
            });
        }
        let claims = self.authenticator.authenticate(token).await?;
        let subject = claims.subject.clone();
        let expires_at = claims.expires_at;
        session.set_claims(Arc::new(claims));
        debug!(session_id = %session_id, %subject, "session connected");
        Ok(ProtocolConnectOk {
            request_id,
            subject,
            expires_at,
        })
    }

    /// Resolve which [`ChannelKind`] handles `channel`, based on its
    /// namespace prefix.
    fn kind_for(&self, channel: &str) -> Arc<dyn ChannelKind> {
        let ns = namespace_of(channel);
        self.namespace_kinds
            .get(ns)
            .cloned()
            .unwrap_or_else(|| self.kind.clone())
    }

    fn require_claims(&self, session: &Session) -> Result<Arc<SessionClaims>, HubError> {
        if let Some(claims) = session.claims() {
            return Ok(claims);
        }
        if self.authenticator.allow_anonymous() {
            let anon = Arc::new(SessionClaims::anonymous());
            session.set_claims(anon.clone());
            return Ok(anon);
        }
        Err(HubError::NotConnected {
            session_id: session.id().0,
        })
    }

    async fn enforce_acl(
        &self,
        claims: &SessionClaims,
        action: AclAction,
        channel: &str,
    ) -> Result<(), HubError> {
        match self.acl.check(claims, action, channel).await? {
            AclDecision::Allow => Ok(()),
            AclDecision::Deny => Err(HubError::Acl(AclError::Denied {
                subject: claims.subject.clone(),
                action: action.as_str(),
                channel: channel.to_owned(),
                reason: "policy denied".to_owned(),
            })),
        }
    }

    /// Deregister a session, aborting any active subscription forwarders.
    pub fn close_session(&self, id: SessionId) {
        if self.sessions.deregister(&id).is_some() {
            metrics::counter!("cherenkov_sessions_closed_total").increment(1);
            metrics::gauge!("cherenkov_sessions_active").set(self.sessions.len() as f64);
            debug!(session_id = %id, "session closed");
        }
    }

    /// Handle an inbound `Subscribe` frame.
    ///
    /// When `since_offset` is non-zero, the channel kind is asked to
    /// replay retained publications with `offset > since_offset` before
    /// the live forwarder starts. Replayed publications are pushed
    /// through the same outbox the live forwarder will use, in order.
    ///
    /// # Errors
    ///
    /// Returns [`HubError::SessionGone`] if `session_id` is not registered,
    /// [`HubError::AlreadySubscribed`] if the session is already subscribed
    /// to `channel`, or any error surfaced by the channel kind or broker.
    #[instrument(level = "debug", skip(self), fields(session_id = session_id.0, request_id, channel, since_offset))]
    pub async fn handle_subscribe(
        &self,
        session_id: SessionId,
        request_id: u64,
        channel: &str,
        since_offset: u64,
    ) -> Result<ProtocolSubscribeOk, HubError> {
        let session = self
            .sessions
            .get(&session_id)
            .ok_or(HubError::SessionGone {
                session_id: session_id.0,
            })?;
        let claims = self.require_claims(&session)?;
        self.enforce_acl(&claims, AclAction::Subscribe, channel)
            .await?;
        if session.has_subscription(channel) {
            return Err(HubError::AlreadySubscribed {
                session_id: session_id.0,
                channel: channel.to_owned(),
            });
        }

        let kind = self.kind_for(channel);
        let cursor = kind.on_subscribe(channel).await?;
        // From this point on every early return must call
        // `kind.on_unsubscribe(channel)` to keep per-channel state in
        // the kind balanced. The closure below centralises that and
        // best-effort logs the unwind error so a bug in
        // `on_unsubscribe` does not mask the original failure.
        let unwind = |kind: Arc<dyn ChannelKind>, channel: String| async move {
            if let Err(err) = kind.on_unsubscribe(&channel).await {
                warn!(channel = %channel, %err, "on_unsubscribe failed during subscribe unwind");
            }
        };
        let mut stream = match self.broker.subscribe(channel).await {
            Ok(s) => s,
            Err(err) => {
                unwind(kind.clone(), channel.to_owned()).await;
                return Err(err.into());
            }
        };

        let outbox = session.outbox().clone();
        let channel_owned = channel.to_owned();

        if since_offset > 0 {
            let replayed = match kind.replay_since(channel, since_offset).await {
                Ok(v) => v,
                Err(err) => {
                    unwind(kind.clone(), channel.to_owned()).await;
                    return Err(err.into());
                }
            };
            for publication in replayed {
                if outbox
                    .send(ServerFrame::Publication(publication))
                    .await
                    .is_err()
                {
                    debug!(channel = %channel_owned, "outbox closed during replay; aborting");
                    unwind(kind.clone(), channel.to_owned()).await;
                    return Err(HubError::SessionGone {
                        session_id: session_id.0,
                    });
                }
            }
        }

        let forwarder = tokio::spawn(async move {
            while let Some(publication) = stream.next().await {
                let frame = ServerFrame::Publication(publication);
                if outbox.send(frame).await.is_err() {
                    debug!(channel = %channel_owned, "outbox closed; stopping forwarder");
                    break;
                }
            }
        });

        if let Err(stray) = session.insert_subscription(channel.to_owned(), forwarder) {
            stray.abort();
            unwind(kind.clone(), channel.to_owned()).await;
            return Err(HubError::AlreadySubscribed {
                session_id: session_id.0,
                channel: channel.to_owned(),
            });
        }
        self.sessions.add_subscription(session_id, channel);

        Ok(ProtocolSubscribeOk {
            request_id,
            channel: channel.to_owned(),
            epoch: cursor.epoch,
            offset: cursor.offset,
        })
    }

    /// Handle an inbound `Unsubscribe` frame.
    ///
    /// # Errors
    ///
    /// Returns [`HubError::SessionGone`] if `session_id` is not registered,
    /// [`HubError::NotSubscribed`] if the session was not subscribed to
    /// `channel`, or any error surfaced by the channel kind.
    #[instrument(level = "debug", skip(self), fields(session_id = session_id.0, request_id, channel))]
    pub async fn handle_unsubscribe(
        &self,
        session_id: SessionId,
        request_id: u64,
        channel: &str,
    ) -> Result<ProtocolUnsubscribeOk, HubError> {
        let session = self
            .sessions
            .get(&session_id)
            .ok_or(HubError::SessionGone {
                session_id: session_id.0,
            })?;
        let Some(forwarder) = session.remove_subscription(channel) else {
            return Err(HubError::NotSubscribed {
                session_id: session_id.0,
                channel: channel.to_owned(),
            });
        };
        forwarder.abort();
        self.sessions.remove_subscription(&session_id, channel);
        self.kind_for(channel).on_unsubscribe(channel).await?;

        Ok(ProtocolUnsubscribeOk {
            request_id,
            channel: channel.to_owned(),
        })
    }

    /// Handle an inbound `Publish` frame.
    ///
    /// The hub does not require the publishing session to be subscribed —
    /// publishers and subscribers are independent at the protocol level.
    ///
    /// The configured [`AclChecker`] is consulted first, then the
    /// [`SchemaValidator`] before the channel kind sees the payload;
    /// namespaces without a declared schema are pass-through.
    ///
    /// # Errors
    ///
    /// Surfaces ACL, schema-validator, channel-kind, and broker errors
    /// as-is.
    #[instrument(level = "debug", skip(self, data), fields(session_id = session_id.0, channel, bytes = data.len()))]
    pub async fn handle_publish(
        &self,
        session_id: SessionId,
        channel: &str,
        data: Bytes,
    ) -> Result<Publication, HubError> {
        let session = self
            .sessions
            .get(&session_id)
            .ok_or(HubError::SessionGone {
                session_id: session_id.0,
            })?;
        let claims = self.require_claims(&session)?;
        self.enforce_acl(&claims, AclAction::Publish, channel)
            .await?;
        self.validator.validate(channel, &data).await?;
        let publication = self.kind_for(channel).on_publish(channel, data).await?;
        if let Err(err) = self.broker.publish(channel, publication.clone()).await {
            warn!(channel = %channel, error = %err, "broker publish failed");
            return Err(err.into());
        }
        metrics::counter!("cherenkov_publications_total").increment(1);
        Ok(publication)
    }

    /// Force-close a session by id (admin / kick-by-id).
    ///
    /// Returns `true` if the session was registered and is now removed,
    /// `false` if it was unknown. Transports learn of the kick via
    /// [`Session::shutdown_notifier`] (which is fired before the session
    /// is removed from the registry) and tear down the underlying
    /// connection.
    pub fn kick_session(&self, id: SessionId) -> bool {
        let Some(session) = self.sessions.get(&id) else {
            return false;
        };
        session.signal_shutdown();
        let removed = self.sessions.deregister(&id).is_some();
        if removed {
            metrics::counter!("cherenkov_sessions_kicked_total").increment(1);
            metrics::gauge!("cherenkov_sessions_active").set(self.sessions.len() as f64);
            debug!(session_id = %id, "session kicked");
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use async_trait::async_trait;
    use bytes::Bytes;
    use cherenkov_protocol::Publication;
    use futures::stream::{self, StreamExt as _};
    use tokio::sync::mpsc;

    use super::*;
    use crate::{
        AuthError, Broker, BrokerError, BrokerStream, ChannelCursor, ChannelError, ChannelKind,
        SchemaError, SchemaValidator,
    };

    /// Channel kind that just returns a default cursor and synthesizes
    /// publications with a per-channel monotonic offset.
    struct CountingKind {
        offset: AtomicU64,
    }

    impl CountingKind {
        fn new() -> Self {
            Self {
                offset: AtomicU64::new(0),
            }
        }
    }

    #[async_trait]
    impl ChannelKind for CountingKind {
        fn name(&self) -> &'static str {
            "counting"
        }

        async fn on_subscribe(&self, _channel: &str) -> Result<ChannelCursor, ChannelError> {
            Ok(ChannelCursor::default())
        }

        async fn on_unsubscribe(&self, _channel: &str) -> Result<(), ChannelError> {
            Ok(())
        }

        async fn on_publish(
            &self,
            channel: &str,
            data: Bytes,
        ) -> Result<Publication, ChannelError> {
            let offset = self.offset.fetch_add(1, Ordering::Relaxed);
            Ok(Publication {
                channel: channel.to_owned(),
                offset,
                data,
            })
        }
    }

    /// Broker that records publishes and produces empty subscribe streams
    /// — enough to exercise hub bookkeeping without spinning up a real
    /// broadcast channel.
    struct RecordingBroker {
        published: parking_lot::Mutex<Vec<(String, Publication)>>,
    }

    impl RecordingBroker {
        fn new() -> Self {
            Self {
                published: parking_lot::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl Broker for RecordingBroker {
        fn name(&self) -> &'static str {
            "recording"
        }

        async fn subscribe(&self, _topic: &str) -> Result<BrokerStream, BrokerError> {
            Ok(stream::pending().boxed())
        }

        async fn publish(&self, topic: &str, publication: Publication) -> Result<(), BrokerError> {
            self.published.lock().push((topic.to_owned(), publication));
            Ok(())
        }
    }

    fn build_hub() -> (Hub, Arc<RecordingBroker>) {
        let broker = Arc::new(RecordingBroker::new());
        let kind = Arc::new(CountingKind::new());
        let built = HubBuilder::new()
            .with_channel_kind(kind)
            .with_broker(broker.clone())
            .build()
            .expect("hub builds with kind and broker");
        (built.hub, broker)
    }

    #[tokio::test]
    async fn subscribe_then_unsubscribe_round_trip() {
        let (hub, _) = build_hub();
        let (tx, _rx) = mpsc::channel(16);
        let session = hub.open_session(tx);

        let ok = hub
            .handle_subscribe(session.id(), 1, "rooms.lobby", 0)
            .await
            .expect("subscribe ok");
        assert_eq!(ok.channel, "rooms.lobby");
        assert_eq!(ok.request_id, 1);

        let ok = hub
            .handle_unsubscribe(session.id(), 2, "rooms.lobby")
            .await
            .expect("unsubscribe ok");
        assert_eq!(ok.request_id, 2);
    }

    #[tokio::test]
    async fn double_subscribe_is_rejected() {
        let (hub, _) = build_hub();
        let (tx, _rx) = mpsc::channel(16);
        let session = hub.open_session(tx);

        hub.handle_subscribe(session.id(), 1, "rooms.lobby", 0)
            .await
            .expect("first subscribe ok");
        let err = hub
            .handle_subscribe(session.id(), 2, "rooms.lobby", 0)
            .await
            .expect_err("second subscribe must fail");
        assert!(matches!(err, HubError::AlreadySubscribed { .. }));
    }

    #[tokio::test]
    async fn double_unsubscribe_is_rejected() {
        let (hub, _) = build_hub();
        let (tx, _rx) = mpsc::channel(16);
        let session = hub.open_session(tx);

        let err = hub
            .handle_unsubscribe(session.id(), 1, "rooms.lobby")
            .await
            .expect_err("unsubscribe without subscribe must fail");
        assert!(matches!(err, HubError::NotSubscribed { .. }));
    }

    #[tokio::test]
    async fn publish_routes_through_kind_and_broker() {
        let (hub, broker) = build_hub();
        let (tx, _rx) = mpsc::channel(16);
        let session = hub.open_session(tx);
        hub.handle_publish(session.id(), "rooms.lobby", Bytes::from_static(b"hi"))
            .await
            .expect("publish ok");
        let recorded = broker.published.lock().clone();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "rooms.lobby");
        assert_eq!(recorded[0].1.data, Bytes::from_static(b"hi"));
    }

    /// Validator that rejects every publication with a fixed reason — used
    /// to prove the hub short-circuits the channel kind and broker when
    /// validation fails.
    struct RejectAllValidator;

    #[async_trait]
    impl SchemaValidator for RejectAllValidator {
        fn name(&self) -> &'static str {
            "reject-all"
        }

        async fn validate(&self, channel: &str, _data: &Bytes) -> Result<(), SchemaError> {
            Err(SchemaError::PayloadRejected {
                channel: channel.to_owned(),
                reason: "always rejects".to_owned(),
            })
        }
    }

    #[tokio::test]
    async fn publish_blocked_by_validator_short_circuits_kind_and_broker() {
        let broker = Arc::new(RecordingBroker::new());
        let kind = Arc::new(CountingKind::new());
        let built = HubBuilder::new()
            .with_channel_kind(kind.clone())
            .with_broker(broker.clone())
            .with_schema_validator(Arc::new(RejectAllValidator))
            .build()
            .expect("hub builds with validator");
        let hub = built.hub;

        let (tx, _rx) = mpsc::channel(16);
        let session = hub.open_session(tx);
        let err = hub
            .handle_publish(session.id(), "rooms.lobby", Bytes::from_static(b"hi"))
            .await
            .expect_err("validator must short-circuit publish");
        assert!(matches!(
            err,
            HubError::Schema(SchemaError::PayloadRejected { .. })
        ));

        // The channel kind must not have been consulted, so its monotonic
        // offset is still zero.
        assert_eq!(kind.offset.load(Ordering::Relaxed), 0);
        assert!(broker.published.lock().is_empty());
    }

    /// Authenticator that requires a fixed token "good".
    struct FixedTokenAuth;

    #[async_trait]
    impl Authenticator for FixedTokenAuth {
        fn name(&self) -> &'static str {
            "fixed"
        }
        fn allow_anonymous(&self) -> bool {
            false
        }
        async fn authenticate(&self, token: &str) -> Result<SessionClaims, AuthError> {
            if token == "good" {
                Ok(SessionClaims {
                    subject: "alice".to_owned(),
                    permissions: vec!["publish".to_owned()],
                    expires_at: 0,
                })
            } else {
                Err(AuthError::InvalidToken {
                    reason: "expected `good`".to_owned(),
                })
            }
        }
    }

    /// Deny-by-channel ACL: deny everything for `forbidden.*`.
    struct PrefixAcl;

    #[async_trait]
    impl AclChecker for PrefixAcl {
        fn name(&self) -> &'static str {
            "prefix"
        }
        async fn check(
            &self,
            _claims: &SessionClaims,
            _action: AclAction,
            channel: &str,
        ) -> Result<AclDecision, AclError> {
            if channel.starts_with("forbidden.") {
                Ok(AclDecision::Deny)
            } else {
                Ok(AclDecision::Allow)
            }
        }
    }

    fn build_auth_hub() -> Hub {
        let broker = Arc::new(RecordingBroker::new());
        let kind = Arc::new(CountingKind::new());
        HubBuilder::new()
            .with_channel_kind(kind)
            .with_broker(broker)
            .with_authenticator(Arc::new(FixedTokenAuth))
            .with_acl_checker(Arc::new(PrefixAcl))
            .build()
            .expect("hub builds with auth + acl")
            .hub
    }

    #[tokio::test]
    async fn subscribe_before_connect_is_rejected() {
        let hub = build_auth_hub();
        let (tx, _rx) = mpsc::channel(16);
        let session = hub.open_session(tx);
        let err = hub
            .handle_subscribe(session.id(), 1, "rooms.lobby", 0)
            .await
            .expect_err("must require connect");
        assert!(matches!(err, HubError::NotConnected { .. }));
    }

    #[tokio::test]
    async fn connect_with_bad_token_returns_auth_error() {
        let hub = build_auth_hub();
        let (tx, _rx) = mpsc::channel(16);
        let session = hub.open_session(tx);
        let err = hub
            .handle_connect(session.id(), 1, "wrong")
            .await
            .expect_err("bad token must fail");
        assert!(matches!(
            err,
            HubError::Auth(AuthError::InvalidToken { .. })
        ));
    }

    #[tokio::test]
    async fn connect_then_subscribe_then_publish_round_trip() {
        let hub = build_auth_hub();
        let (tx, _rx) = mpsc::channel(16);
        let session = hub.open_session(tx);
        let ok = hub
            .handle_connect(session.id(), 1, "good")
            .await
            .expect("connect ok");
        assert_eq!(ok.subject, "alice");
        hub.handle_subscribe(session.id(), 2, "rooms.lobby", 0)
            .await
            .expect("subscribe ok");
        hub.handle_publish(session.id(), "rooms.lobby", Bytes::from_static(b"hi"))
            .await
            .expect("publish ok");
    }

    #[tokio::test]
    async fn acl_deny_short_circuits_subscribe_and_publish() {
        let hub = build_auth_hub();
        let (tx, _rx) = mpsc::channel(16);
        let session = hub.open_session(tx);
        hub.handle_connect(session.id(), 1, "good").await.unwrap();
        let err = hub
            .handle_subscribe(session.id(), 2, "forbidden.x", 0)
            .await
            .expect_err("must deny");
        assert!(matches!(err, HubError::Acl(_)));
        let err = hub
            .handle_publish(session.id(), "forbidden.x", Bytes::from_static(b"hi"))
            .await
            .expect_err("must deny publish too");
        assert!(matches!(err, HubError::Acl(_)));
    }

    #[tokio::test]
    async fn double_connect_is_rejected() {
        let hub = build_auth_hub();
        let (tx, _rx) = mpsc::channel(16);
        let session = hub.open_session(tx);
        hub.handle_connect(session.id(), 1, "good").await.unwrap();
        let err = hub
            .handle_connect(session.id(), 2, "good")
            .await
            .expect_err("re-auth must fail");
        assert!(matches!(err, HubError::AlreadyConnected { .. }));
    }
}
