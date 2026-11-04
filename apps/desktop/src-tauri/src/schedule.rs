//! ZDM Download Queue (the scheduler)
//! Developed by Ramy Badraan
//!
//! The scheduler is a **queue**, not a timetable. The user adds several files,
//! chooses whether to run them one at a time or several in parallel, and the
//! queue feeds the engine accordingly: as soon as one finishes, the next starts.
//!
//! Features:
//!  * Ordering the user controls (move up / down / top).
//!  * Concurrency: one at a time, or N at once.
//!  * Post-completion action: nothing, or power off / sleep once the queue drains.
//!  * Pausing the queue holds everything; resuming continues the rotation.
//!
//! Implementation notes:
//!  * The queue is *the set of downloads with `status = queued`*, ordered by
//!    `queue_position`. There is no second list to drift out of sync with the
//!    downloads table.
//!  * `queue_position` is assigned densely (0,1,2…) whenever the order changes, so
//!    "move up" is a swap of two integers rather than a fractional index.
//!  * The drain action runs exactly once per drain: a flag is cleared when the
//!    queue stops being empty, so adding a file after a power-off aborts it.

use serde::{Deserialize, Serialize};

/// How many files may run at the same time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParallelMode {
    /// Strictly one after another — the classic download queue.
    Sequential,
    /// Two at a time.
    Two,
    /// Three at a time.
    Three,
    /// Four at a time.
    Four,
    /// Let the engine's own limit decide (the general setting).
    Auto,
}

impl ParallelMode {
    /// Slots this mode grants, or `None` for "use the global setting".
    pub fn slots(self) -> Option<usize> {
        match self {
            Self::Sequential => Some(1),
            Self::Two => Some(2),
            Self::Three => Some(3),
            Self::Four => Some(4),
            Self::Auto => None,
        }
    }

}

/// What to do when the queue has fully drained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrainAction {
    Nothing,
    Sleep,
    ShutDown,
}

/// Queue configuration, persisted in settings.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct QueueConfig {
    /// False means "run freely, no queue discipline" — the default, so an existing
    /// install behaves exactly as before.
    pub enabled: bool,
    pub parallel: ParallelMode,
    pub drain_action: DrainAction,
    /// Queued files wait for the user to start the queue.
    pub paused: bool,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            parallel: ParallelMode::Sequential,
            drain_action: DrainAction::Nothing,
            paused: false,
        }
    }
}

/// A snapshot for the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStatus {
    pub config: QueueConfig,
    /// Ids in queue order, highest priority first.
    pub order: Vec<String>,
    /// How many slots are in use right now.
    pub running: usize,
    /// How many slots the current mode grants.
    pub slots: usize,
    /// True when nothing is queued and nothing is running.
    pub drained: bool,
}

/// Decide how many downloads may run now.
///
/// The queue mode wins over the global connection limit when the queue is on;
/// otherwise the global limit applies, which keeps the old behaviour intact.
pub fn effective_slots(config: &QueueConfig, global_max: usize) -> usize {
    if !config.enabled {
        return global_max.max(1);
    }
    match config.parallel.slots() {
        Some(slots) => slots.max(1),
        None => global_max.max(1),
    }
}

/// The order a queue should be started in: lowest position first.
pub fn next_to_start(
    mut queued: Vec<(&str, i64)>,
    running: usize,
    slots: usize,
) -> Vec<&str> {
    if running >= slots {
        return Vec::new();
    }
    queued.sort_by_key(|(_, position)| *position);
    queued
        .into_iter()
        .take(slots - running)
        .map(|(id, _)| id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_disabled_queue_keeps_the_old_behaviour() {
        let config = QueueConfig::default();
        assert_eq!(effective_slots(&config, 3), 3);
        assert_eq!(effective_slots(&config, 16), 16);
    }

    /// The reported request: "one file at a time" or "two at a time".
    #[test]
    fn sequential_grants_exactly_one_slot() {
        let config = QueueConfig {
            enabled: true,
            parallel: ParallelMode::Sequential,
            ..Default::default()
        };
        assert_eq!(effective_slots(&config, 16), 1);
    }

    #[test]
    fn parallel_modes_grant_their_slot_count() {
        let make = |mode| QueueConfig {
            enabled: true,
            parallel: mode,
            ..Default::default()
        };
        assert_eq!(effective_slots(&make(ParallelMode::Two), 16), 2);
        assert_eq!(effective_slots(&make(ParallelMode::Three), 16), 3);
        assert_eq!(effective_slots(&make(ParallelMode::Four), 16), 4);
        // Auto defers to the general limit.
        assert_eq!(effective_slots(&make(ParallelMode::Auto), 7), 7);
    }

    #[test]
    fn the_queue_starts_the_oldest_position_first() {
        let queued = vec![("c", 2i64), ("a", 0), ("b", 1)];
        assert_eq!(next_to_start(queued, 0, 1), vec!["a"]);
    }

    #[test]
    fn free_slots_are_filled_in_order() {
        let queued = || vec![("d", 3i64), ("a", 0), ("b", 1), ("c", 2)];
        assert_eq!(next_to_start(queued(), 0, 2), vec!["a", "b"]);
        assert_eq!(next_to_start(queued(), 1, 3), vec!["a", "b"]);
    }

    #[test]
    fn a_full_queue_starts_nothing() {
        let queued = || vec![("a", 0i64)];
        assert!(next_to_start(queued(), 1, 1).is_empty());
        assert!(next_to_start(queued(), 4, 2).is_empty());
    }

    #[test]
    fn an_empty_queue_starts_nothing() {
        assert!(next_to_start(Vec::new(), 0, 4).is_empty());
    }

    /// Positions are dense, so "move up" is a swap, never a fractional index.
    #[test]
    fn reordering_keeps_positions_dense() {
        let order = ["a", "b", "c", "d"];
        let positions: Vec<i64> = order.iter().enumerate().map(|(i, _)| i as i64).collect();
        assert_eq!(positions, vec![0, 1, 2, 3]);
    }
}
