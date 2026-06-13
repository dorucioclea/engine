//! Integration tests for the alert-only SLA breach sweep.
//!
//! A sequence may declare `sla.max_runtime` / `sla.max_step_runtime`. When an
//! active instance exceeds either bound the scheduler emits an
//! `instance.sla_breached` webhook + `orch8_sla_breached_total` metric and
//! writes a sentinel block output for once-only de-dup — without changing the
//! instance's state.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio_util::sync::CancellationToken;

use orch8_engine::handlers::HandlerRegistry;
use orch8_engine::scheduler::tick_once;
use orch8_storage::StorageBackend;
use orch8_types::ids::BlockId;
use orch8_types::instance::InstanceState;
use orch8_types::sequence::SlaPolicy;

mod common;
use common::*;

async fn run_tick(storage: &Arc<dyn StorageBackend>, handlers: &Arc<HandlerRegistry>) {
    let sem = semaphore(128);
    let config = default_config();
    let seq_cache = cache();
    let cancel = CancellationToken::new();
    tick_once(storage, handlers, &sem, &config, &seq_cache, &cancel)
        .await
        .unwrap();
}

async fn sentinel_count(storage: &Arc<dyn StorageBackend>, inst_id: orch8_types::ids::InstanceId, block: &str) -> usize {
    storage
        .get_all_outputs(inst_id)
        .await
        .unwrap()
        .into_iter()
        .filter(|o| o.block_id.as_str() == block)
        .count()
}

#[tokio::test]
async fn max_runtime_breach_emits_once() {
    let storage = storage().await;
    let handlers = Arc::new(registry());

    let mut seq = mk_sequence(vec![mk_step("s1", "noop")]);
    seq.sla = Some(SlaPolicy {
        max_runtime: Some(Duration::from_secs(60)),
        max_step_runtime: None,
    });
    storage.create_sequence(&seq).await.unwrap();

    // A Waiting instance created an hour ago — well past the 60s budget, and
    // Waiting so the normal claim path leaves it untouched.
    let mut inst = mk_instance_in_state(seq.id, InstanceState::Waiting);
    inst.created_at = Utc::now() - chrono::Duration::hours(1);
    storage.create_instance(&inst).await.unwrap();

    run_tick(&storage, &handlers).await;
    assert_eq!(
        sentinel_count(&storage, inst.id, "_sla:runtime").await,
        1,
        "first tick should record exactly one runtime breach"
    );

    // Still Waiting — alert-only, no state change.
    let after = storage.get_instance(inst.id).await.unwrap().unwrap();
    assert_eq!(after.state, InstanceState::Waiting);

    // Second tick must not re-alert (sentinel de-dup).
    run_tick(&storage, &handlers).await;
    assert_eq!(
        sentinel_count(&storage, inst.id, "_sla:runtime").await,
        1,
        "second tick must not re-alert the same breach"
    );
}

#[tokio::test]
async fn within_budget_does_not_breach() {
    let storage = storage().await;
    let handlers = Arc::new(registry());

    let mut seq = mk_sequence(vec![mk_step("s1", "noop")]);
    seq.sla = Some(SlaPolicy {
        max_runtime: Some(Duration::from_secs(3600)),
        max_step_runtime: None,
    });
    storage.create_sequence(&seq).await.unwrap();

    // Created just now — far inside the 1h budget.
    let inst = mk_instance_in_state(seq.id, InstanceState::Waiting);
    storage.create_instance(&inst).await.unwrap();

    run_tick(&storage, &handlers).await;
    assert_eq!(
        sentinel_count(&storage, inst.id, "_sla:runtime").await,
        0,
        "an instance inside its budget must not breach"
    );
}

#[tokio::test]
async fn max_step_runtime_breach_emits_once() {
    let storage = storage().await;
    let handlers = Arc::new(registry());

    let mut seq = mk_sequence(vec![mk_step("s1", "noop")]);
    seq.sla = Some(SlaPolicy {
        max_runtime: None,
        max_step_runtime: Some(Duration::from_secs(30)),
    });
    storage.create_sequence(&seq).await.unwrap();

    let mut inst = mk_instance_in_state(seq.id, InstanceState::Waiting);
    // Current step started 10 minutes ago — past the 30s step budget.
    inst.context.runtime.current_step = Some(BlockId::new("s1"));
    inst.context.runtime.current_step_started_at = Some(Utc::now() - chrono::Duration::minutes(10));
    storage.create_instance(&inst).await.unwrap();

    run_tick(&storage, &handlers).await;
    assert_eq!(
        sentinel_count(&storage, inst.id, "_sla:step:s1").await,
        1,
        "step runtime breach should record once"
    );

    run_tick(&storage, &handlers).await;
    assert_eq!(
        sentinel_count(&storage, inst.id, "_sla:step:s1").await,
        1,
        "step breach must not re-alert"
    );
}
