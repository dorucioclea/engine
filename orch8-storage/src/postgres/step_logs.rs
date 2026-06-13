use sqlx::Row;

use orch8_types::error::StorageError;
use orch8_types::ids::{BlockId, InstanceId};
use orch8_types::step_log::{StepLog, StepLogEntry};

use super::PostgresStorage;

pub(super) async fn append(
    store: &PostgresStorage,
    instance_id: InstanceId,
    block_id: &BlockId,
    entries: &[StepLogEntry],
) -> Result<(), StorageError> {
    if entries.is_empty() {
        return Ok(());
    }
    let mut tx = store.pool.begin().await?;
    for e in entries {
        sqlx::query(
            r"INSERT INTO step_logs (id, instance_id, block_id, ts, level, message)
              VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(instance_id.into_uuid())
        .bind(block_id.as_str())
        .bind(e.ts)
        .bind(&e.level)
        .bind(&e.message)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub(super) async fn list(
    store: &PostgresStorage,
    instance_id: InstanceId,
) -> Result<Vec<StepLog>, StorageError> {
    let rows = sqlx::query(
        r"SELECT block_id, ts, level, message FROM step_logs
          WHERE instance_id = $1 ORDER BY ts ASC",
    )
    .bind(instance_id.into_uuid())
    .fetch_all(&store.pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| StepLog {
            block_id: row.get("block_id"),
            ts: row.get("ts"),
            level: row.get("level"),
            message: row.get("message"),
        })
        .collect())
}
