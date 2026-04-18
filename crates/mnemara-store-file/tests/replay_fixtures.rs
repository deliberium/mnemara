#![allow(clippy::field_reassign_with_default)]

use mnemara_core::{
    BatchUpsertRequest, CompactionRequest, DeleteRequest, EmbeddingProviderKind, EngineConfig,
    IngestionPolicy, MemoryQualityState, MemoryRecord, MemoryRecordKind, MemoryScope, MemoryStore,
    MemoryTrustLevel, RecallFilters, RecallQuery, RecallScoringProfile, RetentionPolicy,
    UpsertRequest,
};
use mnemara_store_file::{FileMemoryStore, FileStoreConfig};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct ReplayFixtureSet {
    exact_lookup: Vec<FixtureRecord>,
    duplicate_heavy: Vec<FixtureRecord>,
    #[allow(dead_code)]
    recent_thread_local: Vec<FixtureRecord>,
    durable_capture: Vec<DurableCaptureFixture>,
}

#[derive(Debug, Deserialize)]
struct FixtureRecord {
    id: String,
    timestamp_unix_ms: u64,
    prompt: String,
    answer: String,
    importance_score: f32,
    #[serde(default = "default_thread_id")]
    thread_id: String,
    #[serde(default = "default_session_id")]
    session_id: String,
    #[serde(default = "default_source")]
    provenance: String,
}

#[derive(Debug, Deserialize)]
struct DurableCaptureFixture {
    id: String,
    timestamp_unix_ms: u64,
    summary: String,
    importance_score: f32,
    kind: String,
    reason: String,
    #[serde(default = "default_thread_id")]
    thread_id: String,
    #[serde(default = "default_session_id")]
    session_id: String,
    #[serde(default = "default_source")]
    provenance: String,
}

fn default_thread_id() -> String {
    "thread-a".to_string()
}

fn default_session_id() -> String {
    "session-a".to_string()
}

fn default_source() -> String {
    "test".to_string()
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../data/cortex/mnemara-replay-fixtures.json")
}

fn load_fixtures() -> ReplayFixtureSet {
    let raw = fs::read_to_string(fixture_path()).expect("fixture file should exist");
    serde_json::from_str(&raw).expect("fixture json should decode")
}

fn temp_store_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mnemara-store-file-{label}-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn map_fixture_record(record: &FixtureRecord) -> MemoryRecord {
    MemoryRecord {
        id: record.id.clone(),
        scope: MemoryScope {
            tenant_id: "default".to_string(),
            namespace: "conversation".to_string(),
            actor_id: "ava".to_string(),
            conversation_id: Some(record.thread_id.clone()),
            session_id: Some(record.session_id.clone()),
            source: record.provenance.clone(),
            labels: vec!["shared-fixture".to_string()],
            trust_level: MemoryTrustLevel::Verified,
        },
        kind: MemoryRecordKind::Episodic,
        content: format!("Prompt: {}\nAnswer: {}", record.prompt, record.answer),
        summary: Some(record.answer.clone()),
        source_id: None,
        metadata: BTreeMap::new(),
        quality_state: MemoryQualityState::Active,
        created_at_unix_ms: record.timestamp_unix_ms,
        updated_at_unix_ms: record.timestamp_unix_ms,
        expires_at_unix_ms: None,
        importance_score: record.importance_score,
        artifact: None,
    }
}

fn map_durable_capture_fixture(record: &DurableCaptureFixture) -> MemoryRecord {
    let mut metadata = BTreeMap::new();
    metadata.insert("capture_kind".to_string(), record.kind.clone());
    metadata.insert("flush_reason".to_string(), record.reason.clone());

    MemoryRecord {
        id: record.id.clone(),
        scope: MemoryScope {
            tenant_id: "default".to_string(),
            namespace: "conversation".to_string(),
            actor_id: "ava".to_string(),
            conversation_id: Some(record.thread_id.clone()),
            session_id: Some(record.session_id.clone()),
            source: record.provenance.clone(),
            labels: vec!["shared-fixture".to_string(), "durable".to_string()],
            trust_level: MemoryTrustLevel::Verified,
        },
        kind: MemoryRecordKind::Summary,
        content: record.summary.clone(),
        summary: Some(record.summary.clone()),
        source_id: None,
        metadata,
        quality_state: MemoryQualityState::Verified,
        created_at_unix_ms: record.timestamp_unix_ms,
        updated_at_unix_ms: record.timestamp_unix_ms,
        expires_at_unix_ms: None,
        importance_score: record.importance_score,
        artifact: None,
    }
}

async fn seed_store(store: &FileMemoryStore, records: &[FixtureRecord]) {
    let requests = records
        .iter()
        .map(|record| UpsertRequest {
            record: map_fixture_record(record),
            idempotency_key: Some(record.id.clone()),
        })
        .collect::<Vec<_>>();
    store
        .batch_upsert(BatchUpsertRequest { requests })
        .await
        .unwrap();
}

fn query_for(text: &str) -> RecallQuery {
    RecallQuery {
        scope: MemoryScope {
            tenant_id: "default".to_string(),
            namespace: "conversation".to_string(),
            actor_id: "ava".to_string(),
            conversation_id: Some("thread-a".to_string()),
            session_id: Some("session-a".to_string()),
            source: "test".to_string(),
            labels: vec![],
            trust_level: MemoryTrustLevel::Verified,
        },
        query_text: text.to_string(),
        max_items: 3,
        token_budget: None,
        filters: RecallFilters::default(),
        include_explanation: true,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn exact_lookup_fixture_retrieves_backend_record() {
    let fixtures = load_fixtures();
    let store = FileMemoryStore::open(FileStoreConfig::new(temp_store_dir("exact"))).unwrap();
    seed_store(&store, &fixtures.exact_lookup).await;

    let result = store.recall(query_for("CORTEX_BACKEND")).await.unwrap();
    assert!(!result.hits.is_empty());
    assert!(
        result.hits[0]
            .record
            .content
            .contains("CORTEX_BACKEND=sled")
    );
    assert!(
        result
            .explanation
            .as_ref()
            .and_then(|value| value.planning_trace.as_ref())
            .is_some()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn durable_capture_fixture_retrieves_summary_lane() {
    let fixtures = load_fixtures();
    let store = FileMemoryStore::open(FileStoreConfig::new(temp_store_dir("durable"))).unwrap();
    store
        .upsert(UpsertRequest {
            record: map_durable_capture_fixture(&fixtures.durable_capture[0]),
            idempotency_key: Some(fixtures.durable_capture[0].id.clone()),
        })
        .await
        .unwrap();

    let result = store
        .recall(RecallQuery {
            filters: RecallFilters {
                kinds: vec![MemoryRecordKind::Summary],
                ..RecallFilters::default()
            },
            ..query_for("follow-up reconnect storm mitigation")
        })
        .await
        .unwrap();
    assert_eq!(result.hits.len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn retention_and_deletion_are_enforced() {
    let fixtures = load_fixtures();
    let mut config = EngineConfig::default();
    config.retention = RetentionPolicy {
        ttl_days: 1,
        archive_after_days: 50_000,
        max_records_per_namespace: 10,
        pinned_records_exempt: true,
    };
    let store = FileMemoryStore::open(
        FileStoreConfig::new(temp_store_dir("retention")).with_engine_config(config),
    )
    .unwrap();

    let mut expired = map_fixture_record(&fixtures.exact_lookup[0]);
    expired.created_at_unix_ms = 1;
    expired.updated_at_unix_ms = 1;
    let id = expired.id.clone();

    store
        .upsert(UpsertRequest {
            record: expired,
            idempotency_key: Some("expired".to_string()),
        })
        .await
        .unwrap();
    assert!(
        store
            .recall(query_for("CORTEX_BACKEND"))
            .await
            .unwrap()
            .hits
            .is_empty()
    );

    let receipt = store
        .delete(DeleteRequest {
            tenant_id: "default".to_string(),
            namespace: "conversation".to_string(),
            record_id: id,
            hard_delete: false,
            audit_reason: "test".to_string(),
        })
        .await
        .unwrap();
    assert!(!receipt.tombstoned);
}

#[tokio::test(flavor = "current_thread")]
async fn compaction_and_ingestion_policy_work() {
    let fixtures = load_fixtures();
    let mut config = EngineConfig::default();
    config.ingestion = IngestionPolicy {
        idempotent_writes_required: true,
        deduplication_window_hours: 24,
        allow_model_derived_memories: true,
        require_source_labels: true,
    };
    config.retention = RetentionPolicy {
        ttl_days: 50_000,
        archive_after_days: 50_000,
        max_records_per_namespace: 1,
        pinned_records_exempt: true,
    };
    let store = FileMemoryStore::open(
        FileStoreConfig::new(temp_store_dir("policy")).with_engine_config(config),
    )
    .unwrap();

    let first = map_fixture_record(&fixtures.duplicate_heavy[0]);
    let second = map_fixture_record(&fixtures.duplicate_heavy[1]);
    store
        .batch_upsert(BatchUpsertRequest {
            requests: vec![
                UpsertRequest {
                    record: first,
                    idempotency_key: Some("one".to_string()),
                },
                UpsertRequest {
                    record: second,
                    idempotency_key: Some("two".to_string()),
                },
            ],
        })
        .await
        .unwrap();

    let report = store
        .compact(CompactionRequest {
            tenant_id: "default".to_string(),
            namespace: Some("conversation".to_string()),
            dry_run: false,
            reason: "test".to_string(),
        })
        .await
        .unwrap();
    assert!(report.archived_records <= 1);

    let mut unlabeled = map_fixture_record(&fixtures.exact_lookup[0]);
    unlabeled.scope.labels.clear();
    assert!(
        store
            .upsert(UpsertRequest {
                record: unlabeled,
                idempotency_key: Some("bad".to_string()),
            })
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn compaction_can_roll_up_duplicate_clusters_and_cold_archive_stale_records() {
    let fixtures = load_fixtures();
    let mut config = EngineConfig::default();
    config.compaction.summarize_after_record_count = 2;
    config.compaction.cold_archive_after_days = 1;
    config
        .compaction
        .cold_archive_importance_threshold_per_mille = 200;

    let store = FileMemoryStore::open(
        FileStoreConfig::new(temp_store_dir("compaction-rollup")).with_engine_config(config),
    )
    .unwrap();

    let base = map_fixture_record(&fixtures.duplicate_heavy[0]);
    let base_id = base.id.clone();
    let mut duplicate = base.clone();
    duplicate.id = "duplicate-rollup-copy".to_string();
    duplicate.updated_at_unix_ms += 1;
    duplicate.created_at_unix_ms += 1;

    let mut cold = map_fixture_record(&fixtures.exact_lookup[0]);
    cold.id = "cold-record".to_string();
    cold.summary = Some("cold archive candidate".to_string());
    cold.importance_score = 0.1;
    cold.created_at_unix_ms = 1;
    cold.updated_at_unix_ms = 1;

    store
        .batch_upsert(BatchUpsertRequest {
            requests: vec![
                UpsertRequest {
                    record: base,
                    idempotency_key: Some("rollup-base".to_string()),
                },
                UpsertRequest {
                    record: duplicate,
                    idempotency_key: Some("rollup-duplicate".to_string()),
                },
                UpsertRequest {
                    record: cold,
                    idempotency_key: Some("cold-record".to_string()),
                },
            ],
        })
        .await
        .unwrap();

    let report = store
        .compact(CompactionRequest {
            tenant_id: "default".to_string(),
            namespace: Some("conversation".to_string()),
            dry_run: false,
            reason: "phase-4".to_string(),
        })
        .await
        .unwrap();

    assert_eq!(report.summarized_clusters, 1);
    assert!(report.archived_records >= 2);

    let summary = store
        .recall(RecallQuery {
            query_text: String::new(),
            max_items: 10,
            token_budget: None,
            filters: RecallFilters {
                kinds: vec![MemoryRecordKind::Summary],
                ..RecallFilters::default()
            },
            include_explanation: false,
            ..query_for("")
        })
        .await
        .unwrap();
    assert_eq!(summary.hits.len(), 1);
    assert!(
        summary.hits[0]
            .record
            .content
            .contains("Compacted 2 related records")
    );

    let archived = store
        .recall(RecallQuery {
            query_text: String::new(),
            max_items: 20,
            token_budget: None,
            filters: RecallFilters {
                states: vec![MemoryQualityState::Archived],
                include_archived: true,
                ..RecallFilters::default()
            },
            include_explanation: false,
            ..query_for("")
        })
        .await
        .unwrap();
    let archived_ids = archived
        .hits
        .iter()
        .map(|hit| hit.record.id.as_str())
        .collect::<Vec<_>>();
    assert!(archived_ids.contains(&"cold-record"));
    assert!(
        archived_ids.contains(&"duplicate-rollup-copy") || archived_ids.contains(&base_id.as_str())
    );
}

#[tokio::test(flavor = "current_thread")]
async fn scoring_profile_can_shift_rank_order_toward_importance() {
    let mut lexical_config = EngineConfig::default();
    lexical_config.recall_scoring_profile = RecallScoringProfile::LexicalFirst;
    let lexical_store = FileMemoryStore::open(
        FileStoreConfig::new(temp_store_dir("scoring-lexical")).with_engine_config(lexical_config),
    )
    .unwrap();

    let mut importance_config = EngineConfig::default();
    importance_config.recall_scoring_profile = RecallScoringProfile::ImportanceFirst;
    let importance_store = FileMemoryStore::open(
        FileStoreConfig::new(temp_store_dir("scoring-importance"))
            .with_engine_config(importance_config),
    )
    .unwrap();

    let lexical_favorite = MemoryRecord {
        id: "lexical-favorite".to_string(),
        scope: query_for("").scope,
        kind: MemoryRecordKind::Fact,
        content: "storm mitigation storm checklist".to_string(),
        summary: Some("storm checklist".to_string()),
        source_id: None,
        metadata: BTreeMap::new(),
        quality_state: MemoryQualityState::Active,
        created_at_unix_ms: 10,
        updated_at_unix_ms: 10,
        expires_at_unix_ms: None,
        importance_score: 0.1,
        artifact: None,
    };
    let importance_favorite = MemoryRecord {
        id: "importance-favorite".to_string(),
        scope: query_for("").scope,
        kind: MemoryRecordKind::Fact,
        content: "storm memo".to_string(),
        summary: Some("storm memo".to_string()),
        source_id: None,
        metadata: BTreeMap::new(),
        quality_state: MemoryQualityState::Verified,
        created_at_unix_ms: 11,
        updated_at_unix_ms: 11,
        expires_at_unix_ms: None,
        importance_score: 0.95,
        artifact: None,
    };

    for store in [&lexical_store, &importance_store] {
        store
            .batch_upsert(BatchUpsertRequest {
                requests: vec![
                    UpsertRequest {
                        record: lexical_favorite.clone(),
                        idempotency_key: Some("lexical-favorite".to_string()),
                    },
                    UpsertRequest {
                        record: importance_favorite.clone(),
                        idempotency_key: Some("importance-favorite".to_string()),
                    },
                ],
            })
            .await
            .unwrap();
    }

    let lexical_result = lexical_store
        .recall(query_for("storm checklist"))
        .await
        .unwrap();
    let importance_result = importance_store
        .recall(query_for("storm checklist"))
        .await
        .unwrap();

    assert_eq!(lexical_result.hits[0].record.id, "lexical-favorite");
    assert_eq!(importance_result.hits[0].record.id, "importance-favorite");
}

#[tokio::test(flavor = "current_thread")]
async fn semantic_embedding_can_surface_semantic_channel_in_explanations() {
    let mut config = EngineConfig::default();
    config.embedding_provider_kind = EmbeddingProviderKind::DeterministicLocal;
    config.embedding_dimensions = 64;

    let store = FileMemoryStore::open(
        FileStoreConfig::new(temp_store_dir("semantic-channel")).with_engine_config(config),
    )
    .unwrap();

    let semantic_match = MemoryRecord {
        id: "semantic-match".to_string(),
        scope: query_for("").scope,
        kind: MemoryRecordKind::Fact,
        content: "storm checklist mitigation runbook".to_string(),
        summary: Some("storm mitigation runbook".to_string()),
        source_id: None,
        metadata: BTreeMap::new(),
        quality_state: MemoryQualityState::Active,
        created_at_unix_ms: 10,
        updated_at_unix_ms: 10,
        expires_at_unix_ms: None,
        importance_score: 0.3,
        artifact: None,
    };

    store
        .upsert(UpsertRequest {
            record: semantic_match,
            idempotency_key: Some("semantic-match".to_string()),
        })
        .await
        .unwrap();

    let result = store
        .recall(query_for("verified storm checklist"))
        .await
        .unwrap();

    assert!(!result.hits.is_empty());
    assert!(result.hits[0].breakdown.semantic > 0.0);
    assert!(result.hits[0].explanation.as_ref().is_some_and(|value| {
        value
            .selected_channels
            .iter()
            .any(|channel| channel == "semantic")
    }));
    assert!(result.explanation.as_ref().is_some_and(|value| {
        value
            .policy_notes
            .iter()
            .any(|note| note == "embedding_provider=deterministic_local")
    }));
}
