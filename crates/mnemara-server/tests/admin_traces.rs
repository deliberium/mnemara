use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use mnemara_core::{
    MemoryQualityState, MemoryRecord, MemoryRecordKind, MemoryScope, MemoryTrustLevel,
};
use mnemara_server::{AuthConfig, ServerLimits, http_app};
use mnemara_store_sled::{SledMemoryStore, SledStoreConfig};
use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tower::ServiceExt;

fn temp_store_dir(label: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("mnemara-server-{label}-{nonce}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn fixture_record() -> MemoryRecord {
    MemoryRecord {
        id: "trace-record".to_string(),
        scope: MemoryScope {
            tenant_id: "default".to_string(),
            namespace: "conversation".to_string(),
            actor_id: "ava".to_string(),
            conversation_id: Some("thread-a".to_string()),
            session_id: Some("session-a".to_string()),
            source: "test".to_string(),
            labels: vec!["trace".to_string()],
            trust_level: MemoryTrustLevel::Verified,
        },
        kind: MemoryRecordKind::Fact,
        content: "Traceable record content".to_string(),
        summary: Some("Traceable record".to_string()),
        metadata: BTreeMap::new(),
        quality_state: MemoryQualityState::Active,
        created_at_unix_ms: 1_700_000_000_000,
        updated_at_unix_ms: 1_700_000_000_000,
        expires_at_unix_ms: None,
        importance_score: 0.75,
        source_id: None,
        artifact: None,
    }
}

#[tokio::test]
async fn admin_trace_endpoints_expose_recent_operations() {
    let dir = temp_store_dir("trace");
    let store = Arc::new(SledMemoryStore::open(SledStoreConfig::new(&dir)).unwrap());
    let app = http_app(
        Arc::clone(&store),
        ServerLimits::default(),
        AuthConfig::default(),
    );

    let upsert_request = Request::builder()
        .method("POST")
        .uri("/memory/upsert")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&mnemara_core::UpsertRequest {
                record: fixture_record(),
                idempotency_key: Some("trace-key".to_string()),
            })
            .unwrap(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let traces_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/traces?operation=Upsert&status=Ok")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(traces_response.status(), StatusCode::OK);
    let traces_body = to_bytes(traces_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let traces: serde_json::Value = serde_json::from_slice(&traces_body).unwrap();
    assert_eq!(traces.as_array().unwrap().len(), 1);
    assert_eq!(traces[0]["operation"], "Upsert");
    assert_eq!(traces[0]["backend"], "sled");
    assert_eq!(traces[0]["admission_class"], "write");

    let runtime_response = app
        .oneshot(
            Request::builder()
                .uri("/admin/runtime")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(runtime_response.status(), StatusCode::OK);
    let runtime_body = to_bytes(runtime_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let runtime: serde_json::Value = serde_json::from_slice(&runtime_body).unwrap();
    assert_eq!(runtime["backend"], "sled");
    assert_eq!(runtime["traces"]["stored_traces"], 1);
    assert_eq!(runtime["admission"]["fair_queue_policy"], "fifo-per-class");

    fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn admin_trace_endpoints_respect_admin_auth() {
    let dir = temp_store_dir("trace-auth");
    let store = Arc::new(SledMemoryStore::open(SledStoreConfig::new(&dir)).unwrap());
    let app = http_app(
        Arc::clone(&store),
        ServerLimits::default(),
        AuthConfig {
            bearer_token: Some("secret-token".to_string()),
            ..AuthConfig::default()
        },
    );

    let unauthorized = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/traces")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let authorized = app
        .oneshot(
            Request::builder()
                .uri("/admin/traces")
                .header("authorization", "Bearer secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);

    fs::remove_dir_all(dir).unwrap();
}
