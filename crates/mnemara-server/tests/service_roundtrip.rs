#![allow(clippy::field_reassign_with_default)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::{Body, to_bytes};
use axum::http::{Request as HttpRequest, StatusCode};
use mnemara_core::{EmbeddingProviderKind as CoreEmbeddingProviderKind, EngineConfig};
use mnemara_protocol::v1::memory_service_server::MemoryService;
use mnemara_protocol::v1::{
    ArtifactPointer, BatchUpsertMemoryRecordsRequest, CompactRequest, DeleteRequest,
    EmbeddingProviderKind, IntegrityCheckRequest, MemoryRecord, MemoryScope, RecallFilters,
    RecallRequest, RecallScorerKind, RecallScoringProfile, RepairRequest, SnapshotRequest,
    StoreStatsRequest, UpsertMemoryRecordRequest,
};
use mnemara_server::{
    AuthConfig, AuthPermission, GrpcMemoryService, ServerLimits, ServerMetrics, TokenPolicy,
    http_app, http_app_with_metrics,
};
use mnemara_store_sled::{SledMemoryStore, SledStoreConfig};
use tonic::Code;
use tonic::Request;
use tower::util::ServiceExt;

fn temp_store_dir(label: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("mnemara-server-{label}-{nonce}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn test_scope() -> MemoryScope {
    MemoryScope {
        tenant_id: "default".to_string(),
        namespace: "conversation".to_string(),
        actor_id: "ava".to_string(),
        conversation_id: Some("thread-a".to_string()),
        session_id: Some("session-a".to_string()),
        source: "test".to_string(),
        labels: vec!["shared-fixture".to_string()],
        trust_level: "verified".to_string(),
    }
}

fn idempotency_scope_key(scope: &MemoryScope, key: &str) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        scope.tenant_id,
        scope.namespace,
        scope.actor_id,
        scope.conversation_id.as_deref().unwrap_or(""),
        scope.session_id.as_deref().unwrap_or(""),
        key,
    )
}

#[tokio::test(flavor = "current_thread")]
async fn upsert_recall_snapshot_and_compact_round_trip() {
    let store =
        Arc::new(SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("grpc"))).unwrap());
    let service = GrpcMemoryService::new(Arc::clone(&store));

    let upsert_reply = service
        .upsert_memory_record(Request::new(UpsertMemoryRecordRequest {
            record: Some(MemoryRecord {
                id: "record-1".to_string(),
                scope: Some(test_scope()),
                kind: "episodic".to_string(),
                content: "Prompt: How do I set CORTEX_BACKEND?\nAnswer: Set CORTEX_BACKEND=sled"
                    .to_string(),
                summary: Some("Set CORTEX_BACKEND=sled".to_string()),
                metadata: HashMap::from([(String::from("source"), String::from("test"))]),
                quality_state: "active".to_string(),
                created_at_unix_ms: 1,
                updated_at_unix_ms: 1,
                expires_at_unix_ms: None,
                importance_score: 0.9,
                source_id: Some("interaction-1".to_string()),
                artifact: Some(ArtifactPointer {
                    uri: "file:///tmp/record-1.md".to_string(),
                    media_type: Some("text/markdown".to_string()),
                    checksum: Some("abc123".to_string()),
                }),
            }),
            idempotency_key: Some("record-1".to_string()),
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(upsert_reply.record_id, "record-1");

    let recall_reply = service
        .recall(Request::new(RecallRequest {
            scope: Some(test_scope()),
            query_text: "CORTEX_BACKEND".to_string(),
            max_items: 3,
            token_budget: None,
            include_explanation: true,
            filters: Some(RecallFilters {
                kinds: vec!["episodic".to_string()],
                required_labels: vec![],
                source: Some("test".to_string()),
                from_unix_ms: None,
                to_unix_ms: None,
                min_importance_score: Some(0.5),
                trust_levels: vec!["verified".to_string()],
                states: vec![],
                include_archived: false,
            }),
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(recall_reply.hits.len(), 1);
    assert_eq!(recall_reply.hits[0].record.as_ref().unwrap().id, "record-1");
    assert_eq!(
        recall_reply.hits[0]
            .record
            .as_ref()
            .unwrap()
            .source_id
            .as_deref(),
        Some("interaction-1")
    );
    assert!(recall_reply.explanation.is_some());
    assert_eq!(
        recall_reply.hits[0]
            .explanation
            .as_ref()
            .map(|value| value.scorer_kind),
        Some(RecallScorerKind::Profile as i32)
    );
    assert_eq!(
        recall_reply
            .explanation
            .as_ref()
            .map(|value| value.scoring_profile),
        Some(RecallScoringProfile::Balanced as i32)
    );

    let snapshot_reply = service
        .snapshot(Request::new(SnapshotRequest {}))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(snapshot_reply.record_count, 1);
    assert!(
        snapshot_reply
            .namespaces
            .iter()
            .any(|value| value == "conversation")
    );
    assert_eq!(
        snapshot_reply
            .engine
            .as_ref()
            .map(|value| value.recall_scorer_kind),
        Some(RecallScorerKind::Profile as i32)
    );
    assert_eq!(
        snapshot_reply
            .engine
            .as_ref()
            .map(|value| value.embedding_provider_kind),
        Some(EmbeddingProviderKind::Disabled as i32)
    );

    let compact_reply = service
        .compact(Request::new(CompactRequest {
            tenant_id: "default".to_string(),
            namespace: Some("conversation".to_string()),
            dry_run: true,
            reason: "test".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(compact_reply.dry_run);
}

#[tokio::test(flavor = "current_thread")]
async fn batch_upsert_and_delete_round_trip() {
    let store = Arc::new(
        SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("grpc-batch"))).unwrap(),
    );
    let service = GrpcMemoryService::new(Arc::clone(&store));

    let batch_reply = service
        .batch_upsert_memory_records(Request::new(BatchUpsertMemoryRecordsRequest {
            requests: vec![
                UpsertMemoryRecordRequest {
                    record: Some(MemoryRecord {
                        id: "record-a".to_string(),
                        scope: Some(test_scope()),
                        kind: "fact".to_string(),
                        content: "The preferred backend is sled".to_string(),
                        summary: Some("preferred backend is sled".to_string()),
                        metadata: HashMap::new(),
                        quality_state: "verified".to_string(),
                        created_at_unix_ms: 1,
                        updated_at_unix_ms: 1,
                        expires_at_unix_ms: None,
                        importance_score: 0.8,
                        source_id: Some("fact-1".to_string()),
                        artifact: None,
                    }),
                    idempotency_key: Some("record-a".to_string()),
                },
                UpsertMemoryRecordRequest {
                    record: Some(MemoryRecord {
                        id: "record-b".to_string(),
                        scope: Some(test_scope()),
                        kind: "fact".to_string(),
                        content: "The transport is gRPC".to_string(),
                        summary: Some("transport is gRPC".to_string()),
                        metadata: HashMap::new(),
                        quality_state: "active".to_string(),
                        created_at_unix_ms: 2,
                        updated_at_unix_ms: 2,
                        expires_at_unix_ms: None,
                        importance_score: 0.7,
                        source_id: None,
                        artifact: None,
                    }),
                    idempotency_key: Some("record-b".to_string()),
                },
            ],
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(batch_reply.receipts.len(), 2);

    let delete_reply = service
        .delete(Request::new(DeleteRequest {
            tenant_id: "default".to_string(),
            namespace: "conversation".to_string(),
            record_id: "record-a".to_string(),
            hard_delete: false,
            audit_reason: "test".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(delete_reply.tombstoned);

    let recall_reply = service
        .recall(Request::new(RecallRequest {
            scope: Some(test_scope()),
            query_text: "backend".to_string(),
            max_items: 10,
            token_budget: None,
            include_explanation: false,
            filters: Some(RecallFilters {
                kinds: vec!["fact".to_string()],
                required_labels: vec![],
                source: None,
                from_unix_ms: None,
                to_unix_ms: None,
                min_importance_score: None,
                trust_levels: vec![],
                states: vec![],
                include_archived: false,
            }),
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(recall_reply.hits.len(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn stats_integrity_and_repair_round_trip() {
    let dir = temp_store_dir("grpc-stats-repair");
    let scope = test_scope();

    {
        let store = Arc::new(SledMemoryStore::open(SledStoreConfig::new(&dir)).unwrap());
        let service = GrpcMemoryService::new(Arc::clone(&store));

        service
            .upsert_memory_record(Request::new(UpsertMemoryRecordRequest {
                record: Some(MemoryRecord {
                    id: "repair-record".to_string(),
                    scope: Some(scope.clone()),
                    kind: "episodic".to_string(),
                    content: "Prompt: repair\nAnswer: integrity".to_string(),
                    summary: Some("repair integrity".to_string()),
                    metadata: HashMap::new(),
                    quality_state: "active".to_string(),
                    created_at_unix_ms: 1,
                    updated_at_unix_ms: 1,
                    expires_at_unix_ms: None,
                    importance_score: 0.6,
                    source_id: None,
                    artifact: None,
                }),
                idempotency_key: Some("repair-key".to_string()),
            }))
            .await
            .unwrap();
    }

    let db = sled::open(&dir).unwrap();
    let idempotency_tree = db.open_tree("idempotency").unwrap();
    idempotency_tree
        .remove(idempotency_scope_key(&scope, "repair-key").as_bytes())
        .unwrap();
    db.flush().unwrap();
    drop(idempotency_tree);
    drop(db);

    let store = Arc::new(SledMemoryStore::open(SledStoreConfig::new(&dir)).unwrap());
    let service = GrpcMemoryService::new(Arc::clone(&store));

    let stats = service
        .stats(Request::new(StoreStatsRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(stats.total_records, 1);
    assert_eq!(stats.namespaces.len(), 1);
    assert_eq!(
        stats
            .engine
            .as_ref()
            .map(|value| value.recall_scoring_profile),
        Some(RecallScoringProfile::Balanced as i32)
    );

    let integrity_before = service
        .integrity_check(Request::new(IntegrityCheckRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(!integrity_before.healthy);
    assert_eq!(integrity_before.missing_idempotency_keys, 1);

    let repair = service
        .repair(Request::new(RepairRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
            dry_run: false,
            reason: "test".to_string(),
            remove_stale_idempotency_keys: false,
            rebuild_missing_idempotency_keys: true,
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(repair.rebuilt_missing_idempotency_keys, 1);
    assert!(repair.healthy_after);

    let integrity_after = service
        .integrity_check(Request::new(IntegrityCheckRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(integrity_after.healthy);
    assert_eq!(integrity_after.missing_idempotency_keys, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn http_health_snapshot_and_compact_routes_round_trip() {
    let store =
        Arc::new(SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("http"))).unwrap());
    let service = GrpcMemoryService::new(Arc::clone(&store));

    service
        .upsert_memory_record(Request::new(UpsertMemoryRecordRequest {
            record: Some(MemoryRecord {
                id: "record-http".to_string(),
                scope: Some(test_scope()),
                kind: "episodic".to_string(),
                content: "Prompt: healthz\nAnswer: snapshot route".to_string(),
                summary: Some("snapshot route".to_string()),
                metadata: HashMap::new(),
                quality_state: "active".to_string(),
                created_at_unix_ms: 1,
                updated_at_unix_ms: 1,
                expires_at_unix_ms: None,
                importance_score: 0.5,
                source_id: None,
                artifact: None,
            }),
            idempotency_key: Some("record-http".to_string()),
        }))
        .await
        .unwrap();

    let app = http_app(store, ServerLimits::default(), AuthConfig::default());

    let health = app
        .clone()
        .oneshot(
            HttpRequest::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);

    let ready = app
        .clone()
        .oneshot(
            HttpRequest::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);

    let snapshot = app
        .clone()
        .oneshot(
            HttpRequest::builder()
                .uri("/admin/snapshot")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(snapshot.status(), StatusCode::OK);
    let snapshot_body = to_bytes(snapshot.into_body(), usize::MAX).await.unwrap();
    let snapshot_body: serde_json::Value = serde_json::from_slice(&snapshot_body).unwrap();
    assert_eq!(snapshot_body["engine"]["recall_scorer_kind"], "Profile");
    assert_eq!(
        snapshot_body["engine"]["embedding_provider_kind"],
        "Disabled"
    );

    let compact = app
        .clone()
        .oneshot(
            HttpRequest::builder()
                .method("POST")
                .uri("/admin/compact")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"tenant_id":"default","namespace":"conversation","dry_run":true,"reason":"test"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(compact.status(), StatusCode::OK);

    let delete = app
        .oneshot(
            HttpRequest::builder()
                .method("POST")
                .uri("/admin/delete")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"tenant_id":"default","namespace":"conversation","record_id":"record-http","hard_delete":false,"audit_reason":"test"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete.status(), StatusCode::OK);
}

#[tokio::test(flavor = "current_thread")]
async fn http_memory_stats_integrity_and_repair_routes_round_trip() {
    let dir = temp_store_dir("http-stats-repair");
    let scope = test_scope();
    let payload = serde_json::json!({
        "record": {
            "id": "http-repair-record",
            "scope": {
                "tenant_id": scope.tenant_id,
                "namespace": scope.namespace,
                "actor_id": scope.actor_id,
                "conversation_id": scope.conversation_id,
                "session_id": scope.session_id,
                "source": scope.source,
                "labels": scope.labels,
                "trust_level": "Verified"
            },
            "kind": "Episodic",
            "content": "Prompt: repair\nAnswer: over http",
            "summary": "repair over http",
            "metadata": {},
            "quality_state": "Active",
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 1,
            "expires_at_unix_ms": null,
            "importance_score": 0.4,
            "source_id": null,
            "artifact": null
        },
        "idempotency_key": "http-repair-key"
    });

    {
        let store = Arc::new(SledMemoryStore::open(SledStoreConfig::new(&dir)).unwrap());
        let app = http_app(store, ServerLimits::default(), AuthConfig::default());

        let upsert = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri("/memory/upsert")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(upsert.status(), StatusCode::OK);

        let recall_payload = serde_json::json!({
            "scope": {
                "tenant_id": scope.tenant_id,
                "namespace": scope.namespace,
                "actor_id": scope.actor_id,
                "conversation_id": scope.conversation_id,
                "session_id": scope.session_id,
                "source": scope.source,
                "labels": scope.labels,
                "trust_level": "Verified"
            },
            "query_text": "repair",
            "max_items": 4,
            "token_budget": null,
            "include_explanation": false,
            "filters": {
                "kinds": [],
                "required_labels": [],
                "source": null,
                "from_unix_ms": null,
                "to_unix_ms": null,
                "min_importance_score": null,
                "trust_levels": [],
                "states": [],
                "include_archived": false
            }
        });
        let recall = app
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri("/memory/recall")
                    .header("content-type", "application/json")
                    .body(Body::from(recall_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(recall.status(), StatusCode::OK);
        let recall_body = to_bytes(recall.into_body(), usize::MAX).await.unwrap();
        let recall_body: serde_json::Value = serde_json::from_slice(&recall_body).unwrap();
        assert_eq!(recall_body["hits"].as_array().unwrap().len(), 1);
    }

    let db = sled::open(&dir).unwrap();
    let idempotency_tree = db.open_tree("idempotency").unwrap();
    idempotency_tree
        .remove(idempotency_scope_key(&scope, "http-repair-key").as_bytes())
        .unwrap();
    db.flush().unwrap();
    drop(idempotency_tree);
    drop(db);

    let store = Arc::new(SledMemoryStore::open(SledStoreConfig::new(&dir)).unwrap());
    let app = http_app(store, ServerLimits::default(), AuthConfig::default());

    let stats = app
        .clone()
        .oneshot(
            HttpRequest::builder()
                .method("GET")
                .uri("/admin/stats?tenant_id=default&namespace=conversation")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stats.status(), StatusCode::OK);
    let stats_body = to_bytes(stats.into_body(), usize::MAX).await.unwrap();
    let stats_body: serde_json::Value = serde_json::from_slice(&stats_body).unwrap();
    assert_eq!(stats_body["total_records"], 1);
    assert_eq!(stats_body["engine"]["recall_scoring_profile"], "Balanced");
    assert_eq!(stats_body["engine"]["embedding_provider_kind"], "Disabled");

    let integrity = app
        .clone()
        .oneshot(
            HttpRequest::builder()
                .method("GET")
                .uri("/admin/integrity?tenant_id=default&namespace=conversation")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(integrity.status(), StatusCode::OK);
    let integrity_body = to_bytes(integrity.into_body(), usize::MAX).await.unwrap();
    let integrity_body: serde_json::Value = serde_json::from_slice(&integrity_body).unwrap();
    assert_eq!(integrity_body["healthy"], false);
    assert_eq!(integrity_body["missing_idempotency_keys"], 1);

    let repair_payload = serde_json::json!({
        "tenant_id": "default",
        "namespace": "conversation",
        "dry_run": false,
        "reason": "http-test",
        "remove_stale_idempotency_keys": false,
        "rebuild_missing_idempotency_keys": true
    });
    let repair = app
        .clone()
        .oneshot(
            HttpRequest::builder()
                .method("POST")
                .uri("/admin/repair")
                .header("content-type", "application/json")
                .body(Body::from(repair_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repair.status(), StatusCode::OK);
    let repair_body = to_bytes(repair.into_body(), usize::MAX).await.unwrap();
    let repair_body: serde_json::Value = serde_json::from_slice(&repair_body).unwrap();
    assert_eq!(repair_body["rebuilt_missing_idempotency_keys"], 1);
    assert_eq!(repair_body["healthy_after"], true);

    let integrity_after = app
        .oneshot(
            HttpRequest::builder()
                .method("GET")
                .uri("/admin/integrity?tenant_id=default&namespace=conversation")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(integrity_after.status(), StatusCode::OK);
    let integrity_after_body = to_bytes(integrity_after.into_body(), usize::MAX)
        .await
        .unwrap();
    let integrity_after_body: serde_json::Value =
        serde_json::from_slice(&integrity_after_body).unwrap();
    assert_eq!(integrity_after_body["healthy"], true);
    assert_eq!(integrity_after_body["missing_idempotency_keys"], 0);
}

#[tokio::test(flavor = "current_thread")]
async fn http_recall_exposes_semantic_channel_when_embeddings_are_enabled() {
    let mut config = EngineConfig::default();
    config.embedding_provider_kind = CoreEmbeddingProviderKind::DeterministicLocal;
    config.embedding_dimensions = 64;

    let store = Arc::new(
        SledMemoryStore::open(
            SledStoreConfig::new(temp_store_dir("http-semantic")).with_engine_config(config),
        )
        .unwrap(),
    );
    let app = http_app(store, ServerLimits::default(), AuthConfig::default());

    let upsert_payload = serde_json::json!({
        "record": {
            "id": "semantic-http-record",
            "scope": {
                "tenant_id": "default",
                "namespace": "conversation",
                "actor_id": "ava",
                "conversation_id": "thread-a",
                "session_id": "session-a",
                "source": "http-semantic-test",
                "labels": ["shared-fixture"],
                "trust_level": "Verified"
            },
            "kind": "Fact",
            "content": "storm checklist mitigation runbook",
            "summary": "storm mitigation runbook",
            "metadata": {},
            "quality_state": "Active",
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 1,
            "expires_at_unix_ms": null,
            "importance_score": 0.3,
            "source_id": null,
            "artifact": null
        },
        "idempotency_key": "semantic-http-record"
    });

    let upsert = app
        .clone()
        .oneshot(
            HttpRequest::builder()
                .method("POST")
                .uri("/memory/upsert")
                .header("content-type", "application/json")
                .body(Body::from(upsert_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(upsert.status(), StatusCode::OK);

    let recall_payload = serde_json::json!({
        "scope": {
            "tenant_id": "default",
            "namespace": "conversation",
            "actor_id": "ava",
            "conversation_id": "thread-a",
            "session_id": "session-a",
            "source": "http-semantic-test",
            "labels": [],
            "trust_level": "Verified"
        },
        "query_text": "verified storm checklist",
        "max_items": 4,
        "token_budget": null,
        "include_explanation": true,
        "filters": {
            "kinds": [],
            "required_labels": [],
            "source": null,
            "from_unix_ms": null,
            "to_unix_ms": null,
            "min_importance_score": null,
            "trust_levels": [],
            "states": [],
            "include_archived": false
        }
    });

    let recall = app
        .clone()
        .oneshot(
            HttpRequest::builder()
                .method("POST")
                .uri("/memory/recall")
                .header("content-type", "application/json")
                .body(Body::from(recall_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(recall.status(), StatusCode::OK);
    let recall_body = to_bytes(recall.into_body(), usize::MAX).await.unwrap();
    let recall_body: serde_json::Value = serde_json::from_slice(&recall_body).unwrap();
    assert_eq!(recall_body["hits"].as_array().unwrap().len(), 1);
    assert!(
        recall_body["hits"][0]["breakdown"]["semantic"]
            .as_f64()
            .is_some_and(|value| value > 0.0)
    );
    assert!(
        recall_body["hits"][0]["explanation"]["selected_channels"]
            .as_array()
            .is_some_and(|channels| channels.iter().any(|value| value == "semantic"))
    );
    assert!(
        recall_body["explanation"]["policy_notes"]
            .as_array()
            .is_some_and(|notes| notes
                .iter()
                .any(|value| value == "embedding_provider=deterministic_local"))
    );

    let snapshot = app
        .clone()
        .oneshot(
            HttpRequest::builder()
                .uri("/admin/snapshot")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(snapshot.status(), StatusCode::OK);
    let snapshot_body = to_bytes(snapshot.into_body(), usize::MAX).await.unwrap();
    let snapshot_body: serde_json::Value = serde_json::from_slice(&snapshot_body).unwrap();
    assert_eq!(
        snapshot_body["engine"]["embedding_provider_kind"],
        "DeterministicLocal"
    );

    let stats = app
        .oneshot(
            HttpRequest::builder()
                .method("GET")
                .uri("/admin/stats?tenant_id=default&namespace=conversation")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stats.status(), StatusCode::OK);
    let stats_body = to_bytes(stats.into_body(), usize::MAX).await.unwrap();
    let stats_body: serde_json::Value = serde_json::from_slice(&stats_body).unwrap();
    assert_eq!(
        stats_body["engine"]["embedding_provider_kind"],
        "DeterministicLocal"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn grpc_recall_exposes_semantic_channel_when_embeddings_are_enabled() {
    let mut config = EngineConfig::default();
    config.embedding_provider_kind = CoreEmbeddingProviderKind::DeterministicLocal;
    config.embedding_dimensions = 64;

    let store = Arc::new(
        SledMemoryStore::open(
            SledStoreConfig::new(temp_store_dir("grpc-semantic")).with_engine_config(config),
        )
        .unwrap(),
    );
    let service = GrpcMemoryService::new(Arc::clone(&store));

    service
        .upsert_memory_record(Request::new(UpsertMemoryRecordRequest {
            record: Some(MemoryRecord {
                id: "semantic-grpc-record".to_string(),
                scope: Some(MemoryScope {
                    tenant_id: "default".to_string(),
                    namespace: "conversation".to_string(),
                    actor_id: "ava".to_string(),
                    conversation_id: Some("thread-a".to_string()),
                    session_id: Some("session-a".to_string()),
                    source: "grpc-semantic-test".to_string(),
                    labels: vec!["shared-fixture".to_string()],
                    trust_level: "verified".to_string(),
                }),
                kind: "fact".to_string(),
                content: "storm checklist mitigation runbook".to_string(),
                summary: Some("storm mitigation runbook".to_string()),
                metadata: HashMap::new(),
                quality_state: "active".to_string(),
                created_at_unix_ms: 1,
                updated_at_unix_ms: 1,
                expires_at_unix_ms: None,
                importance_score: 0.3,
                source_id: None,
                artifact: None,
            }),
            idempotency_key: Some("semantic-grpc-record".to_string()),
        }))
        .await
        .unwrap();

    let recall = service
        .recall(Request::new(RecallRequest {
            scope: Some(MemoryScope {
                tenant_id: "default".to_string(),
                namespace: "conversation".to_string(),
                actor_id: "ava".to_string(),
                conversation_id: Some("thread-a".to_string()),
                session_id: Some("session-a".to_string()),
                source: "grpc-semantic-test".to_string(),
                labels: vec![],
                trust_level: "verified".to_string(),
            }),
            query_text: "verified storm checklist".to_string(),
            max_items: 4,
            token_budget: None,
            include_explanation: true,
            filters: Some(RecallFilters {
                kinds: vec![],
                required_labels: vec![],
                source: None,
                from_unix_ms: None,
                to_unix_ms: None,
                min_importance_score: None,
                trust_levels: vec![],
                states: vec![],
                include_archived: false,
            }),
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(recall.hits.len(), 1);
    assert!(
        recall.hits[0]
            .breakdown
            .as_ref()
            .is_some_and(|value| value.semantic > 0.0)
    );
    assert!(recall.hits[0].explanation.as_ref().is_some_and(|value| {
        value
            .selected_channels
            .iter()
            .any(|channel| channel == "semantic")
    }));
    assert!(recall.explanation.as_ref().is_some_and(|value| {
        value
            .policy_notes
            .iter()
            .any(|note| note == "embedding_provider=deterministic_local")
    }));

    let snapshot = service
        .snapshot(Request::new(SnapshotRequest {}))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        snapshot
            .engine
            .as_ref()
            .map(|value| value.embedding_provider_kind),
        Some(EmbeddingProviderKind::DeterministicLocal as i32)
    );

    let stats = service
        .stats(Request::new(StoreStatsRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        stats
            .engine
            .as_ref()
            .map(|value| value.embedding_provider_kind),
        Some(EmbeddingProviderKind::DeterministicLocal as i32)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn grpc_limits_reject_oversized_recall_and_batch_requests() {
    let store = Arc::new(
        SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("grpc-limits"))).unwrap(),
    );
    let limits = ServerLimits {
        max_http_body_bytes: 1024,
        max_batch_upsert_requests: 1,
        max_recall_items: 2,
        max_query_text_bytes: 8,
        max_record_content_bytes: 32,
        max_labels_per_scope: 2,
        ..ServerLimits::default()
    };
    let service = GrpcMemoryService::with_limits(Arc::clone(&store), limits);

    let oversized_recall = service
        .recall(Request::new(RecallRequest {
            scope: Some(test_scope()),
            query_text: "this-query-is-too-long".to_string(),
            max_items: 3,
            token_budget: None,
            include_explanation: false,
            filters: None,
        }))
        .await
        .unwrap_err();
    assert_eq!(oversized_recall.code(), Code::InvalidArgument);

    let oversized_batch = service
        .batch_upsert_memory_records(Request::new(BatchUpsertMemoryRecordsRequest {
            requests: vec![
                UpsertMemoryRecordRequest {
                    record: Some(MemoryRecord {
                        id: "record-1".to_string(),
                        scope: Some(test_scope()),
                        kind: "fact".to_string(),
                        content: "short content".to_string(),
                        summary: None,
                        metadata: HashMap::new(),
                        quality_state: "active".to_string(),
                        created_at_unix_ms: 1,
                        updated_at_unix_ms: 1,
                        expires_at_unix_ms: None,
                        importance_score: 0.1,
                        source_id: None,
                        artifact: None,
                    }),
                    idempotency_key: Some("record-1".to_string()),
                },
                UpsertMemoryRecordRequest {
                    record: Some(MemoryRecord {
                        id: "record-2".to_string(),
                        scope: Some(test_scope()),
                        kind: "fact".to_string(),
                        content: "short content".to_string(),
                        summary: None,
                        metadata: HashMap::new(),
                        quality_state: "active".to_string(),
                        created_at_unix_ms: 1,
                        updated_at_unix_ms: 1,
                        expires_at_unix_ms: None,
                        importance_score: 0.1,
                        source_id: None,
                        artifact: None,
                    }),
                    idempotency_key: Some("record-2".to_string()),
                },
            ],
        }))
        .await
        .unwrap_err();
    assert_eq!(oversized_batch.code(), Code::InvalidArgument);
}

#[tokio::test(flavor = "current_thread")]
async fn http_limits_reject_oversized_request_bodies() {
    let store = Arc::new(
        SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("http-limits"))).unwrap(),
    );
    let app = http_app(
        store,
        ServerLimits {
            max_http_body_bytes: 32,
            ..ServerLimits::default()
        },
        AuthConfig::default(),
    );
    let oversized_body = r#"{"tenant_id":"default","namespace":"conversation","dry_run":true,"reason":"this-payload-is-intentionally-larger-than-thirty-two-bytes"}"#;

    let compact = app
        .oneshot(
            HttpRequest::builder()
                .method("POST")
                .uri("/admin/compact")
                .header("content-type", "application/json")
                .header("content-length", oversized_body.len().to_string())
                .body(Body::from(oversized_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(compact.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test(flavor = "current_thread")]
async fn metrics_endpoint_reports_http_and_grpc_activity() {
    let store =
        Arc::new(SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("metrics"))).unwrap());
    let metrics = Arc::new(ServerMetrics::default());
    let service = GrpcMemoryService::with_observability(
        Arc::clone(&store),
        ServerLimits::default(),
        Arc::clone(&metrics),
    );

    service
        .upsert_memory_record(Request::new(UpsertMemoryRecordRequest {
            record: Some(MemoryRecord {
                id: "metrics-record".to_string(),
                scope: Some(test_scope()),
                kind: "episodic".to_string(),
                content: "Prompt: metrics\nAnswer: record".to_string(),
                summary: Some("metrics record".to_string()),
                metadata: HashMap::new(),
                quality_state: "active".to_string(),
                created_at_unix_ms: 1,
                updated_at_unix_ms: 1,
                expires_at_unix_ms: None,
                importance_score: 0.5,
                source_id: None,
                artifact: None,
            }),
            idempotency_key: Some("metrics-record".to_string()),
        }))
        .await
        .unwrap();

    let app = http_app_with_metrics(
        store,
        ServerLimits::default(),
        Arc::clone(&metrics),
        AuthConfig::default(),
    );

    let metrics_response = app
        .oneshot(
            HttpRequest::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(metrics_response.status(), StatusCode::OK);
    let body = to_bytes(metrics_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();

    assert!(body.contains("mnemara_grpc_upsert_requests_started_total 1"));
    assert!(body.contains("mnemara_grpc_upsert_requests_ok_total 1"));
    assert!(body.contains("mnemara_http_metrics_requests_total 1"));
}

#[tokio::test(flavor = "current_thread")]
async fn recall_reply_exposes_planning_trace_payload() {
    let store = Arc::new(
        SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("planning-trace"))).unwrap(),
    );
    let service = GrpcMemoryService::new(Arc::clone(&store));

    service
        .batch_upsert_memory_records(Request::new(BatchUpsertMemoryRecordsRequest {
            requests: vec![
                UpsertMemoryRecordRequest {
                    record: Some(MemoryRecord {
                        id: "trace-a".to_string(),
                        scope: Some(test_scope()),
                        kind: "episodic".to_string(),
                        content: "Prompt: websocket reconnect storm\nAnswer: mitigation checklist"
                            .to_string(),
                        summary: Some("mitigation checklist".to_string()),
                        metadata: HashMap::new(),
                        quality_state: "active".to_string(),
                        created_at_unix_ms: 1,
                        updated_at_unix_ms: 1,
                        expires_at_unix_ms: None,
                        importance_score: 0.8,
                        source_id: None,
                        artifact: None,
                    }),
                    idempotency_key: Some("trace-a".to_string()),
                },
                UpsertMemoryRecordRequest {
                    record: Some(MemoryRecord {
                        id: "trace-b".to_string(),
                        scope: Some(test_scope()),
                        kind: "episodic".to_string(),
                        content: "Prompt: websocket reconnect storm\nAnswer: retry tuning"
                            .to_string(),
                        summary: Some("retry tuning".to_string()),
                        metadata: HashMap::new(),
                        quality_state: "active".to_string(),
                        created_at_unix_ms: 2,
                        updated_at_unix_ms: 2,
                        expires_at_unix_ms: None,
                        importance_score: 0.7,
                        source_id: None,
                        artifact: None,
                    }),
                    idempotency_key: Some("trace-b".to_string()),
                },
            ],
        }))
        .await
        .unwrap();

    let recall = service
        .recall(Request::new(RecallRequest {
            scope: Some(test_scope()),
            query_text: "websocket reconnect storm".to_string(),
            max_items: 1,
            token_budget: Some(6),
            include_explanation: true,
            filters: None,
        }))
        .await
        .unwrap()
        .into_inner();

    let explanation = recall.explanation.expect("expected top-level explanation");
    let planning_trace = explanation
        .planning_trace
        .expect("expected planning trace payload");
    assert!(planning_trace.token_budget_applied);
    assert!(planning_trace.candidates.len() >= 2);
    assert!(
        planning_trace
            .candidates
            .iter()
            .any(|candidate| candidate.selected)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn auth_guards_reject_missing_bearer_tokens() {
    let store =
        Arc::new(SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("auth"))).unwrap());
    let auth = AuthConfig {
        bearer_token: Some("secret-token".to_string()),
        protect_metrics: true,
        token_policies: vec![],
    };
    let service = GrpcMemoryService::with_runtime_config(
        Arc::clone(&store),
        ServerLimits::default(),
        Arc::new(ServerMetrics::default()),
        auth.clone(),
    );

    let grpc_error = service
        .snapshot(Request::new(SnapshotRequest {}))
        .await
        .unwrap_err();
    assert_eq!(grpc_error.code(), Code::Unauthenticated);

    let app = http_app(store, ServerLimits::default(), auth);
    let http_response = app
        .oneshot(
            HttpRequest::builder()
                .uri("/admin/snapshot")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(http_response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "current_thread")]
async fn role_based_auth_policies_enforce_read_write_admin_and_metrics_permissions() {
    let store =
        Arc::new(SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("authz"))).unwrap());
    let auth = AuthConfig {
        bearer_token: None,
        protect_metrics: true,
        token_policies: vec![
            TokenPolicy {
                token: "read-token".to_string(),
                permissions: vec![AuthPermission::Read],
            },
            TokenPolicy {
                token: "write-token".to_string(),
                permissions: vec![AuthPermission::Write],
            },
            TokenPolicy {
                token: "admin-token".to_string(),
                permissions: vec![AuthPermission::Admin],
            },
            TokenPolicy {
                token: "metrics-token".to_string(),
                permissions: vec![AuthPermission::Metrics],
            },
        ],
    };
    let service = GrpcMemoryService::with_runtime_config(
        Arc::clone(&store),
        ServerLimits::default(),
        Arc::new(ServerMetrics::default()),
        auth.clone(),
    );

    let write_request = Request::new(UpsertMemoryRecordRequest {
        record: Some(MemoryRecord {
            id: "role-record".to_string(),
            scope: Some(test_scope()),
            kind: "episodic".to_string(),
            content: "Prompt: role auth\nAnswer: test".to_string(),
            summary: Some("role auth test".to_string()),
            metadata: HashMap::new(),
            quality_state: "active".to_string(),
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
            expires_at_unix_ms: None,
            importance_score: 0.5,
            source_id: None,
            artifact: None,
        }),
        idempotency_key: Some("role-record".to_string()),
    });
    let mut write_request = write_request;
    write_request
        .metadata_mut()
        .insert("authorization", "Bearer write-token".parse().unwrap());
    service.upsert_memory_record(write_request).await.unwrap();

    let read_request = Request::new(RecallRequest {
        scope: Some(test_scope()),
        query_text: "role auth".to_string(),
        max_items: 1,
        token_budget: None,
        include_explanation: false,
        filters: None,
    });
    let mut read_request = read_request;
    read_request
        .metadata_mut()
        .insert("authorization", "Bearer read-token".parse().unwrap());
    let recall = service.recall(read_request).await.unwrap().into_inner();
    assert_eq!(recall.hits.len(), 1);

    let snapshot_request = Request::new(SnapshotRequest {});
    let mut snapshot_request = snapshot_request;
    snapshot_request
        .metadata_mut()
        .insert("authorization", "Bearer admin-token".parse().unwrap());
    service.snapshot(snapshot_request).await.unwrap();

    let unauthorized_write = service
        .upsert_memory_record({
            let mut request = Request::new(UpsertMemoryRecordRequest {
                record: Some(MemoryRecord {
                    id: "role-record-2".to_string(),
                    scope: Some(test_scope()),
                    kind: "episodic".to_string(),
                    content: "Prompt: denied\nAnswer: write".to_string(),
                    summary: Some("denied write".to_string()),
                    metadata: HashMap::new(),
                    quality_state: "active".to_string(),
                    created_at_unix_ms: 2,
                    updated_at_unix_ms: 2,
                    expires_at_unix_ms: None,
                    importance_score: 0.4,
                    source_id: None,
                    artifact: None,
                }),
                idempotency_key: Some("role-record-2".to_string()),
            });
            request
                .metadata_mut()
                .insert("authorization", "Bearer read-token".parse().unwrap());
            request
        })
        .await
        .unwrap_err();
    assert_eq!(unauthorized_write.code(), Code::Unauthenticated);

    let app = http_app(store, ServerLimits::default(), auth);

    let metrics_response = app
        .clone()
        .oneshot(
            HttpRequest::builder()
                .uri("/metrics")
                .header("authorization", "Bearer metrics-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(metrics_response.status(), StatusCode::OK);

    let denied_snapshot = app
        .oneshot(
            HttpRequest::builder()
                .uri("/admin/snapshot")
                .header("authorization", "Bearer metrics-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied_snapshot.status(), StatusCode::UNAUTHORIZED);
}
