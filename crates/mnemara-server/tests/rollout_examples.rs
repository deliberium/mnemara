#![allow(clippy::field_reassign_with_default)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::{Body, to_bytes};
use axum::http::{Request as HttpRequest, StatusCode};
use mnemara_core::{
    BatchUpsertRequest, EPISODE_SCHEMA_VERSION, EpisodeContext, EpisodeContinuityState,
    EpisodeSalience, LineageLink, LineageRelationKind, MemoryHistoricalState, MemoryQualityState,
    MemoryRecord, MemoryRecordKind, MemoryScope, MemoryStore, MemoryTrustLevel, UpsertRequest,
};
use mnemara_server::{AuthConfig, ServerLimits, http_app};
use mnemara_store_sled::{SledMemoryStore, SledStoreConfig};
use tower::util::ServiceExt;

fn temp_store_dir(label: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("mnemara-rollout-{label}-{nonce}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn scope(source: &str) -> MemoryScope {
    MemoryScope {
        tenant_id: "default".to_string(),
        namespace: "conversation".to_string(),
        actor_id: "ava".to_string(),
        conversation_id: Some("thread-a".to_string()),
        session_id: Some("session-a".to_string()),
        source: source.to_string(),
        labels: vec![],
        trust_level: MemoryTrustLevel::Verified,
    }
}

fn continuity_record(
    id: &str,
    content: &str,
    updated_at_unix_ms: u64,
    previous_record_id: Option<&str>,
    historical_state: MemoryHistoricalState,
) -> MemoryRecord {
    MemoryRecord {
        id: id.to_string(),
        scope: scope("docs-memory"),
        kind: MemoryRecordKind::Task,
        content: content.to_string(),
        summary: Some(content.to_string()),
        source_id: None,
        metadata: BTreeMap::new(),
        quality_state: MemoryQualityState::Active,
        created_at_unix_ms: updated_at_unix_ms,
        updated_at_unix_ms,
        expires_at_unix_ms: None,
        importance_score: 0.8,
        artifact: None,
        episode: Some(EpisodeContext {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: "storm-episode".to_string(),
            summary: Some("storm remediation episode".to_string()),
            continuity_state: EpisodeContinuityState::Open,
            actor_ids: vec!["ava".to_string(), "ops-bot".to_string()],
            goal: Some("close the reconnect storm follow-up list".to_string()),
            outcome: None,
            started_at_unix_ms: Some(1),
            ended_at_unix_ms: None,
            last_active_unix_ms: Some(updated_at_unix_ms),
            previous_record_id: previous_record_id.map(ToString::to_string),
            recurrence_key: None,
            recurrence_interval_ms: None,
            boundary_label: Some("storm-session-handoff".to_string()),
            next_record_id: None,
            causal_record_ids: previous_record_id
                .map(|value| vec![value.to_string()])
                .unwrap_or_default(),
            related_record_ids: vec![],
            linked_artifact_uris: vec![],
            salience: EpisodeSalience {
                reuse_count: 4,
                novelty_score: 0.2,
                goal_relevance: 0.95,
                unresolved_weight: 0.95,
            },
            affective: None,
        }),
        historical_state,
        lineage: Vec::new(),
    }
}

fn policy_record(
    id: &str,
    content: &str,
    quality_state: MemoryQualityState,
    historical_state: MemoryHistoricalState,
    lineage: Vec<LineageLink>,
) -> MemoryRecord {
    MemoryRecord {
        id: id.to_string(),
        scope: scope("docs-memory"),
        kind: MemoryRecordKind::Fact,
        content: content.to_string(),
        summary: Some(content.to_string()),
        source_id: None,
        metadata: BTreeMap::new(),
        quality_state,
        created_at_unix_ms: 2,
        updated_at_unix_ms: 2,
        expires_at_unix_ms: None,
        importance_score: 0.7,
        artifact: None,
        episode: None,
        historical_state,
        lineage,
    }
}

fn stable_policy_notes(value: &serde_json::Value) -> Vec<String> {
    let mut notes = value
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry.as_str())
        .filter(|entry| {
            !entry.starts_with("correlation_id=") && !entry.starts_with("planning_trace_id=")
        })
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    notes.sort();
    notes
}

#[tokio::test(flavor = "current_thread")]
async fn http_continuity_recall_example_matches_golden_subset() {
    let store = Arc::new(
        SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("rollout-continuity"))).unwrap(),
    );
    store
        .batch_upsert(BatchUpsertRequest {
            requests: vec![
                UpsertRequest {
                    record: continuity_record(
                        "incident-mitigation-step",
                        "Mitigation step: raise retry jitter and clear stale sessions.",
                        10,
                        None,
                        MemoryHistoricalState::Historical,
                    ),
                    idempotency_key: Some("incident-mitigation-step".to_string()),
                },
                UpsertRequest {
                    record: continuity_record(
                        "incident-open-followup",
                        "Open follow-up: verify mobile clients honor reconnect backoff after the patch.",
                        20,
                        Some("incident-mitigation-step"),
                        MemoryHistoricalState::Current,
                    ),
                    idempotency_key: Some("incident-open-followup".to_string()),
                },
            ],
        })
        .await
        .unwrap();

    let app = http_app(
        Arc::clone(&store),
        ServerLimits::default(),
        AuthConfig::default(),
    );
    let recall_payload = serde_json::json!({
        "scope": {
            "tenant_id": "default",
            "namespace": "conversation",
            "actor_id": "ava",
            "conversation_id": "thread-a",
            "session_id": "session-a",
            "source": "docs-query",
            "labels": [],
            "trust_level": "Verified"
        },
        "query_text": "what is still unresolved in the storm episode",
        "max_items": 2,
        "token_budget": null,
        "include_explanation": true,
        "filters": {
            "episode_id": "storm-episode",
            "unresolved_only": true,
            "temporal_order": "ChronologicalDesc",
            "historical_mode": "CurrentOnly"
        }
    });

    let response = app
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
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let actual = serde_json::json!({
        "top_record_id": body["hits"][0]["record"]["id"],
        "planning_profile": body["explanation"]["planning_profile"],
        "selected_channels": body["hits"][0]["explanation"]["selected_channels"],
        "stable_policy_notes": stable_policy_notes(&body["explanation"]["policy_notes"]),
        "selected_candidate_stage": body["explanation"]["planning_trace"]["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|candidate| candidate["selected"].as_bool() == Some(true))
            .and_then(|candidate| candidate["planner_stage"].as_str())
            .unwrap(),
    });

    let expected = serde_json::json!({
        "top_record_id": "incident-open-followup",
        "planning_profile": "ContinuityAware",
        "selected_channels": ["episodic", "lexical", "policy", "salience"],
        "stable_policy_notes": [
            "episode_filter_applied",
            "initial_sled_backend_scoring",
            "planning_profile=continuity_aware",
            "policy_profile=general",
            "scoring_profile=balanced",
            "unresolved_only_filter_applied"
        ],
        "selected_candidate_stage": "CandidateGeneration",
    });

    assert_eq!(actual, expected);
}

#[tokio::test(flavor = "current_thread")]
async fn http_historical_lineage_recall_example_matches_user_guide() {
    let store = Arc::new(
        SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("rollout-history"))).unwrap(),
    );
    store
        .batch_upsert(BatchUpsertRequest {
            requests: vec![
                UpsertRequest {
                    record: policy_record(
                        "export-policy-current",
                        "Current export policy: tenant-scoped validation must pass before cross-backend import.",
                        MemoryQualityState::Verified,
                        MemoryHistoricalState::Current,
                        vec![LineageLink {
                            record_id: "export-policy-old".to_string(),
                            relation: LineageRelationKind::Supersedes,
                            confidence: 1.0,
                        }],
                    ),
                    idempotency_key: Some("export-policy-current".to_string()),
                },
                UpsertRequest {
                    record: policy_record(
                        "export-policy-old",
                        "Historical export policy: cross-backend import could proceed before tenant validation completed.",
                        MemoryQualityState::Archived,
                        MemoryHistoricalState::Historical,
                        vec![LineageLink {
                            record_id: "export-policy-current".to_string(),
                            relation: LineageRelationKind::SupersededBy,
                            confidence: 1.0,
                        }],
                    ),
                    idempotency_key: Some("export-policy-old".to_string()),
                },
            ],
        })
        .await
        .unwrap();

    let app = http_app(
        Arc::clone(&store),
        ServerLimits::default(),
        AuthConfig::default(),
    );
    let recall_payload = serde_json::json!({
        "scope": {
            "tenant_id": "default",
            "namespace": "conversation",
            "actor_id": "ava",
            "conversation_id": "thread-a",
            "session_id": "session-a",
            "source": "docs-query",
            "labels": [],
            "trust_level": "Verified"
        },
        "query_text": "previous export validation policy",
        "max_items": 3,
        "token_budget": null,
        "include_explanation": true,
        "filters": {
            "include_archived": true,
            "historical_mode": "HistoricalOnly",
            "lineage_record_id": "export-policy-current"
        }
    });

    let response = app
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
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let actual = serde_json::json!({
        "record_ids": body["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|hit| hit["record"]["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        "historical_states": body["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|hit| hit["record"]["historical_state"].as_str().unwrap())
            .collect::<Vec<_>>(),
        "planning_profile": body["explanation"]["planning_profile"],
    });

    let expected = serde_json::json!({
        "record_ids": ["export-policy-old"],
        "historical_states": ["Historical"],
        "planning_profile": "FastPath",
    });

    assert_eq!(actual, expected);
}
