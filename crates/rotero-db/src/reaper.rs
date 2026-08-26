//! Removing tombstones once every device has certainly seen them.
//!
//! A tombstone is a row kept only to carry a deletion. It has to outlive the
//! delete long enough for every device to merge it, but not forever — so the
//! question is when "long enough" has provably passed.
//!
//! Reaping is the only operation here that destroys data irreversibly, and
//! getting it wrong resurrects deleted papers. Three conditions bound it:
//!
//! 1. **Only this device's own tombstones.** Each device is the sole writer of
//!    its own snapshot, so removing a peer's tombstone from the local mirror
//!    achieves nothing — the next merge reads it straight back. Worse, if the
//!    peer has already reaped it, deleting the local copy loses the deletion.
//! 2. **Only past every peer's horizon.** A tombstone is safe to drop when it
//!    is older than the oldest snapshot any peer has published, by a wide
//!    margin. If a peer has not been seen in a long time, its horizon holds
//!    everything back — which is the correct, conservative direction.
//! 3. **Only past the TTL.** A fixed floor under the horizon check, so a
//!    momentary lack of peers cannot make everything reapable at once.
//!
//! A device offline longer than the TTL comes back holding rows the others have
//! reaped, re-publishes them, and resurrects them. That is the accepted cost of
//! ever reaping at all, and it is why the TTL is months rather than days.

use turso::Value;

use crate::Database;
use crate::sync_schema::SYNCED_TABLES;

/// How long a tombstone must be untouched before it can be removed.
pub const TOMBSTONE_TTL_MS: i64 = 180 * 24 * 60 * 60 * 1000;

/// How often the reaper runs, regardless of how often the app launches.
const REAP_INTERVAL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// The `app_flags` key recording the last reap.
const LAST_REAP_KEY: &str = "tombstone_last_reap_ms";

/// What a reap removed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReapStats {
    /// Tombstones permanently removed.
    pub removed: usize,
}

impl Database {
    /// Remove this device's long-settled tombstones.
    ///
    /// `peer_horizon_ms` is the oldest `generated_at` across every peer snapshot
    /// currently readable, which is what proves those devices have seen
    /// everything older than it. Pass `None` when no peer snapshot could be read
    /// — nothing is reaped, because there is then no evidence anyone has seen
    /// these deletions.
    ///
    /// Intended to run at startup rather than per sync tick: it is a full scan,
    /// and once a week is ample for rows that are already invisible.
    pub async fn reap_tombstones(
        &self,
        peer_horizon_ms: Option<i64>,
        now_ms: i64,
    ) -> Result<ReapStats, crate::DbError> {
        let Some(horizon) = peer_horizon_ms else {
            return Ok(ReapStats::default());
        };

        if !self.reap_is_due(now_ms).await? {
            return Ok(ReapStats::default());
        }

        // Both bounds must hold. The horizon proves peers have seen the
        // deletion; the TTL keeps a single freshly-written peer snapshot from
        // making the entire history reapable in one pass.
        let cutoff = horizon.min(now_ms - TOMBSTONE_TTL_MS) - TOMBSTONE_TTL_MS;

        let mut removed = 0;
        for table in SYNCED_TABLES {
            let affected = self
                .conn()
                .execute(
                    &format!(
                        "DELETE FROM {} WHERE deleted = 1 AND updated_at < ?1 AND updated_by = ?2",
                        table.name
                    ),
                    turso::params::Params::Positional(vec![
                        Value::Integer(cutoff),
                        Value::Text(self.device_id().to_string()),
                    ]),
                )
                .await?;
            removed += affected as usize;
        }

        self.set_app_flag(LAST_REAP_KEY, &now_ms.to_string()).await?;
        Ok(ReapStats { removed })
    }

    /// Whether enough time has passed since the last reap.
    async fn reap_is_due(&self, now_ms: i64) -> Result<bool, crate::DbError> {
        let last = self
            .get_app_flag(LAST_REAP_KEY)
            .await?
            .and_then(|v| v.parse::<i64>().ok());
        Ok(match last {
            Some(last) => now_ms - last >= REAP_INTERVAL_MS,
            None => true,
        })
    }
}
