//! Dynamic task-queue routing rules.
//!
//! A rule overrides the queue an external-worker task lands on, evaluated at
//! enqueue time. Keyed by `(tenant_id, handler_name)` with an optional
//! `match_queue` (apply only when the task's declared queue equals this — i.e.
//! remap queue X → Y). Highest `priority` wins; `enabled = false` disables a
//! rule without deleting it. Closes the gap Temporal tracks as #1988
//! (per-tenant/per-handler queue routing).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QueueRoutingRule {
    pub id: Uuid,
    pub tenant_id: String,
    /// The handler name this rule applies to.
    pub handler_name: String,
    /// When set, the rule only applies if the task's currently-declared queue
    /// equals this value (queue remap). `None` matches any current queue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_queue: Option<String>,
    /// The queue the matching task is routed to instead.
    pub queue_override: String,
    /// Higher priority rules are evaluated first; the first match wins.
    #[serde(default)]
    pub priority: i32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const fn default_true() -> bool {
    true
}

impl QueueRoutingRule {
    /// Does this rule apply to a task with the given current queue?
    #[must_use]
    pub fn matches(&self, current_queue: Option<&str>) -> bool {
        self.enabled
            && match &self.match_queue {
                None => true,
                Some(q) => current_queue == Some(q.as_str()),
            }
    }
}
