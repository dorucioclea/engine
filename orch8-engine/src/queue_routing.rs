//! Dynamic task-queue routing applied at enqueue.
//!
//! When a step is dispatched to an external worker, its declared `queue_name`
//! can be overridden by a per-(tenant, handler) routing rule — so an operator
//! can move a handler's traffic onto a dedicated queue without redeploying the
//! sequence. Closes the queue-routing gap Temporal tracks as #1988.

use orch8_storage::StorageBackend;
use orch8_types::ids::TenantId;

/// Resolve the queue an external-worker task should land on. Returns the
/// `queue_override` of the highest-priority matching rule, else the `declared`
/// queue (the step's own `queue_name`). Best-effort: a storage error logs and
/// falls back to `declared`, so routing never blocks dispatch.
pub async fn resolve_queue(
    storage: &dyn StorageBackend,
    tenant_id: &TenantId,
    handler: &str,
    declared: Option<String>,
) -> Option<String> {
    match storage
        .list_queue_routing_rules(Some(tenant_id), Some(handler))
        .await
    {
        Ok(rules) => {
            // Rules arrive ordered by priority DESC; first match wins.
            for rule in &rules {
                if rule.matches(declared.as_deref()) {
                    return Some(rule.queue_override.clone());
                }
            }
            declared
        }
        Err(e) => {
            tracing::warn!(
                handler,
                error = %e,
                "queue routing lookup failed; using declared queue"
            );
            declared
        }
    }
}
