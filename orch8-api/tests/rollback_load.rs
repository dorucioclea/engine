//! Load test for error budget / auto-rollback calculation.
//!
//! Verifies that rollback checks perform correctly under high-volume telemetry
//! ingestion and that hysteresis (cooldown + confirmation window) works as expected.

use orch8_api::test_harness::spawn_test_server;
use orch8_storage::AdminStore;

#[tokio::test]
async fn rollback_high_volume_telemetry_ingestion() {
    let server = spawn_test_server().await;
    let client = reqwest::Client::new();
    let seq = "high-volume-seq";

    // Create a policy: 10% error rate threshold, 5-minute window.
    let resp = client
        .post(format!("{}/rollback-policies", server.base_url))
        .json(&serde_json::json!({
            "sequence_name": seq,
            "error_rate_threshold": 0.1,
            "time_window_secs": 300
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Ingest 100 success events in batches of 50.
    for _ in 0..2 {
        let events: Vec<serde_json::Value> = (0..50)
            .map(|i| {
                serde_json::json!({
                    "event_type": "InstanceCompleted",
                    "payload": serde_json::json!({"sequence_name": seq, "i": i}).to_string(),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "device": {
                        "device_id": format!("device-{i}"),
                        "os_name": "iOS",
                        "os_version": "17",
                        "app_version": "1.0",
                        "sdk_version": "0.1"
                    }
                })
            })
            .collect();
        let resp = client
            .post(format!("{}/telemetry/mobile", server.base_url))
            .json(&serde_json::json!({ "events": events }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 202);
    }

    // Ingest 5 errors (5% error rate) — below threshold, no rollback.
    for i in 0..5 {
        let resp = client
            .post(format!("{}/telemetry/mobile/errors", server.base_url))
            .json(&serde_json::json!({
                "error_type": "RuntimeError",
                "message": format!("error {i}"),
                "device": {
                    "device_id": format!("device-err-{i}"),
                    "os_name": "iOS",
                    "os_version": "17",
                    "app_version": "1.0",
                    "sdk_version": "0.1"
                },
                "sequence_name": seq
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 202);
    }

    // Verify no rollback triggered (5% < 10% threshold).
    let history = server
        .storage
        .list_rollback_history(None, None, 100)
        .await
        .unwrap();
    assert!(
        history.is_empty(),
        "rollback should NOT trigger at 5% error rate (threshold 10%)"
    );
}

#[tokio::test]
async fn rollback_cooldown_prevents_flapping() {
    let server = spawn_test_server().await;
    let client = reqwest::Client::new();
    let seq = "flappy-seq";

    // Create policy: 0% threshold (any error triggers), 1-hour cooldown.
    let resp = client
        .post(format!("{}/rollback-policies", server.base_url))
        .json(&serde_json::json!({
            "sequence_name": seq,
            "error_rate_threshold": 0.0,
            "time_window_secs": 3600
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // First error — should trigger rollback.
    let resp = client
        .post(format!("{}/telemetry/mobile/errors", server.base_url))
        .json(&serde_json::json!({
            "error_type": "RuntimeError",
            "message": "first error",
            "device": {
                "device_id": "d1",
                "os_name": "iOS",
                "os_version": "17",
                "app_version": "1.0",
                "sdk_version": "0.1"
            },
            "sequence_name": seq
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);

    let history = server
        .storage
        .list_rollback_history(None, None, 100)
        .await
        .unwrap();
    assert_eq!(history.len(), 1, "first error should trigger rollback");

    // Second error immediately after — should NOT trigger due to cooldown.
    let resp = client
        .post(format!("{}/telemetry/mobile/errors", server.base_url))
        .json(&serde_json::json!({
            "error_type": "RuntimeError",
            "message": "second error",
            "device": {
                "device_id": "d2",
                "os_name": "iOS",
                "os_version": "17",
                "app_version": "1.0",
                "sdk_version": "0.1"
            },
            "sequence_name": seq
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);

    let history = server
        .storage
        .list_rollback_history(None, None, 100)
        .await
        .unwrap();
    assert_eq!(
        history.len(),
        1,
        "second error within cooldown should NOT trigger another rollback"
    );
}

#[tokio::test]
async fn rollback_batch_stress() {
    let server = spawn_test_server().await;
    let client = reqwest::Client::new();
    let seq = "stress-seq";

    // Create policy: 50% threshold.
    let resp = client
        .post(format!("{}/rollback-policies", server.base_url))
        .json(&serde_json::json!({
            "sequence_name": seq,
            "error_rate_threshold": 0.5,
            "time_window_secs": 3600
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Ingest max batch (500 events).
    let events: Vec<serde_json::Value> = (0..500)
        .map(|i| {
            serde_json::json!({
                "event_type": "InstanceCompleted",
                "payload": serde_json::json!({"sequence_name": seq}).to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "device": {
                    "device_id": format!("d-{i}"),
                    "os_name": "Android",
                    "os_version": "14",
                    "app_version": "2.0",
                    "sdk_version": "0.1"
                }
            })
        })
        .collect();
    let resp = client
        .post(format!("{}/telemetry/mobile", server.base_url))
        .json(&serde_json::json!({ "events": events }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["accepted"], 500);

    // Ingest errors to bring rate above 50%.
    for i in 0..600 {
        let resp = client
            .post(format!("{}/telemetry/mobile/errors", server.base_url))
            .json(&serde_json::json!({
                "error_type": "RuntimeError",
                "message": format!("stress error {i}"),
                "device": {
                    "device_id": format!("d-err-{i}"),
                    "os_name": "Android",
                    "os_version": "14",
                    "app_version": "2.0",
                    "sdk_version": "0.1"
                },
                "sequence_name": seq
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 202);
    }

    // Rollback should have been triggered once (cooldown prevents multiple).
    let history = server
        .storage
        .list_rollback_history(None, None, 100)
        .await
        .unwrap();
    assert_eq!(
        history.len(),
        1,
        "exactly one rollback should trigger despite 600 errors (cooldown)"
    );
}
