//! Test-friendly server composition: build a hub, attach it to a
//! [`tokio::net::TcpListener`], and drive the WebSocket transport in the
//! background.
//!
//! Production startup uses this same code path through `main.rs`; tests
//! use [`run_with_listener`] directly so they can bind to port 0 and read
//! the assigned port back from the listener.

use std::net::SocketAddr;
use std::sync::Arc;

use cherenkov_auth::{AclMatch, AclRule, JwtAuthenticator, NamespaceAcl};
use cherenkov_broker::MemoryBroker;
use cherenkov_broker_nats::{NatsBroker, NatsBrokerConfig};
use cherenkov_broker_redis::{RedisBroker, RedisBrokerConfig};
use cherenkov_channel_crdt::{AutomergeChannel, YjsChannel};
use cherenkov_channel_pubsub::PubSubChannel;
use cherenkov_core::{
    AclChecker, AllowAllAcl, AllowAllAuthenticator, Authenticator, Broker, ChannelKind, Hub,
    HubBuilder,
};
use cherenkov_schema::{JsonSchemaRegistry, RegistryError};
use cherenkov_transport_sse::serve_on_listener as sse_serve_on_listener;
use cherenkov_transport_ws::serve_on_listener;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::config::{
    AclActionConfig, AclConfig, AclEffectConfig, AuthConfig, BrokerBackend, BrokerConfig,
    ChannelKindName, ChannelKindsConfig, NamespacesConfig, ServerConfig,
};

/// Errors surfaced by [`run_with_listener`].
#[derive(Debug, Error)]
pub enum ServerError {
    /// The hub builder rejected the supplied configuration.
    #[error("hub configuration error: {0}")]
    Hub(String),
    /// The transport failed to start.
    #[error("transport error: {0}")]
    Transport(String),
    /// The transport failed mid-flight.
    #[error("transport task panicked: {0}")]
    Task(String),
    /// A namespace's schema declaration was malformed.
    #[error("schema configuration error: {0}")]
    Schema(String),
    /// The auth section was malformed (e.g. empty HMAC secret).
    #[error("auth configuration error: {0}")]
    Auth(String),
    /// An ACL rule was malformed (e.g. invalid glob).
    #[error("acl configuration error: {0}")]
    Acl(String),
    /// The broker section was malformed (e.g. `redis` backend without
    /// `redis_url`).
    #[error("broker configuration error: {0}")]
    Broker(String),
}

/// Handle to a running server: the bound address plus the join handle of
/// the transport task.
pub struct ServerHandle {
    /// The address the listener is bound to.
    pub local_addr: SocketAddr,
    /// The transport's join handle. Dropping it does not stop the server;
    /// call [`ServerHandle::shutdown`] to abort cleanly.
    pub join: JoinHandle<Result<(), ServerError>>,
}

impl ServerHandle {
    /// Abort the transport task. The server stops accepting new
    /// connections; in-flight sessions are dropped.
    pub fn shutdown(self) {
        self.join.abort();
    }
}

/// Build a hub, bind the WebSocket transport to `listener`, and spawn the
/// accept loop on the current Tokio runtime.
///
/// This is the test-friendly entry point: it ignores `config.transport.sse`
/// and `config.admin` so callers can drive a deterministic single-listener
/// setup. For full multi-transport startup (production binary), use
/// [`run`] instead.
///
/// # Errors
///
/// Returns [`ServerError::Hub`] if the hub builder cannot construct a hub
/// from the supplied configuration.
pub async fn run_with_listener(
    config: ServerConfig,
    listener: TcpListener,
) -> Result<ServerHandle, ServerError> {
    let local_addr = listener
        .local_addr()
        .map_err(|e| ServerError::Transport(e.to_string()))?;

    let hub = build_hub(&config).await?;

    let path = config.transport.ws.path.clone();
    let outbox_capacity = config.transport.ws.outbox_capacity;

    let join = tokio::spawn(async move {
        serve_on_listener(listener, path, hub, outbox_capacity)
            .await
            .map_err(|e| ServerError::Transport(e.to_string()))
    });

    Ok(ServerHandle { local_addr, join })
}

/// Construct a fully-configured [`Hub`] from `config`.
///
/// Used by the multi-transport startup path; tests prefer
/// [`run_with_listener`] which calls this internally.
///
/// # Errors
///
/// Surfaces every kind of configuration error ([`ServerError::Schema`],
/// [`ServerError::Auth`], [`ServerError::Acl`], [`ServerError::Broker`])
/// plus a final [`ServerError::Hub`] from `HubBuilder::build`.
pub async fn build_hub(config: &ServerConfig) -> Result<Hub, ServerError> {
    let kind = Arc::new(PubSubChannel::with_bounds(
        config.channel_pubsub.history_size,
        config.channel_pubsub.history_ttl(),
    ));
    let broker = build_broker(&config.broker).await?;
    let validator = Arc::new(build_schema_registry(&config.namespaces)?);
    let authenticator: Arc<dyn Authenticator> = build_authenticator(config.auth.as_ref())?;
    let acl: Arc<dyn AclChecker> = build_acl(config.acl.as_ref())?;

    let mut builder = HubBuilder::new()
        .with_channel_kind(kind)
        .with_broker(broker)
        .with_schema_validator(validator)
        .with_authenticator(authenticator)
        .with_acl_checker(acl);
    for (namespace, kind) in build_namespace_kinds(&config.channel_kinds) {
        builder = builder.with_channel_kind_for(namespace, kind);
    }
    let built = builder
        .build()
        .map_err(|e| ServerError::Hub(e.to_owned()))?;
    Ok(built.hub)
}

/// Install the Prometheus recorder if it has not been installed yet.
///
/// `metrics::set_global_recorder` only succeeds once per process; calling
/// it twice (e.g. inside a test that also boots the binary path) returns
/// an error. We swallow that case and return the existing handle by
/// re-building it locally — `PrometheusBuilder::build_recorder` always
/// produces a fresh handle attached to its own recorder.
fn install_prometheus_recorder() -> Option<PrometheusHandle> {
    let recorder = PrometheusBuilder::new().build_recorder();
    let handle = recorder.handle();
    match metrics::set_global_recorder(recorder) {
        Ok(()) => Some(handle),
        Err(_) => {
            // Another recorder is already installed (typical inside the
            // test binary). Surface a fresh handle so the route still
            // returns something coherent; it just won't be tied to the
            // global registry.
            Some(PrometheusBuilder::new().build_recorder().handle())
        }
    }
}

/// Materialize the configured broker.
async fn build_broker(cfg: &BrokerConfig) -> Result<Arc<dyn Broker>, ServerError> {
    Ok(match cfg.backend {
        BrokerBackend::Memory => Arc::new(MemoryBroker::with_capacity(cfg.capacity)),
        BrokerBackend::Redis => {
            let url = cfg.redis_url.clone().ok_or_else(|| {
                ServerError::Broker("broker.backend = redis requires broker.redis_url".to_owned())
            })?;
            Arc::new(
                RedisBroker::connect(RedisBrokerConfig::new(url))
                    .await
                    .map_err(|e| ServerError::Broker(e.to_string()))?,
            )
        }
        BrokerBackend::Nats => {
            let url = cfg.nats_url.clone().ok_or_else(|| {
                ServerError::Broker("broker.backend = nats requires broker.nats_url".to_owned())
            })?;
            Arc::new(
                NatsBroker::connect(NatsBrokerConfig::new(url))
                    .await
                    .map_err(|e| ServerError::Broker(e.to_string()))?,
            )
        }
    })
}

/// Run the full multi-transport server: WebSocket on `config.transport.ws`,
/// optional SSE on `config.transport.sse`, optional admin on `config.admin`.
///
/// Returns a [`ServerRunHandle`] that owns the join handles for every
/// spawned task. Dropping it does not stop the server; call
/// [`ServerRunHandle::shutdown`].
///
/// # Errors
///
/// Surfaces every error from [`build_hub`] plus per-transport bind
/// failures.
pub async fn run(config: ServerConfig) -> Result<ServerRunHandle, ServerError> {
    let hub = build_hub(&config).await?;

    let ws_listener = TcpListener::bind(config.transport.ws.listen)
        .await
        .map_err(|e| ServerError::Transport(format!("ws bind: {e}")))?;
    let ws_local = ws_listener
        .local_addr()
        .map_err(|e| ServerError::Transport(e.to_string()))?;

    let mut joins = Vec::new();

    let ws_path = config.transport.ws.path.clone();
    let ws_capacity = config.transport.ws.outbox_capacity;
    let ws_hub = hub.clone();
    joins.push(tokio::spawn(async move {
        serve_on_listener(ws_listener, ws_path, ws_hub, ws_capacity)
            .await
            .map_err(|e| ServerError::Transport(format!("ws: {e}")))
    }));

    let sse_local = if let Some(sse_cfg) = config.transport.sse.clone() {
        let listener = TcpListener::bind(sse_cfg.listen)
            .await
            .map_err(|e| ServerError::Transport(format!("sse bind: {e}")))?;
        let local = listener
            .local_addr()
            .map_err(|e| ServerError::Transport(e.to_string()))?;
        let sse_hub = hub.clone();
        let prefix = sse_cfg.path_prefix.clone();
        joins.push(tokio::spawn(async move {
            sse_serve_on_listener(listener, prefix, sse_hub)
                .await
                .map_err(|e| ServerError::Transport(format!("sse: {e}")))
        }));
        Some(local)
    } else {
        None
    };

    let admin_local = if config.admin.enabled {
        let listener = TcpListener::bind(config.admin.listen)
            .await
            .map_err(|e| ServerError::Transport(format!("admin bind: {e}")))?;
        let local = listener
            .local_addr()
            .map_err(|e| ServerError::Transport(e.to_string()))?;
        let sessions = hub.sessions().clone();
        let metrics_handle = install_prometheus_recorder();
        let mut resources = cherenkov_admin::AdminResources::new(sessions).with_hub(hub.clone());
        if let Some(handle) = metrics_handle {
            resources = resources.with_metrics(handle);
        }
        if let Some(token) = config.admin.auth_token.clone() {
            resources = resources.with_auth_token(token);
        }
        joins.push(tokio::spawn(async move {
            cherenkov_admin::serve_on_listener(listener, resources)
                .await
                .map_err(|e| ServerError::Transport(format!("admin: {e}")))
        }));
        Some(local)
    } else {
        None
    };

    Ok(ServerRunHandle {
        ws_addr: ws_local,
        sse_addr: sse_local,
        admin_addr: admin_local,
        joins,
    })
}

/// Handle to a fully-running server (every transport + admin).
pub struct ServerRunHandle {
    /// WebSocket listen address.
    pub ws_addr: SocketAddr,
    /// SSE listen address, if SSE was enabled.
    pub sse_addr: Option<SocketAddr>,
    /// Admin listen address, if admin was enabled.
    pub admin_addr: Option<SocketAddr>,
    joins: Vec<JoinHandle<Result<(), ServerError>>>,
}

impl ServerRunHandle {
    /// Abort every spawned task. Returns once each `JoinHandle::abort`
    /// has been called; tasks may take a moment to actually wind down.
    pub fn shutdown(self) {
        for j in self.joins {
            j.abort();
        }
    }

    /// Wait for any spawned task to terminate. Returns the first
    /// non-`Ok` outcome (whether due to an error or panic) so the
    /// caller can tear down on the first transport failure.
    ///
    /// # Errors
    ///
    /// Bubbles up [`ServerError::Task`] for panics and
    /// [`ServerError::Transport`] from individual transport tasks.
    pub async fn wait(mut self) -> Result<(), ServerError> {
        if self.joins.is_empty() {
            return Ok(());
        }
        let (result, idx, rest) = futures::future::select_all(self.joins).await;
        self.joins = rest;
        for j in self.joins {
            j.abort();
        }
        let _ = idx;
        match result {
            Ok(inner) => inner,
            Err(e) => Err(ServerError::Task(e.to_string())),
        }
    }
}

/// Compile every declared namespace schema into a [`JsonSchemaRegistry`].
///
/// Returns [`ServerError::Schema`] if any namespace's declaration is
/// malformed (no schema source, both schema and schema_path set, file
/// missing, invalid JSON Schema document).
fn build_schema_registry(namespaces: &NamespacesConfig) -> Result<JsonSchemaRegistry, ServerError> {
    let mut builder = JsonSchemaRegistry::builder();
    for (namespace, ns_cfg) in namespaces.as_map() {
        ns_cfg
            .validate_schema_source()
            .map_err(|reason| ServerError::Schema(format!("namespace `{namespace}`: {reason}")))?;

        let schema_value = match (&ns_cfg.schema, &ns_cfg.schema_path) {
            (Some(value), None) => value.clone(),
            (None, Some(path)) => {
                let bytes = std::fs::read(path).map_err(|e| {
                    ServerError::Schema(format!(
                        "namespace `{namespace}`: cannot read schema file {}: {e}",
                        path.display()
                    ))
                })?;
                serde_json::from_slice(&bytes).map_err(|e| {
                    ServerError::Schema(format!(
                        "namespace `{namespace}`: schema file {} is not valid JSON: {e}",
                        path.display()
                    ))
                })?
            }
            // Other combinations are unreachable: validate_schema_source
            // above already rejected the (None, None) and (Some, Some)
            // shapes with a typed error.
            _ => unreachable!("validate_schema_source rejected invalid combinations"),
        };

        builder = builder
            .with_namespace(namespace.clone(), schema_value)
            .map_err(|err: RegistryError| ServerError::Schema(err.to_string()))?;
    }
    Ok(builder.build())
}

/// Build the configured [`Authenticator`], or [`AllowAllAuthenticator`]
/// if no `auth:` section is present.
fn build_authenticator(auth: Option<&AuthConfig>) -> Result<Arc<dyn Authenticator>, ServerError> {
    let Some(cfg) = auth else {
        return Ok(Arc::new(AllowAllAuthenticator));
    };
    if cfg.hmac_secret.is_empty() {
        return Err(ServerError::Auth(
            "auth.hmac_secret must not be empty".to_owned(),
        ));
    }
    let mut builder = JwtAuthenticator::builder().with_hmac_secret(cfg.hmac_secret.as_bytes());
    for aud in &cfg.audiences {
        builder = builder.with_audience(aud);
    }
    if let Some(iss) = &cfg.issuer {
        builder = builder.with_issuer(iss);
    }
    Ok(Arc::new(builder.build()))
}

/// Materialize the per-namespace channel kinds declared in config.
fn build_namespace_kinds(cfg: &ChannelKindsConfig) -> Vec<(String, Arc<dyn ChannelKind>)> {
    let mut out = Vec::with_capacity(cfg.as_map().len());
    for (namespace, name) in cfg.as_map() {
        let kind: Arc<dyn ChannelKind> = match name {
            ChannelKindName::Pubsub => Arc::new(PubSubChannel::new()),
            ChannelKindName::CrdtYjs => Arc::new(YjsChannel::new()),
            ChannelKindName::CrdtAutomerge => Arc::new(AutomergeChannel::new()),
        };
        out.push((namespace.clone(), kind));
    }
    out
}

/// Build the configured [`AclChecker`], or [`AllowAllAcl`] if no `acl:`
/// section is present.
fn build_acl(acl: Option<&AclConfig>) -> Result<Arc<dyn AclChecker>, ServerError> {
    let Some(cfg) = acl else {
        return Ok(Arc::new(AllowAllAcl));
    };
    let mut builder = NamespaceAcl::builder().default_allow(cfg.default_allow);
    for rule in &cfg.rules {
        let action = match rule.action {
            AclActionConfig::Subscribe => AclMatch::Subscribe,
            AclActionConfig::Publish => AclMatch::Publish,
            AclActionConfig::Any => AclMatch::Any,
        };
        let mut compiled = match rule.effect {
            AclEffectConfig::Allow => AclRule::try_allow(&rule.channel, action),
            AclEffectConfig::Deny => AclRule::try_deny(&rule.channel, action),
        }
        .map_err(|e| ServerError::Acl(format!("invalid channel glob `{}`: {e}", rule.channel)))?;
        if let Some(subject) = &rule.subject {
            compiled = compiled
                .with_subject(subject)
                .map_err(|e| ServerError::Acl(format!("invalid subject glob `{subject}`: {e}")))?;
        }
        builder = builder.with_rule(compiled);
    }
    Ok(Arc::new(builder.build()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AdminConfig, ServerConfig};

    #[tokio::test]
    async fn redis_backend_without_url_is_rejected() {
        let config = ServerConfig {
            broker: BrokerConfig {
                backend: BrokerBackend::Redis,
                ..BrokerConfig::default()
            },
            ..ServerConfig::default()
        };
        let err = match build_hub(&config).await {
            Ok(_) => panic!("must reject"),
            Err(e) => e,
        };
        assert!(matches!(err, ServerError::Broker(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn nats_backend_without_url_is_rejected() {
        let config = ServerConfig {
            broker: BrokerConfig {
                backend: BrokerBackend::Nats,
                ..BrokerConfig::default()
            },
            ..ServerConfig::default()
        };
        let err = match build_hub(&config).await {
            Ok(_) => panic!("must reject"),
            Err(e) => e,
        };
        assert!(matches!(err, ServerError::Broker(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn empty_hmac_secret_is_rejected() {
        let config = ServerConfig {
            auth: Some(AuthConfig::default()),
            ..ServerConfig::default()
        };
        let err = match build_hub(&config).await {
            Ok(_) => panic!("must reject"),
            Err(e) => e,
        };
        assert!(matches!(err, ServerError::Auth(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn invalid_acl_glob_is_rejected() {
        use crate::config::{AclConfig, AclEffectConfig, AclRuleConfig};
        let config = ServerConfig {
            acl: Some(AclConfig {
                rules: vec![AclRuleConfig {
                    effect: AclEffectConfig::Allow,
                    channel: "[".to_owned(),
                    subject: None,
                    action: AclActionConfig::Any,
                }],
                default_allow: false,
            }),
            ..ServerConfig::default()
        };
        let err = match build_hub(&config).await {
            Ok(_) => panic!("must reject"),
            Err(e) => e,
        };
        assert!(matches!(err, ServerError::Acl(_)), "got {err:?}");
    }

    #[test]
    fn admin_config_default_is_disabled_with_no_token() {
        let admin = AdminConfig::default();
        assert!(!admin.enabled);
        assert!(admin.auth_token.is_none());
    }
}
