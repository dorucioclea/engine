//! E2E tests for the queue routing rule API.

use orch8_api::test_harness::spawn_test_server;
use reqwest::StatusCode;
use serde_json::json;

#[tokio::test]
async fn routing_rule_crud_round_trip() {
    let srv = spawn_test_server().await;
    let client = reqwest::Client::new();

    // Create.
    let body = json!({
        "tenant_id": "t1",
        "handler_name": "email_send",
        "queue_override": "priority-email",
        "priority": 10
    });
    let resp = client
        .post(format!("{}/routing-rules", srv.base_url))
        .header("X-Tenant-Id", "t1")
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created: serde_json::Value = resp.json().await.unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["queue_override"], "priority-email");
    assert_eq!(created["enabled"], true);

    // List filtered by handler.
    let resp = client
        .get(format!(
            "{}/routing-rules?handler_name=email_send",
            srv.base_url
        ))
        .header("X-Tenant-Id", "t1")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let rules: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(rules.as_array().unwrap().len(), 1);

    // Get.
    let resp = client
        .get(format!("{}/routing-rules/{id}", srv.base_url))
        .header("X-Tenant-Id", "t1")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Cross-tenant get is denied (404).
    let resp = client
        .get(format!("{}/routing-rules/{id}", srv.base_url))
        .header("X-Tenant-Id", "other")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Delete.
    let resp = client
        .delete(format!("{}/routing-rules/{id}", srv.base_url))
        .header("X-Tenant-Id", "t1")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = client
        .get(format!("{}/routing-rules?tenant_id=t1", srv.base_url))
        .header("X-Tenant-Id", "t1")
        .send()
        .await
        .unwrap();
    let rules: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(rules.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn create_rule_requires_handler_and_queue() {
    let srv = spawn_test_server().await;
    let client = reqwest::Client::new();
    let body = json!({ "tenant_id": "t1", "handler_name": "", "queue_override": "" });
    let resp = client
        .post(format!("{}/routing-rules", srv.base_url))
        .header("X-Tenant-Id", "t1")
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
