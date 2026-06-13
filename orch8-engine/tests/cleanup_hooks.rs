//! Integration tests for sequence-level best-effort cleanup hooks
//! (`on_failure` / `on_cancel`). These run as the instance reaches a terminal
//! state, dispatching their step blocks once, errors swallowed, so a dying run
//! can release resources or notify instead of "just disappearing".

use std::sync::Arc;

use serde_json::json;
use tokio_util::sync::CancellationToken;

use orch8_engine::handlers::HandlerRegistry;
use orch8_engine::scheduler::tick_once;
use orch8_storage::StorageBackend;
use orch8_types::error::StepError;
use orch8_types::ids::BlockId;
use orch8_types::instance::InstanceState;
use orch8_types::signal::{Signal, SignalType};

mod common;
use common::*;

/// Registry: a permanently-failing `boom` handler plus a `cleanup` handler that
/// records that it ran (returns a marker output).
fn registry_with_cleanup() -> HandlerRegistry {
    let mut reg = registry();
    reg.register("boom", |_ctx| {
        Box::pin(async {
            Err(StepError::Permanent {
                message: "boom".into(),
                details: None,
            })
        })
    });
    reg.register("cleanup", |_ctx| {
        Box::pin(async { Ok(json!({ "cleaned": true })) })
    });
    reg
}

async fn run_until_terminal(
    storage: &Arc<dyn StorageBackend>,
    handlers: &Arc<HandlerRegistry>,
    inst_id: orch8_types::ids::InstanceId,
) -> InstanceState {
    let sem = semaphore(128);
    let config = default_config();
    let seq_cache = cache();
    let cancel = CancellationToken::new();
    for _ in 0..50 {
        tick_once(storage, handlers, &sem, &config, &seq_cache, &cancel)
            .await
            .unwrap();
        let inst = storage.get_instance(inst_id).await.unwrap().unwrap();
        if matches!(
            inst.state,
            InstanceState::Completed | InstanceState::Failed | InstanceState::Cancelled
        ) {
            return inst.state;
        }
    }
    panic!("instance did not reach a terminal state");
}

#[tokio::test]
async fn on_failure_cleanup_runs_when_instance_fails() {
    let storage = storage().await;
    let handlers = Arc::new(registry_with_cleanup());

    let mut seq = mk_sequence(vec![mk_step("work", "boom")]);
    seq.on_failure = Some(vec![mk_step("cleanup", "cleanup")]);
    storage.create_sequence(&seq).await.unwrap();

    let inst = mk_instance_scheduled(seq.id, json!({}));
    storage.create_instance(&inst).await.unwrap();

    let final_state = run_until_terminal(&storage, &handlers, inst.id).await;
    assert_eq!(final_state, InstanceState::Failed);

    // The cleanup step's output proves the hook ran.
    let out = storage
        .get_block_output(inst.id, &BlockId::new("cleanup"))
        .await
        .unwrap();
    assert!(out.is_some(), "on_failure cleanup step should have run");
    assert_eq!(out.unwrap().output["cleaned"], true);
}

#[tokio::test]
async fn no_cleanup_when_instance_succeeds() {
    let storage = storage().await;
    let handlers = Arc::new(registry_with_cleanup());

    // `noop` succeeds, so the instance completes and on_failure must NOT run.
    let mut seq = mk_sequence(vec![mk_step("work", "noop")]);
    seq.on_failure = Some(vec![mk_step("cleanup", "cleanup")]);
    storage.create_sequence(&seq).await.unwrap();

    let inst = mk_instance_scheduled(seq.id, json!({}));
    storage.create_instance(&inst).await.unwrap();

    let final_state = run_until_terminal(&storage, &handlers, inst.id).await;
    assert_eq!(final_state, InstanceState::Completed);

    let out = storage
        .get_block_output(inst.id, &BlockId::new("cleanup"))
        .await
        .unwrap();
    assert!(out.is_none(), "cleanup must not run on success");
}

#[tokio::test]
async fn on_cancel_cleanup_runs_on_signal_cancel() {
    let storage = storage().await;
    let handlers = Arc::new(registry_with_cleanup());

    let mut seq = mk_sequence(vec![mk_step("work", "noop")]);
    seq.on_cancel = Some(vec![mk_step("cleanup", "cleanup")]);
    storage.create_sequence(&seq).await.unwrap();

    // A Paused instance with a pending cancel signal — processed by the
    // scheduler's signalled-instance sweep (immediate, unscoped cancel).
    let inst = mk_instance_in_state(seq.id, InstanceState::Paused);
    storage.create_instance(&inst).await.unwrap();

    let sig = Signal {
        id: uuid::Uuid::now_v7(),
        instance_id: inst.id,
        signal_type: SignalType::Cancel,
        payload: json!({}),
        delivered: false,
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    storage.enqueue_signal(&sig).await.unwrap();

    let sem = semaphore(128);
    let config = default_config();
    let seq_cache = cache();
    let cancel = CancellationToken::new();
    tick_once(&storage, &handlers, &sem, &config, &seq_cache, &cancel)
        .await
        .unwrap();

    let after = storage.get_instance(inst.id).await.unwrap().unwrap();
    assert_eq!(after.state, InstanceState::Cancelled);

    let out = storage
        .get_block_output(inst.id, &BlockId::new("cleanup"))
        .await
        .unwrap();
    assert!(out.is_some(), "on_cancel cleanup step should have run");
}
