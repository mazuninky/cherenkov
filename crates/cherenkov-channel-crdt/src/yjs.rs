//! Y.js channel kind backed by the [`yrs`] crate.
//!
//! Each channel owns one [`yrs::Doc`]; subscribe is a no-op for state
//! tracking, publish applies the incoming update to the doc and returns
//! a [`Publication`] carrying the same bytes back so subscribers can
//! merge it into their own copy.
//!
//! Storage is in-memory only. A subscriber that joins after history has
//! been published cannot reconstruct the document from past
//! `Publication`s alone — out-of-band state-vector sync is the
//! responsibility of the application or a future replay layer.

use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use bytes::Bytes;
use cherenkov_core::{ChannelCursor, ChannelError, ChannelKind};
use cherenkov_protocol::Publication;
use dashmap::DashMap;
use parking_lot::Mutex;
use yrs::updates::decoder::Decode;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

use crate::CrdtError;

/// In-memory Y.js channel kind.
#[derive(Default)]
pub struct YjsChannel {
    docs: DashMap<String, Mutex<DocSlot>>,
}

struct DocSlot {
    doc: Doc,
    next_offset: AtomicU64,
}

impl YjsChannel {
    /// Construct an empty channel kind.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Encode the current state of `channel` as a `yrs` update suitable
    /// for sending to a freshly-joined subscriber.
    #[must_use]
    pub fn snapshot(&self, channel: &str) -> Option<Vec<u8>> {
        let entry = self.docs.get(channel)?;
        let slot = entry.lock();
        let txn = slot.doc.transact();
        Some(txn.encode_state_as_update_v1(&StateVector::default()))
    }
}

#[async_trait]
impl ChannelKind for YjsChannel {
    fn name(&self) -> &'static str {
        "crdt-yjs"
    }

    async fn on_subscribe(&self, channel: &str) -> Result<ChannelCursor, ChannelError> {
        if channel.is_empty() {
            return Err(ChannelError::InvalidChannel {
                channel: channel.to_owned(),
            });
        }
        let entry = self.docs.entry(channel.to_owned()).or_insert_with(|| {
            Mutex::new(DocSlot {
                doc: Doc::new(),
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
        let update = Update::decode_v1(&data).map_err(|e| ChannelError::RejectedPublication {
            reason: CrdtError::InvalidUpdate {
                engine: "yjs",
                channel: channel.to_owned(),
                reason: e.to_string(),
            }
            .to_string(),
        })?;

        let entry = self.docs.entry(channel.to_owned()).or_insert_with(|| {
            Mutex::new(DocSlot {
                doc: Doc::new(),
                next_offset: AtomicU64::new(0),
            })
        });
        let slot = entry.lock();
        {
            let mut txn = slot.doc.transact_mut();
            txn.apply_update(update)
                .map_err(|e| ChannelError::RejectedPublication {
                    reason: CrdtError::InvalidUpdate {
                        engine: "yjs",
                        channel: channel.to_owned(),
                        reason: e.to_string(),
                    }
                    .to_string(),
                })?;
        }
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
    use yrs::{GetString, Text, Transact};

    use super::*;

    fn yjs_update_inserting_hello() -> Bytes {
        let doc = Doc::new();
        let text = doc.get_or_insert_text("greeting");
        let mut txn = doc.transact_mut();
        text.insert(&mut txn, 0, "hello");
        let bytes = txn.encode_update_v1();
        Bytes::from(bytes)
    }

    #[tokio::test]
    async fn publish_valid_update_round_trips_and_advances_offset() {
        let kind = YjsChannel::new();
        let p0 = kind
            .on_publish("doc.lobby", yjs_update_inserting_hello())
            .await
            .expect("first publish ok");
        assert_eq!(p0.offset, 0);
        let p1 = kind
            .on_publish("doc.lobby", yjs_update_inserting_hello())
            .await
            .expect("second publish ok");
        assert_eq!(p1.offset, 1);
    }

    #[tokio::test]
    async fn publish_rejects_garbage() {
        let kind = YjsChannel::new();
        let err = kind
            .on_publish("doc.lobby", Bytes::from_static(&[0xff, 0xff, 0xff]))
            .await
            .expect_err("garbage rejected");
        assert!(matches!(err, ChannelError::RejectedPublication { .. }));
    }

    #[tokio::test]
    async fn snapshot_reflects_applied_updates() {
        let kind = YjsChannel::new();
        kind.on_publish("doc.lobby", yjs_update_inserting_hello())
            .await
            .unwrap();
        let snapshot = kind.snapshot("doc.lobby").expect("snapshot present");
        assert!(!snapshot.is_empty());

        let replay_doc = Doc::new();
        let _ = replay_doc.get_or_insert_text("greeting");
        let update = Update::decode_v1(&snapshot).expect("decode");
        replay_doc
            .transact_mut()
            .apply_update(update)
            .expect("apply");
        let text = replay_doc.get_or_insert_text("greeting");
        let txn = replay_doc.transact();
        assert_eq!(text.get_string(&txn), "hello");
    }

    #[tokio::test]
    async fn empty_channel_name_is_rejected() {
        let kind = YjsChannel::new();
        let err = kind
            .on_subscribe("")
            .await
            .expect_err("empty subscribe rejected");
        assert!(matches!(err, ChannelError::InvalidChannel { .. }));
        let err = kind
            .on_publish("", Bytes::new())
            .await
            .expect_err("empty publish rejected");
        assert!(matches!(err, ChannelError::InvalidChannel { .. }));
    }
}
