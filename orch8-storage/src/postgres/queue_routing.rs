use sqlx::Row;
use uuid::Uuid;

use orch8_types::error::StorageError;
use orch8_types::ids::TenantId;
use orch8_types::queue_routing::QueueRoutingRule;

use super::PostgresStorage;

fn row_to_rule(row: &sqlx::postgres::PgRow) -> QueueRoutingRule {
    QueueRoutingRule {
        id: row.get("id"),
        tenant_id: row.get("tenant_id"),
        handler_name: row.get("handler_name"),
        match_queue: row.get("match_queue"),
        queue_override: row.get("queue_override"),
        priority: row.get("priority"),
        enabled: row.get("enabled"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

pub(super) async fn create(
    store: &PostgresStorage,
    rule: &QueueRoutingRule,
) -> Result<(), StorageError> {
    sqlx::query(
        r"INSERT INTO queue_routing_rules
            (id, tenant_id, handler_name, match_queue, queue_override, priority, enabled, created_at, updated_at)
          VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(rule.id)
    .bind(&rule.tenant_id)
    .bind(&rule.handler_name)
    .bind(&rule.match_queue)
    .bind(&rule.queue_override)
    .bind(rule.priority)
    .bind(rule.enabled)
    .bind(rule.created_at)
    .bind(rule.updated_at)
    .execute(&store.pool)
    .await?;
    Ok(())
}

pub(super) async fn list(
    store: &PostgresStorage,
    tenant_id: Option<&TenantId>,
    handler_name: Option<&str>,
) -> Result<Vec<QueueRoutingRule>, StorageError> {
    let mut qb = sqlx::QueryBuilder::new(
        r"SELECT id, tenant_id, handler_name, match_queue, queue_override, priority, enabled, created_at, updated_at
          FROM queue_routing_rules WHERE 1=1",
    );
    if let Some(t) = tenant_id {
        qb.push(" AND tenant_id=").push_bind(t.as_str().to_string());
    }
    if let Some(h) = handler_name {
        qb.push(" AND handler_name=").push_bind(h.to_string());
    }
    qb.push(" ORDER BY priority DESC, created_at ASC");
    let rows = qb.build().fetch_all(&store.pool).await?;
    Ok(rows.iter().map(row_to_rule).collect())
}

pub(super) async fn get(
    store: &PostgresStorage,
    id: Uuid,
) -> Result<Option<QueueRoutingRule>, StorageError> {
    let row = sqlx::query(
        r"SELECT id, tenant_id, handler_name, match_queue, queue_override, priority, enabled, created_at, updated_at
          FROM queue_routing_rules WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&store.pool)
    .await?;
    Ok(row.as_ref().map(row_to_rule))
}

pub(super) async fn delete(store: &PostgresStorage, id: Uuid) -> Result<(), StorageError> {
    sqlx::query("DELETE FROM queue_routing_rules WHERE id = $1")
        .bind(id)
        .execute(&store.pool)
        .await?;
    Ok(())
}
