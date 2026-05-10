//! Server configuration schema and loader.
//!
//! Loaded from a YAML file with `CHERENKOV_*` environment overrides via
//! [`figment`]. Defaults are honest: localhost binding, in-memory broker,
//! pretty-printed logs.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use figment::providers::{Env, Format as _, Yaml};
use figment::Figment;
use serde::Deserialize;
use serde_json::Value as JsonValue;

/// Top-level server configuration.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    /// Transport configuration (currently WebSocket only).
    pub transport: TransportConfig,
    /// Broker configuration.
    pub broker: BrokerConfig,
    /// Pub/sub channel kind configuration.
    pub channel_pubsub: ChannelPubSubConfig,
    /// Per-namespace JSON Schema declarations.
    ///
    /// Namespaces with a declared schema have every publication validated
    /// against it before the channel kind sees the data. Namespaces with no
    /// entry are opaque pass-through.
    pub namespaces: NamespacesConfig,
    /// Optional JWT authentication. When unset, every session is treated
    /// as anonymous (`AllowAllAuthenticator`).
    pub auth: Option<AuthConfig>,
    /// Optional ACL rule list. When unset, every action is permitted
    /// (`AllowAllAcl`).
    pub acl: Option<AclConfig>,
    /// Per-namespace channel-kind overrides.
    ///
    /// Channels in unlisted namespaces use the default pub/sub kind.
    /// Channels in `crdt-yjs` / `crdt-automerge` namespaces are routed
    /// through the corresponding CRDT channel kind.
    pub channel_kinds: ChannelKindsConfig,
    /// Admin HTTP API + UI settings.
    pub admin: AdminConfig,
    /// Logging configuration.
    pub log: LogConfig,
}

impl ServerConfig {
    /// Load the config from `path`, layering `CHERENKOV_*` env overrides
    /// on top.
    #[allow(clippy::result_large_err)]
    pub fn load(path: &Path) -> Result<Self, figment::Error> {
        Figment::new()
            .merge(Yaml::file(path))
            .merge(Env::prefixed("CHERENKOV_").split("__"))
            .extract()
    }
}

/// Transport configuration root.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TransportConfig {
    /// WebSocket transport.
    pub ws: WsConfig,
    /// Server-Sent Events transport (optional; disabled when absent).
    pub sse: Option<SseConfig>,
}

/// SSE transport settings.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SseConfig {
    /// Address and port to bind. Defaults to `127.0.0.1:7100`.
    pub listen: SocketAddr,
    /// HTTP path prefix the SSE router is mounted under.
    pub path_prefix: String,
}

impl Default for SseConfig {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:7100".parse().expect("static literal parses"),
            path_prefix: "/sse/v1".to_owned(),
        }
    }
}

/// WebSocket transport settings.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WsConfig {
    /// Address and port to bind. Defaults to `127.0.0.1:7000`.
    pub listen: SocketAddr,
    /// HTTP path the WebSocket is mounted at.
    pub path: String,
    /// Per-session outbound queue depth.
    pub outbox_capacity: usize,
}

impl Default for WsConfig {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:7000".parse().expect("static literal parses"),
            path: "/connect/v1".to_owned(),
            outbox_capacity: 1024,
        }
    }
}

/// Broker settings.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BrokerConfig {
    /// Backend selector.
    pub backend: BrokerBackend,
    /// Per-topic broadcast capacity, in publications. Used by the
    /// in-memory backend; ignored by Redis and NATS.
    pub capacity: usize,
    /// `redis://` URL. Required when `backend = "redis"`.
    pub redis_url: Option<String>,
    /// `nats://` URL. Required when `backend = "nats"`.
    pub nats_url: Option<String>,
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            backend: BrokerBackend::default(),
            capacity: 1024,
            redis_url: None,
            nats_url: None,
        }
    }
}

/// Broker backend selector.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BrokerBackend {
    /// In-process [`cherenkov_broker::MemoryBroker`].
    #[default]
    Memory,
    /// Redis Pub/Sub via [`cherenkov_broker_redis::RedisBroker`].
    Redis,
    /// NATS via [`cherenkov_broker_nats::NatsBroker`].
    Nats,
}

/// Admin HTTP API + UI settings.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AdminConfig {
    /// Address and port to bind. Defaults to `127.0.0.1:7200`.
    pub listen: SocketAddr,
    /// Whether to mount the admin endpoints. Default: `false`.
    pub enabled: bool,
    /// Optional bearer token. When set, every admin JSON request must
    /// carry `Authorization: Bearer <token>`.
    pub auth_token: Option<String>,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:7200".parse().expect("static literal parses"),
            enabled: false,
            auth_token: None,
        }
    }
}

/// Pub/sub channel kind settings.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChannelPubSubConfig {
    /// Maximum number of history entries retained per channel.
    pub history_size: usize,
    /// History entry TTL, in seconds.
    pub history_ttl_seconds: u64,
}

impl Default for ChannelPubSubConfig {
    fn default() -> Self {
        Self {
            history_size: 256,
            history_ttl_seconds: 300,
        }
    }
}

impl ChannelPubSubConfig {
    /// History entry TTL as a [`Duration`].
    #[must_use]
    pub fn history_ttl(&self) -> Duration {
        Duration::from_secs(self.history_ttl_seconds)
    }
}

/// Per-namespace schema declarations.
///
/// The map key is the namespace (the part before the first `.` in a
/// channel name); the value describes how to load the schema. The
/// declaration may either inline the schema as JSON under `schema` or
/// reference a file via `schema_path`. Exactly one form must be provided
/// per namespace.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, transparent)]
pub struct NamespacesConfig(pub BTreeMap<String, NamespaceConfig>);

impl NamespacesConfig {
    /// Borrow the underlying map.
    #[must_use]
    pub fn as_map(&self) -> &BTreeMap<String, NamespaceConfig> {
        &self.0
    }

    /// Whether any namespace declares a schema.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Schema declaration for a single namespace.
///
/// Exactly one of [`Self::schema`] or [`Self::schema_path`] must be set.
/// `kind` defaults to JSON Schema; future variants (Protobuf descriptor
/// sets, Avro) will surface as additional enum members.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NamespaceConfig {
    /// Schema language. JSON Schema is currently the only supported value.
    pub kind: SchemaKind,
    /// Inline schema document (mutually exclusive with `schema_path`).
    pub schema: Option<JsonValue>,
    /// Path to a schema file on disk (mutually exclusive with `schema`).
    pub schema_path: Option<PathBuf>,
}

impl NamespaceConfig {
    /// Verify that exactly one of [`Self::schema`] / [`Self::schema_path`]
    /// is set.
    ///
    /// This is the single source of truth for the "exactly one schema
    /// source per namespace" invariant; `app::run_with_listener` calls it
    /// during composition so misconfiguration fails the boot rather than
    /// the first publish, and tests / programmatic config builders can
    /// exercise the same check without re-implementing it.
    ///
    /// # Errors
    ///
    /// Returns a human-readable description of the violation when neither
    /// or both fields are set.
    pub fn validate_schema_source(&self) -> Result<(), &'static str> {
        match (&self.schema, &self.schema_path) {
            (None, None) => Err("one of `schema` or `schema_path` must be set"),
            (Some(_), Some(_)) => Err("`schema` and `schema_path` are mutually exclusive"),
            _ => Ok(()),
        }
    }
}

/// Per-namespace channel-kind routing.
///
/// The map key is the namespace prefix (everything before the first `.`
/// in a channel name); the value names the channel kind to use.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, transparent)]
pub struct ChannelKindsConfig(pub BTreeMap<String, ChannelKindName>);

impl ChannelKindsConfig {
    /// Borrow the underlying map.
    #[must_use]
    pub fn as_map(&self) -> &BTreeMap<String, ChannelKindName> {
        &self.0
    }
}

/// Channel-kind selector: either the default pub/sub kind or one of the
/// CRDT engines provided by `cherenkov-channel-crdt`.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ChannelKindName {
    /// Plain pub/sub with bounded history (the default).
    #[default]
    Pubsub,
    /// Y.js document via the `yrs` crate.
    CrdtYjs,
    /// Automerge document.
    CrdtAutomerge,
}

/// JWT authentication configuration.
///
/// HMAC-SHA secrets only for now; asymmetric keys are reserved for a
/// follow-up change.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AuthConfig {
    /// HMAC secret used to verify token signatures. **Must** be set when
    /// `auth` is configured; the loader rejects an empty string.
    pub hmac_secret: String,
    /// Accepted `aud` values. Empty means "audience not validated".
    pub audiences: Vec<String>,
    /// Required `iss` value. `None` means "issuer not validated".
    pub issuer: Option<String>,
}

/// ACL configuration: an ordered list of rules.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AclConfig {
    /// Rules evaluated in declaration order; the first match wins.
    pub rules: Vec<AclRuleConfig>,
    /// Decision when no rule matches. Defaults to `false` (deny).
    #[serde(default)]
    pub default_allow: bool,
}

/// One ACL rule.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AclRuleConfig {
    /// Rule effect (`"allow"` or `"deny"`).
    pub effect: AclEffectConfig,
    /// Channel name glob this rule applies to.
    pub channel: String,
    /// Optional subject glob (defaults to "all subjects").
    #[serde(default)]
    pub subject: Option<String>,
    /// Action(s) covered (`"subscribe"`, `"publish"`, `"any"`).
    #[serde(default)]
    pub action: AclActionConfig,
}

/// `allow` / `deny`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AclEffectConfig {
    /// Permit the matching action.
    Allow,
    /// Forbid the matching action.
    Deny,
}

/// Action(s) the rule covers.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AclActionConfig {
    /// `Subscribe` only.
    Subscribe,
    /// `Publish` only.
    Publish,
    /// Both.
    #[default]
    Any,
}

/// Schema language supported by a namespace declaration.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SchemaKind {
    /// JSON Schema, draft auto-detected by the validator.
    #[default]
    JsonSchema,
}

/// Logging configuration.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LogConfig {
    /// `tracing-subscriber` env-filter directive.
    pub level: String,
    /// Output format.
    pub format: LogFormat,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_owned(),
            format: LogFormat::Pretty,
        }
    }
}

/// Log output format.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// JSON-formatted ECS-style log lines.
    Json,
    /// Human-readable, single-line format.
    #[default]
    Pretty,
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    #[test]
    fn default_config_compiles() {
        let cfg = ServerConfig::default();
        assert_eq!(cfg.transport.ws.path, "/connect/v1");
        assert_eq!(cfg.broker.capacity, 1024);
        assert_eq!(cfg.broker.backend, BrokerBackend::Memory);
        assert_eq!(cfg.channel_pubsub.history_size, 256);
        assert!(cfg.namespaces.is_empty());
        assert!(cfg.transport.sse.is_none());
        assert!(!cfg.admin.enabled);
        assert!(cfg.admin.auth_token.is_none());
    }

    #[test]
    fn broker_backend_round_trips() {
        let body = r#"
broker:
  backend: redis
  redis_url: "redis://localhost:6379"
"#;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(body.as_bytes()).unwrap();
        let cfg = ServerConfig::load(tmp.path()).unwrap();
        assert_eq!(cfg.broker.backend, BrokerBackend::Redis);
        assert_eq!(
            cfg.broker.redis_url.as_deref(),
            Some("redis://localhost:6379")
        );
    }

    #[test]
    fn admin_auth_token_parses() {
        let body = r#"
admin:
  enabled: true
  auth_token: "s3cr3t"
"#;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(body.as_bytes()).unwrap();
        let cfg = ServerConfig::load(tmp.path()).unwrap();
        assert!(cfg.admin.enabled);
        assert_eq!(cfg.admin.auth_token.as_deref(), Some("s3cr3t"));
    }

    #[test]
    fn load_from_yaml_file() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        let body = r#"
transport:
  ws:
    listen: "127.0.0.1:0"
    path: "/connect/v1"
    outbox_capacity: 32
broker:
  capacity: 16
channel_pubsub:
  history_size: 8
  history_ttl_seconds: 60
log:
  level: "info"
  format: "json"
"#;
        tmp.write_all(body.as_bytes()).expect("write");
        let cfg = ServerConfig::load(tmp.path()).expect("load");
        assert_eq!(cfg.broker.capacity, 16);
        assert_eq!(cfg.channel_pubsub.history_size, 8);
        assert_eq!(cfg.log.format, LogFormat::Json);
    }

    #[test]
    fn namespace_validate_schema_source_round_trip() {
        let mut cfg = NamespaceConfig::default();
        assert_eq!(
            cfg.validate_schema_source(),
            Err("one of `schema` or `schema_path` must be set")
        );

        cfg.schema = Some(serde_json::json!({"type": "string"}));
        cfg.validate_schema_source()
            .expect("inline schema is valid");

        cfg.schema_path = Some(std::path::PathBuf::from("/dev/null"));
        assert_eq!(
            cfg.validate_schema_source(),
            Err("`schema` and `schema_path` are mutually exclusive")
        );

        cfg.schema = None;
        cfg.validate_schema_source()
            .expect("schema_path alone is valid");
    }

    #[test]
    fn namespace_section_parses_inline_schema() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        let body = r#"
namespaces:
  orders:
    kind: "json-schema"
    schema:
      type: "object"
      required: ["sku"]
      properties:
        sku:
          type: "string"
"#;
        tmp.write_all(body.as_bytes()).expect("write");
        let cfg = ServerConfig::load(tmp.path()).expect("load");
        let orders = cfg
            .namespaces
            .as_map()
            .get("orders")
            .expect("orders namespace present");
        assert_eq!(orders.kind, SchemaKind::JsonSchema);
        assert!(orders.schema.is_some());
        assert!(orders.schema_path.is_none());
    }
}
