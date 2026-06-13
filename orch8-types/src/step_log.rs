//! Step-scoped logs.
//!
//! A step can accumulate log lines two ways: an **external worker** attaches
//! them when completing/failing a task (`logs: [{ts, level, message}]`), and
//! the engine captures **in-process** handler logs via a tracing layer scoped
//! to the `orch8.step` span. Both land in the same `step_logs` store, keyed by
//! `(instance_id, block_id)`, and surface in the dashboard's per-execution Logs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One log line captured during (or reported for) a step's execution.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StepLogEntry {
    pub ts: DateTime<Utc>,
    /// `trace` | `debug` | `info` | `warn` | `error`.
    pub level: String,
    pub message: String,
}

/// A stored step log line, annotated with the block it belongs to.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StepLog {
    pub block_id: String,
    pub ts: DateTime<Utc>,
    pub level: String,
    pub message: String,
}
