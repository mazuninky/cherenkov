//! Automerge channel kind backed by the [`automerge`] crate.
//!
//! Mirrors [`crate::yjs::YjsChannel`] in shape: each channel owns one
//! [`automerge::AutoCommit`]; publish bytes are an Automerge change set,
//! validated by `load_incremental` and re-broadcast as the same opaque
//! payload.
//!
//! Persistence is in-memory; see the crate-level docs for the trade-off.

use std::sync::atomic::{AtomicU64, Ordering};

use ::automerge::AutoCommit;
use async_trait::async_trait;
use bytes::Bytes;
use cherenkov_core::{ChannelCursor, ChannelError, ChannelKind};
use cherenkov_protocol::Publication;
use dashmap::DashMap;
use parking_lot::Mutex;

use crate::CrdtError;

/// In-memory Automerge channel kind.
#[derive(Default)]
pub struct AutomergeChannel {
    docs: DashMap<String, Mutex<DocSlot>>,
}

struct DocSlot {
    doc: AutoCommit,
    next_offset: AtomicU64,
}

impl AutomergeChannel {
    /// Construct an empty channel kind.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Encode the current state of `channel` as an Automerge save bundle.
    #[must_use]
    pub fn snapshot(&self, channel: &str) -> Option<Vec<u8>> {
        let entry = self.docs.get(channel)?;
        let mut slot = entry.lock();
        Some(slot.doc.save())
    }
}

#[async_trait]
impl ChannelKind for AutomergeChannel {
    fn name(&self) -> &'static str {
        "crdt-automerge"
    }

    async fn on_subscribe(&self, channel: &str) -> Result<ChannelCursor, ChannelError> {
        if channel.is_empty() {
            return Err(ChannelError::InvalidChannel {
                channel: channel.to_owned(),
            });
        }
        let entry = self.docs.entry(channel.to_owned()).or_insert_with(|| {
            Mutex::new(DocSlot {
                doc: AutoCommit::new(),
                next_offset: AtomicU64::new(0),
            })
        });
        let offset = entry.lock().next_offset.load(Ordering::Relaxed);
        Ok(ChannelCursor {
            epoch: 1,
            offset: offset.saturating_sub(1),
        })
    }

    async fn on_unsubscribe(&self, _channel: &str) -> Result<(), ChannelError> {
        Ok(())
    }

    async fn on_publish(&self, channel: &str, data: Bytes) -> Result<Publication, ChannelError> {
        if channel.is_empty() {
            return Err(ChannelError::InvalidChannel {
                channel: channel.to_owned(),
            });
        }
        let entry = self.docs.entry(channel.to_owned()).or_insert_with(|| {
            Mutex::new(DocSlot {
                doc: AutoCommit::new(),
                next_offset: AtomicU64::new(0),
            })
        });
        let mut slot = entry.lock();
        slot.doc
            .load_incremental(&data)
            .map_err(|e| ChannelError::RejectedPublication {
                reason: CrdtError::InvalidUpdate {
                    engine: "automerge",
                    channel: channel.to_owned(),
                    reason: e.to_string(),
                }
                .to_string(),
            })?;
        let offset = slot.next_offset.fetch_add(1, Ordering::Relaxed);
        Ok(Publication {
            channel: channel.to_owned(),
            offset,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use ::automerge::transaction::Transactable;
    use ::automerge::{AutoCommit, ObjType, ReadDoc};

    use super::*;

    fn automerge_change_setting_greeting() -> Bytes {
        let mut doc = AutoCommit::new();
        let _ = doc
            .put_object(::automerge::ROOT, "doc", ObjType::Map)
            .unwrap();
        doc.put(::automerge::ROOT, "greeting", "hello").unwrap();
        Bytes::from(doc.save())
    }

    #[tokio::test]
    async fn publish_valid_change_round_trips() {
        let kind = AutomergeChannel::new();
        let p0 = kind
            .on_publish("doc.lobby", automerge_change_setting_greeting())
            .await
            .unwrap();
        assert_eq!(p0.offset, 0);

        let snapshot = kind.snapshot("doc.lobby").expect("snapshot");
        let replay = AutoCommit::load(&snapshot).expect("load");
        let value = replay.get(::automerge::ROOT, "greeting").unwrap();
        assert!(value.is_some(), "greeting key visible after replay");
    }

    #[tokio::test]
    async fn publish_rejects_garbage() {
        let kind = AutomergeChannel::new();
        let err = kind
            .on_publish("doc.lobby", Bytes::from_static(&[0xff, 0xff, 0xff]))
            .await
            .expect_err("garbage rejected");
        assert!(matches!(err, ChannelError::RejectedPublication { .. }));
    }
}
