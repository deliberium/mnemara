#![allow(clippy::field_reassign_with_default)]

use mnemara_core::{
    BatchUpsertRequest, CompactionRequest, DeleteRequest, EPISODE_SCHEMA_VERSION,
    EmbeddingProviderKind, EngineConfig, IngestionPolicy, IntegrityCheckRequest, LineageLink,
    LineageRelationKind, MemoryHistoricalState, MemoryQualityState, MemoryRecord, MemoryRecordKind,
    MemoryScope, MemoryStore, MemoryTrustLevel, RecallFilters, RecallHistoricalMode, RecallQuery,
    RecallScoringProfile, RepairRequest, RetentionPolicy, UpsertRequest,
};
use mnemara_store_sled::{SledMemoryStore, SledStoreConfig};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct ReplayFixtureSet {
    exact_lookup: Vec<FixtureRecord>,
    duplicate_heavy: Vec<FixtureRecord>,
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
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest_dir.ancestors() {
        let candidate = ancestor.join("data/cortex/mnemara-replay-fixtures.json");
        if candidate.is_file() {
            return candidate;
        }
    }

    panic!(
        "fixture file should exist under a parent data/cortex directory of {}",
        manifest_dir.display()
    )
}

fn load_fixtures() -> ReplayFixtureSet {
    let raw = fs::read_to_string(fixture_path()).expect("fixture file should exist");
    serde_json::from_str(&raw).expect("fixture json should decode")
}

fn temp_store_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mnemara-store-sled-{label}-{}",
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
        episode: None,
        historical_state: Default::default(),
        lineage: Vec::new(),
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
        episode: None,
        historical_state: Default::default(),
        lineage: Vec::new(),
    }
}

async fn seed_store(store: &SledMemoryStore, records: &[FixtureRecord]) {
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

fn recent_query_for(scope_thread_id: Option<&str>, max_items: usize) -> RecallQuery {
    RecallQuery {
        scope: MemoryScope {
            tenant_id: "default".to_string(),
            namespace: "conversation".to_string(),
            actor_id: "ava".to_string(),
            conversation_id: scope_thread_id.map(ToString::to_string),
            session_id: Some("session-a".to_string()),
            source: "test".to_string(),
            labels: vec![],
            trust_level: MemoryTrustLevel::Verified,
        },
        query_text: String::new(),
        max_items,
        token_budget: None,
        filters: RecallFilters::default(),
        include_explanation: true,
    }
}

fn episodic_fixture_record() -> MemoryRecord {
    MemoryRecord {
        id: "episodic-sled-repair".to_string(),
        scope: MemoryScope {
            tenant_id: "default".to_string(),
            namespace: "conversation".to_string(),
            actor_id: "ava".to_string(),
            conversation_id: Some("thread-a".to_string()),
            session_id: Some("session-a".to_string()),
            source: "test".to_string(),
            labels: vec!["shared-fixture".to_string(), "episode".to_string()],
            trust_level: MemoryTrustLevel::Verified,
        },
        kind: MemoryRecordKind::Task,
        content: "Restore reconnect rollout after operator restart".to_string(),
        summary: Some("Restart-safe repair fixture".to_string()),
        source_id: Some("episodic-sled-repair".to_string()),
        metadata: BTreeMap::new(),
        quality_state: MemoryQualityState::Active,
        created_at_unix_ms: 10,
        updated_at_unix_ms: 20,
        expires_at_unix_ms: None,
        importance_score: 0.9,
        artifact: None,
        episode: Some(mnemara_core::EpisodeContext {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: "storm-episode".to_string(),
            summary: Some("Storm remediation episode".to_string()),
            continuity_state: mnemara_core::EpisodeContinuityState::Open,
            actor_ids: vec!["ava".to_string()],
            goal: Some("close the reconnect storm follow-up list".to_string()),
            outcome: None,
            started_at_unix_ms: Some(1),
            ended_at_unix_ms: None,
            last_active_unix_ms: Some(20),
            recurrence_key: None,
            recurrence_interval_ms: None,
            boundary_label: Some("incident-handoff".to_string()),
            previous_record_id: Some("incident-root".to_string()),
            next_record_id: None,
            causal_record_ids: vec!["incident-root".to_string()],
            related_record_ids: vec!["incident-postmortem".to_string()],
            linked_artifact_uris: vec!["file:///tmp/storm.md".to_string()],
            salience: mnemara_core::EpisodeSalience {
                reuse_count: 4,
                novelty_score: 0.3,
                goal_relevance: 0.95,
                unresolved_weight: 0.9,
            },
            affective: None,
        }),
        historical_state: MemoryHistoricalState::Historical,
        lineage: vec![LineageLink {
            record_id: "incident-root".to_string(),
            relation: LineageRelationKind::DerivedFrom,
            confidence: 0.8,
        }],
    }
}

fn idempotency_scoped_key(scope: &MemoryScope, key: &str) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        scope.tenant_id,
        scope.namespace,
        scope.actor_id,
        scope.conversation_id.as_deref().unwrap_or(""),
        scope.session_id.as_deref().unwrap_or(""),
        key
    )
}

#[tokio::test(flavor = "current_thread")]
async fn exact_lookup_fixture_retrieves_backend_record() {
    let fixtures = load_fixtures();
    let store = SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("exact"))).unwrap();
    seed_store(&store, &fixtures.exact_lookup).await;

    let result = store.recall(query_for("CORTEX_BACKEND")).await.unwrap();
    assert!(!result.hits.is_empty());
    assert!(
        result.hits[0]
            .record
            .content
            .contains("CORTEX_BACKEND=sled")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn duplicate_heavy_fixture_returns_results_in_ranked_order() {
    let fixtures = load_fixtures();
    let store = SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("duplicate"))).unwrap();
    seed_store(&store, &fixtures.duplicate_heavy).await;

    let result = store
        .recall(query_for("websocket reconnect storm"))
        .await
        .unwrap();
    assert!(result.hits.len() >= 2);
    assert!(result.hits[0].breakdown.total >= result.hits[1].breakdown.total);
}

#[tokio::test(flavor = "current_thread")]
async fn recent_thread_local_fixture_excludes_cross_thread_records() {
    let fixtures = load_fixtures();
    let store = SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("recent"))).unwrap();
    seed_store(&store, &fixtures.recent_thread_local).await;

    let result = store
        .recall(recent_query_for(Some("thread-a"), 3))
        .await
        .unwrap();
    let ids = result
        .hits
        .iter()
        .map(|hit| hit.record.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["recent-3", "recent-1"]);
}

#[tokio::test(flavor = "current_thread")]
async fn durable_capture_fixture_retrieves_summary_lane() {
    let fixtures = load_fixtures();
    let store = SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("durable"))).unwrap();

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
    assert_eq!(result.hits[0].record.kind, MemoryRecordKind::Summary);
    assert!(
        result.hits[0]
            .record
            .summary
            .as_deref()
            .unwrap_or_default()
            .contains("reconnect storm mitigation")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn repair_recovers_missing_idempotency_after_restart_without_losing_lineage() {
    let dir = temp_store_dir("repair-restart");
    let record = episodic_fixture_record();
    let scope = record.scope.clone();

    let store = SledMemoryStore::open(SledStoreConfig::new(&dir)).unwrap();
    store
        .upsert(UpsertRequest {
            record,
            idempotency_key: Some("repair-key".to_string()),
        })
        .await
        .unwrap();
    drop(store);

    let db = sled::open(&dir).unwrap();
    let idempotency_tree = db.open_tree("idempotency").unwrap();
    idempotency_tree
        .remove(idempotency_scoped_key(&scope, "repair-key").as_bytes())
        .unwrap();
    db.flush().unwrap();
    drop(idempotency_tree);
    drop(db);

    let reopened = SledMemoryStore::open(SledStoreConfig::new(&dir)).unwrap();
    let integrity_before = reopened
        .integrity_check(IntegrityCheckRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
        })
        .await
        .unwrap();
    assert!(!integrity_before.healthy);
    assert_eq!(integrity_before.missing_idempotency_keys, 1);

    let dry_run = reopened
        .repair(RepairRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
            dry_run: true,
            reason: "test".to_string(),
            remove_stale_idempotency_keys: false,
            rebuild_missing_idempotency_keys: true,
        })
        .await
        .unwrap();
    assert_eq!(dry_run.rebuilt_missing_idempotency_keys, 1);

    let integrity_after_dry_run = reopened
        .integrity_check(IntegrityCheckRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
        })
        .await
        .unwrap();
    assert_eq!(integrity_after_dry_run.missing_idempotency_keys, 1);

    let repaired = reopened
        .repair(RepairRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
            dry_run: false,
            reason: "test".to_string(),
            remove_stale_idempotency_keys: false,
            rebuild_missing_idempotency_keys: true,
        })
        .await
        .unwrap();
    assert_eq!(repaired.rebuilt_missing_idempotency_keys, 1);
    drop(reopened);

    let final_store = SledMemoryStore::open(SledStoreConfig::new(&dir)).unwrap();
    let integrity_after = final_store
        .integrity_check(IntegrityCheckRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
        })
        .await
        .unwrap();
    assert!(integrity_after.healthy);

    let recalled = final_store
        .recall(RecallQuery {
            scope,
            query_text: "restore reconnect rollout incident root".to_string(),
            max_items: 3,
            token_budget: None,
            filters: RecallFilters {
                lineage_record_id: Some("incident-root".to_string()),
                historical_mode: RecallHistoricalMode::IncludeHistorical,
                ..RecallFilters::default()
            },
            include_explanation: true,
        })
        .await
        .unwrap();

    assert_eq!(recalled.hits.len(), 1);
    let record = &recalled.hits[0].record;
    assert_eq!(record.scope.tenant_id, "default");
    assert_eq!(record.scope.namespace, "conversation");
    assert_eq!(record.historical_state, MemoryHistoricalState::Historical);
    assert_eq!(record.lineage.len(), 1);
    assert_eq!(record.lineage[0].relation, LineageRelationKind::DerivedFrom);
}

#[tokio::test(flavor = "current_thread")]
async fn snapshot_and_repair_preserve_lineage_and_scope() {
    let dir = temp_store_dir("snapshot-repair");
    let record = episodic_fixture_record();
    let scope = record.scope.clone();

    let store = SledMemoryStore::open(SledStoreConfig::new(&dir)).unwrap();
    store
        .upsert(UpsertRequest {
            record,
            idempotency_key: Some("snapshot-repair-key".to_string()),
        })
        .await
        .unwrap();

    let snapshot_before = store.snapshot().await.unwrap();
    assert_eq!(snapshot_before.record_count, 1);
    assert_eq!(snapshot_before.namespaces, vec!["conversation".to_string()]);
    drop(store);

    let db = sled::open(&dir).unwrap();
    let idempotency_tree = db.open_tree("idempotency").unwrap();
    idempotency_tree
        .remove(idempotency_scoped_key(&scope, "snapshot-repair-key").as_bytes())
        .unwrap();
    db.flush().unwrap();
    drop(idempotency_tree);
    drop(db);

    let reopened = SledMemoryStore::open(SledStoreConfig::new(&dir)).unwrap();
    let repaired = reopened
        .repair(RepairRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
            dry_run: false,
            reason: "snapshot-repair-test".to_string(),
            remove_stale_idempotency_keys: false,
            rebuild_missing_idempotency_keys: true,
        })
        .await
        .unwrap();
    assert_eq!(repaired.rebuilt_missing_idempotency_keys, 1);

    let snapshot_after = reopened.snapshot().await.unwrap();
    assert_eq!(snapshot_after.record_count, 1);
    assert_eq!(snapshot_after.namespaces, vec!["conversation".to_string()]);

    let recalled = reopened
        .recall(RecallQuery {
            scope,
            query_text: "restore reconnect rollout incident root".to_string(),
            max_items: 3,
            token_budget: None,
            filters: RecallFilters {
                lineage_record_id: Some("incident-root".to_string()),
                historical_mode: RecallHistoricalMode::IncludeHistorical,
                ..RecallFilters::default()
            },
            include_explanation: true,
        })
        .await
        .unwrap();

    assert_eq!(recalled.hits.len(), 1);
    let record = &recalled.hits[0].record;
    assert_eq!(record.scope.tenant_id, "default");
    assert_eq!(record.scope.namespace, "conversation");
    assert_eq!(record.scope.actor_id, "ava");
    assert_eq!(record.historical_state, MemoryHistoricalState::Historical);
    assert_eq!(record.lineage.len(), 1);
    assert_eq!(record.lineage[0].record_id, "incident-root");
    assert_eq!(record.lineage[0].relation, LineageRelationKind::DerivedFrom);
}

#[tokio::test(flavor = "current_thread")]
async fn repeated_idempotent_upsert_is_safe_under_retry() {
    let fixtures = load_fixtures();
    let store = SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("idempotent"))).unwrap();
    let record = map_fixture_record(&fixtures.exact_lookup[0]);

    let first = store
        .upsert(UpsertRequest {
            record: record.clone(),
            idempotency_key: Some("retry-key".to_string()),
        })
        .await
        .unwrap();
    let second = store
        .upsert(UpsertRequest {
            record,
            idempotency_key: Some("retry-key".to_string()),
        })
        .await
        .unwrap();

    assert!(!first.deduplicated);
    assert!(second.deduplicated);

    let result = store.recall(query_for("CORTEX_BACKEND")).await.unwrap();
    assert_eq!(result.hits.len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn soft_delete_tombstones_record_without_showing_it_in_default_recall() {
    let fixtures = load_fixtures();
    let store = SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("soft-delete"))).unwrap();
    let record = map_fixture_record(&fixtures.exact_lookup[0]);
    let record_id = record.id.clone();

    store
        .upsert(UpsertRequest {
            record,
            idempotency_key: Some("soft-delete".to_string()),
        })
        .await
        .unwrap();

    let receipt = store
        .delete(DeleteRequest {
            tenant_id: "default".to_string(),
            namespace: "conversation".to_string(),
            record_id: record_id.clone(),
            hard_delete: false,
            audit_reason: "test".to_string(),
        })
        .await
        .unwrap();
    assert!(receipt.tombstoned);
    assert!(!receipt.hard_deleted);

    let default_result = store.recall(query_for("CORTEX_BACKEND")).await.unwrap();
    assert!(default_result.hits.is_empty());

    let deleted_result = store
        .recall(RecallQuery {
            filters: RecallFilters {
                states: vec![MemoryQualityState::Deleted],
                ..RecallFilters::default()
            },
            ..query_for("CORTEX_BACKEND")
        })
        .await
        .unwrap();
    assert_eq!(deleted_result.hits.len(), 1);
    assert_eq!(deleted_result.hits[0].record.id, record_id);
}

#[tokio::test(flavor = "current_thread")]
async fn compaction_archives_duplicate_records_and_reports_them() {
    let fixtures = load_fixtures();
    let store = SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("compact"))).unwrap();
    let base = map_fixture_record(&fixtures.duplicate_heavy[0]);
    let mut duplicate = base.clone();
    duplicate.id = "duplicate-copy".to_string();
    duplicate.updated_at_unix_ms += 1;
    duplicate.created_at_unix_ms += 1;

    store
        .batch_upsert(BatchUpsertRequest {
            requests: vec![
                UpsertRequest {
                    record: base,
                    idempotency_key: Some("compact-base".to_string()),
                },
                UpsertRequest {
                    record: duplicate,
                    idempotency_key: Some("compact-duplicate".to_string()),
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

    assert!(report.deduplicated_records >= 1);
    assert_eq!(report.deduplicated_records, report.archived_records);

    let archived = store
        .recall(RecallQuery {
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
            query_text: String::new(),
            max_items: 10,
            token_budget: None,
            filters: RecallFilters {
                states: vec![MemoryQualityState::Archived],
                include_archived: true,
                historical_mode: mnemara_core::RecallHistoricalMode::IncludeHistorical,
                ..RecallFilters::default()
            },
            include_explanation: false,
        })
        .await
        .unwrap();
    assert!(!archived.hits.is_empty());
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

    let store = SledMemoryStore::open(
        SledStoreConfig::new(temp_store_dir("compaction-rollup")).with_engine_config(config),
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
                historical_mode: mnemara_core::RecallHistoricalMode::IncludeHistorical,
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
async fn retention_policy_deletes_expired_records() {
    let fixtures = load_fixtures();
    let mut config = EngineConfig::default();
    config.retention = RetentionPolicy {
        ttl_days: 1,
        archive_after_days: 30,
        max_records_per_namespace: 10,
        pinned_records_exempt: true,
    };

    let store = SledMemoryStore::open(
        SledStoreConfig::new(temp_store_dir("retention-expired")).with_engine_config(config),
    )
    .unwrap();

    let mut expired = map_fixture_record(&fixtures.exact_lookup[0]);
    expired.created_at_unix_ms = 1;
    expired.updated_at_unix_ms = 1;

    store
        .upsert(UpsertRequest {
            record: expired,
            idempotency_key: Some("expired".to_string()),
        })
        .await
        .unwrap();

    let result = store.recall(query_for("CORTEX_BACKEND")).await.unwrap();
    assert!(result.hits.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn retention_policy_archives_old_records_without_deleting_them() {
    let fixtures = load_fixtures();
    let mut config = EngineConfig::default();
    config.retention = RetentionPolicy {
        ttl_days: 50_000,
        archive_after_days: 1,
        max_records_per_namespace: 10,
        pinned_records_exempt: true,
    };

    let store = SledMemoryStore::open(
        SledStoreConfig::new(temp_store_dir("retention-archive")).with_engine_config(config),
    )
    .unwrap();

    let mut old = map_fixture_record(&fixtures.exact_lookup[0]);
    old.created_at_unix_ms = 1;
    old.updated_at_unix_ms = 1;

    store
        .upsert(UpsertRequest {
            record: old,
            idempotency_key: Some("old".to_string()),
        })
        .await
        .unwrap();

    let default_result = store.recall(query_for("CORTEX_BACKEND")).await.unwrap();
    assert!(default_result.hits.is_empty());

    let archived_result = store
        .recall(RecallQuery {
            filters: RecallFilters {
                states: vec![MemoryQualityState::Archived],
                include_archived: true,
                historical_mode: mnemara_core::RecallHistoricalMode::IncludeHistorical,
                ..RecallFilters::default()
            },
            ..query_for("CORTEX_BACKEND")
        })
        .await
        .unwrap();
    assert_eq!(archived_result.hits.len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn retention_policy_caps_active_namespace_records() {
    let fixtures = load_fixtures();
    let mut config = EngineConfig::default();
    config.retention = RetentionPolicy {
        ttl_days: 50_000,
        archive_after_days: 50_000,
        max_records_per_namespace: 1,
        pinned_records_exempt: true,
    };

    let store = SledMemoryStore::open(
        SledStoreConfig::new(temp_store_dir("retention-cap")).with_engine_config(config),
    )
    .unwrap();

    let first = map_fixture_record(&fixtures.duplicate_heavy[0]);
    let second = map_fixture_record(&fixtures.duplicate_heavy[1]);

    store
        .batch_upsert(BatchUpsertRequest {
            requests: vec![
                UpsertRequest {
                    record: first,
                    idempotency_key: Some("cap-1".to_string()),
                },
                UpsertRequest {
                    record: second,
                    idempotency_key: Some("cap-2".to_string()),
                },
            ],
        })
        .await
        .unwrap();

    let active = store
        .recall(RecallQuery {
            query_text: String::new(),
            max_items: 10,
            token_budget: None,
            filters: RecallFilters::default(),
            include_explanation: false,
            ..query_for("")
        })
        .await
        .unwrap();
    assert_eq!(active.hits.len(), 1);

    let archived = store
        .recall(RecallQuery {
            query_text: String::new(),
            max_items: 20,
            token_budget: None,
            filters: RecallFilters {
                states: vec![MemoryQualityState::Archived],
                include_archived: true,
                historical_mode: mnemara_core::RecallHistoricalMode::IncludeHistorical,
                ..RecallFilters::default()
            },
            include_explanation: false,
            ..query_for("")
        })
        .await
        .unwrap();
    assert_eq!(archived.hits.len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn ingestion_policy_can_require_labels_and_idempotency() {
    let fixtures = load_fixtures();
    let mut config = EngineConfig::default();
    config.ingestion = IngestionPolicy {
        idempotent_writes_required: true,
        deduplication_window_hours: 24,
        allow_model_derived_memories: true,
        require_source_labels: true,
    };

    let store = SledMemoryStore::open(
        SledStoreConfig::new(temp_store_dir("ingestion-policy")).with_engine_config(config),
    )
    .unwrap();

    let mut record = map_fixture_record(&fixtures.exact_lookup[0]);
    record.scope.labels.clear();

    let missing_labels = store
        .upsert(UpsertRequest {
            record: record.clone(),
            idempotency_key: Some("labels".to_string()),
        })
        .await;
    assert!(missing_labels.is_err());

    record.scope.labels.push("shared-fixture".to_string());
    let missing_idempotency = store
        .upsert(UpsertRequest {
            record,
            idempotency_key: None,
        })
        .await;
    assert!(missing_idempotency.is_err());
}

#[tokio::test(flavor = "current_thread")]
async fn recall_explanations_include_planning_trace_candidates() {
    let fixtures = load_fixtures();
    let store = SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("trace"))).unwrap();
    seed_store(&store, &fixtures.duplicate_heavy).await;

    let result = store
        .recall(RecallQuery {
            token_budget: Some(16),
            ..query_for("websocket reconnect storm")
        })
        .await
        .unwrap();

    let explanation = result.explanation.expect("expected explanation");
    let planning_trace = explanation.planning_trace.expect("expected planning trace");
    assert!(planning_trace.token_budget_applied);
    assert!(!planning_trace.candidates.is_empty());
    assert!(
        planning_trace
            .candidates
            .iter()
            .any(|candidate| candidate.selected)
    );
    assert!(
        planning_trace
            .candidates
            .iter()
            .all(|candidate| !candidate.record_id.is_empty())
    );
    assert!(
        planning_trace
            .candidates
            .iter()
            .all(|candidate| !candidate.candidate_sources.is_empty())
    );
    assert!(planning_trace.candidates.iter().any(|candidate| matches!(
        candidate.planner_stage,
        mnemara_core::RecallPlannerStage::CandidateGeneration
            | mnemara_core::RecallPlannerStage::GraphExpansion
    )));
}

#[tokio::test(flavor = "current_thread")]
async fn scoring_profile_can_shift_rank_order_toward_importance() {
    let mut lexical_config = EngineConfig::default();
    lexical_config.recall_scoring_profile = RecallScoringProfile::LexicalFirst;
    let lexical_store = SledMemoryStore::open(
        SledStoreConfig::new(temp_store_dir("scoring-lexical")).with_engine_config(lexical_config),
    )
    .unwrap();

    let mut importance_config = EngineConfig::default();
    importance_config.recall_scoring_profile = RecallScoringProfile::ImportanceFirst;
    let importance_store = SledMemoryStore::open(
        SledStoreConfig::new(temp_store_dir("scoring-importance"))
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
        episode: None,
        historical_state: Default::default(),
        lineage: Vec::new(),
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
        episode: None,
        historical_state: Default::default(),
        lineage: Vec::new(),
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

    let store = SledMemoryStore::open(
        SledStoreConfig::new(temp_store_dir("semantic-channel")).with_engine_config(config),
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
        episode: None,
        historical_state: Default::default(),
        lineage: Vec::new(),
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
