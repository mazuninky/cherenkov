//! Library facade for the Cherenkov server binary.
//!
//! Most logic — config loading, hub composition — lives in submodules so
//! integration tests can drive a real server without re-implementing
//! startup glue.

pub mod app;
pub mod config;

pub use app::{build_hub, run, run_with_listener, ServerError, ServerHandle, ServerRunHandle};
pub use config::{
    AclActionConfig, AclConfig, AclEffectConfig, AclRuleConfig, AdminConfig, AuthConfig,
    BrokerBackend, BrokerConfig, ChannelKindName, ChannelKindsConfig, NamespaceConfig,
    NamespacesConfig, SchemaKind, ServerConfig, SseConfig, TransportConfig, WsConfig,
};
