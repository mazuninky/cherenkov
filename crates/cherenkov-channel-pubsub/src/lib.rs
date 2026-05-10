//! Plain pub/sub [`cherenkov_core::ChannelKind`] for Cherenkov.
//!
//! `PubSubChannel` keeps a bounded in-memory ring buffer per channel,
//! evicting entries by both age (TTL) and count (max size). Replay on
//! reconnect is reserved for a later milestone — at M1 the `epoch` /
//! `offset` returned on subscribe is informational only.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bytes::Bytes;
use cherenkov_core::{ChannelCursor, ChannelError, ChannelKind};
use cherenkov_protocol::Publication;
use dashmap::DashMap;
use parking_lot::Mutex;

/// Default per-channel ring-buffer cap.
pub const DEFAULT_HISTORY_SIZE: usize = 256;
/// Default per-channel history TTL.
pub const DEFAULT_HISTORY_TTL: Duration = Duration::from_secs(300);

/// Pub/sub channel kind.
///
/// Cheap to clone (internally `Arc`-shared); embed inside an `Arc` and pass
/// to [`cherenkov_core::HubBuilder::with_channel_kind`].
pub struct PubSubChannel {
    epoch: u64,
    history_size: usize,
    history_ttl: Duration,
    state: DashMap<String, Mutex<ChannelState>>,
}

struct ChannelState {
    next_offset: u64,
    entries: VecDeque<HistoryEntry>,
}

struct HistoryEntry {
    inserted_at: Instant,
    offset: u64,
    /// Retained payload bytes — required for replay-on-resubscribe.
    data: Bytes,
}

impl PubSubChannel {
    /// Construct a channel with default history bounds.
    #[must_use]
    pub fn new() -> Self {
        Self::with_bounds(DEFAULT_HISTORY_SIZE, DEFAULT_HISTORY_TTL)
    }

    /// Construct a channel with a custom history size cap and TTL.
    ///
    /// `history_size` clamps to at least 1; `history_ttl` of zero means
    /// "evict immediately on next publish".
    #[must_use]
    pub fn with_bounds(history_size: usize, history_ttl: Duration) -> Self {
        Self {
            epoch: 1,
            history_size: history_size.max(1),
            history_ttl,
            state: DashMap::new(),
        }
    }

    fn evict_expired(&self, state: &mut ChannelState, now: Instant) {
        let ttl = self.history_ttl;
        while let Some(front) = state.entries.front() {
            if now.saturating_duration_since(front.inserted_at) > ttl {
                state.entries.pop_front();
            } else {
                break;
            }
        }
    }

    fn cursor_locked(&self, state: &ChannelState) -> ChannelCursor {
        ChannelCursor {
            epoch: self.epoch,
            offset: state.next_offset.saturating_sub(1),
        }
    }

    /// Returns the current cursor for `channel`, or a default cursor if the
    /// channel has never been touched.
    #[must_use]
    pub fn cursor(&self, channel: &str) -> ChannelCursor {
        match self.state.get(channel) {
            Some(entry) => self.cursor_locked(&entry.lock()),
            None => ChannelCursor {
                epoch: self.epoch,
                offset: 0,
            },
        }
    }

    /// Number of entries currently retained for `channel` (after eviction).
    #[must_use]
    pub fn history_len(&self, channel: &str) -> usize {
        self.state
            .get(channel)
            .map(|e| e.lock().entries.len())
            .unwrap_or(0)
    }
}

impl Default for PubSubChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChannelKind for PubSubChannel {
    fn name(&self) -> &'static str {
        "pubsub"
    }

    async fn on_subscribe(&self, channel: &str) -> Result<ChannelCursor, ChannelError> {
        if channel.is_empty() {
            return Err(ChannelError::InvalidChannel {
                channel: channel.to_owned(),
            });
        }
        Ok(self.cursor(channel))
    }

    async fn on_unsubscribe(&self, _channel: &str) -> Result<(), ChannelError> {
        // History is retained until eviction by TTL/size; no per-subscriber
        // bookkeeping at M1.
        Ok(())
    }

    async fn on_publish(&self, channel: &str, data: Bytes) -> Result<Publication, ChannelError> {
        if channel.is_empty() {
            return Err(ChannelError::InvalidChannel {
                channel: channel.to_owned(),
            });
        }
        let now = Instant::now();
        let entry = self.state.entry(channel.to_owned()).or_insert_with(|| {
            Mutex::new(ChannelState {
                next_offset: 0,
                entries: VecDeque::with_capacity(self.history_size),
            })
        });
        let mut state = entry.lock();
        self.evict_expired(&mut state, now);

        let offset = state.next_offset;
        state.next_offset = state.next_offset.saturating_add(1);
        state.entries.push_back(HistoryEntry {
            inserted_at: now,
            offset,
            data: data.clone(),
        });
        while state.entries.len() > self.history_size {
            state.entries.pop_front();
        }

        Ok(Publication {
            channel: channel.to_owned(),
            offset,
            data,
        })
    }

    async fn replay_since(
        &self,
        channel: &str,
        since_offset: u64,
    ) -> Result<Vec<Publication>, ChannelError> {
        let Some(entry) = self.state.get(channel) else {
            return Ok(Vec::new());
        };
        let mut state = entry.lock();
        // Drop expired entries before scanning so callers never observe
        // history older than the configured TTL.
        let now = Instant::now();
        self.evict_expired(&mut state, now);
        let out = state
            .entries
            .iter()
            .filter(|e| e.offset > since_offset)
            .map(|e| Publication {
                channel: channel.to_owned(),
                offset: e.offset,
                data: e.data.clone(),
            })
            .collect();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bytes::Bytes;

    use super::*;

    #[tokio::test]
    async fn happy_path_publish_assigns_increasing_offsets() {
        let channel = PubSubChannel::new();
        let p0 = channel
            .on_publish("rooms.lobby", Bytes::from_static(b"a"))
            .await
            .unwrap();
        let p1 = channel
            .on_publish("rooms.lobby", Bytes::from_static(b"b"))
            .await
            .unwrap();
        assert_eq!(p0.offset, 0);
        assert_eq!(p1.offset, 1);
    }

    #[tokio::test]
    async fn empty_channel_name_is_rejected() {
        let channel = PubSubChannel::new();
        let err = channel.on_subscribe("").await.expect_err("must reject");
        assert!(matches!(err, ChannelError::InvalidChannel { .. }));
        let err = channel
            .on_publish("", Bytes::from_static(b"x"))
            .await
            .expect_err("must reject");
        assert!(matches!(err, ChannelError::InvalidChannel { .. }));
    }

    #[tokio::test]
    async fn history_size_caps_retention() {
        let channel = PubSubChannel::with_bounds(3, Duration::from_secs(60));
        for i in 0..5u8 {
            channel
                .on_publish("rooms.lobby", Bytes::copy_from_slice(&[i]))
                .await
                .unwrap();
        }
        assert_eq!(channel.history_len("rooms.lobby"), 3);
    }

    #[tokio::test]
    async fn ttl_evicts_old_entries_on_next_publish() {
        let channel = PubSubChannel::with_bounds(64, Duration::from_millis(0));
        channel
            .on_publish("rooms.lobby", Bytes::from_static(b"a"))
            .await
            .unwrap();
        // Sleep to ensure the previous entry's age exceeds the zero TTL.
        tokio::time::sleep(Duration::from_millis(5)).await;
        channel
            .on_publish("rooms.lobby", Bytes::from_static(b"b"))
            .await
            .unwrap();
        assert_eq!(
            channel.history_len("rooms.lobby"),
            1,
            "first entry should have been evicted by TTL",
        );
    }

    #[tokio::test]
    async fn cursor_reflects_most_recent_offset() {
        let channel = PubSubChannel::new();
        for _ in 0..3 {
            channel
                .on_publish("rooms.lobby", Bytes::from_static(b"x"))
                .await
                .unwrap();
        }
        let cursor = channel.on_subscribe("rooms.lobby").await.unwrap();
        assert_eq!(cursor.offset, 2);
    }

    #[tokio::test]
    async fn replay_since_returns_entries_after_cursor() {
        let channel = PubSubChannel::new();
        channel
            .on_publish("rooms.lobby", Bytes::from_static(b"a"))
            .await
            .unwrap();
        channel
            .on_publish("rooms.lobby", Bytes::from_static(b"b"))
            .await
            .unwrap();
        channel
            .on_publish("rooms.lobby", Bytes::from_static(b"c"))
            .await
            .unwrap();
        // Strict-greater-than semantics: replay_since(N) returns offsets > N.
        let replay_after_first = channel.replay_since("rooms.lobby", 0).await.unwrap();
        assert_eq!(replay_after_first.len(), 2);
        assert_eq!(replay_after_first[0].offset, 1);
        assert_eq!(replay_after_first[1].offset, 2);

        let replay_after_second = channel.replay_since("rooms.lobby", 1).await.unwrap();
        assert_eq!(replay_after_second.len(), 1);
        assert_eq!(replay_after_second[0].offset, 2);
        assert_eq!(replay_after_second[0].data, Bytes::from_static(b"c"));

        let nothing_left = channel.replay_since("rooms.lobby", 99).await.unwrap();
        assert!(nothing_left.is_empty());
    }

    #[tokio::test]
    async fn replay_since_empty_when_history_evicted() {
        let channel = PubSubChannel::with_bounds(2, Duration::from_secs(60));
        for i in 0..5u8 {
            channel
                .on_publish("rooms.lobby", Bytes::copy_from_slice(&[i]))
                .await
                .unwrap();
        }
        let replay = channel.replay_since("rooms.lobby", 1).await.unwrap();
        // Only the last 2 entries (offset 3 and 4) survived eviction.
        assert_eq!(replay.len(), 2);
        assert_eq!(replay[0].offset, 3);
        assert_eq!(replay[1].offset, 4);
    }
}
