use mnemara_core::{
    EPISODE_SCHEMA_VERSION, EpisodeContext, EpisodeContinuityState, EpisodeSalience, ExportRequest,
    ImportMode, ImportRequest, LineageLink, LineageRelationKind, MemoryHistoricalState,
    MemoryQualityState, MemoryRecord, MemoryRecordKind, MemoryScope, MemoryStore, MemoryTrustLevel,
    RecallFilters, RecallQuery, UpsertRequest,
};
use mnemara_store_file::{FileMemoryStore, FileStoreConfig};
use mnemara_store_sled::{SledMemoryStore, SledStoreConfig};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn temp_store_dir(label: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("mnemara-portable-{label}-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn fixture_record() -> MemoryRecord {
    MemoryRecord {
        id: "portable-1".to_string(),
        scope: MemoryScope {
            tenant_id: "default".to_string(),
            namespace: "conversation".to_string(),
            actor_id: "ava".to_string(),
            conversation_id: Some("thread-a".to_string()),
            session_id: Some("session-a".to_string()),
            source: "test".to_string(),
            labels: vec!["portable".to_string()],
            trust_level: MemoryTrustLevel::Verified,
        },
        kind: MemoryRecordKind::Fact,
        content: "Portable export keeps this fact intact.".to_string(),
        summary: Some("Portable fact".to_string()),
        metadata: BTreeMap::new(),
        quality_state: MemoryQualityState::Verified,
        created_at_unix_ms: 1_700_000_000_000,
        updated_at_unix_ms: 1_700_000_000_000,
        expires_at_unix_ms: None,
        importance_score: 0.8,
        source_id: None,
        artifact: None,
        episode: None,
        historical_state: Default::default(),
        lineage: Vec::new(),
    }
}

fn episodic_fixture_record() -> MemoryRecord {
    MemoryRecord {
        id: "portable-episode-1".to_string(),
        scope: MemoryScope {
            tenant_id: "default".to_string(),
            namespace: "conversation".to_string(),
            actor_id: "ava".to_string(),
            conversation_id: Some("thread-a".to_string()),
            session_id: Some("session-a".to_string()),
            source: "test".to_string(),
            labels: vec!["portable".to_string(), "episode".to_string()],
            trust_level: MemoryTrustLevel::Verified,
        },
        kind: MemoryRecordKind::Task,
        content: "Portable export keeps episode lineage intact.".to_string(),
        summary: Some("Portable episodic fact".to_string()),
        metadata: BTreeMap::new(),
        quality_state: MemoryQualityState::Verified,
        created_at_unix_ms: 1_700_000_000_100,
        updated_at_unix_ms: 1_700_000_000_200,
        expires_at_unix_ms: None,
        importance_score: 0.95,
        source_id: Some("portable-episode-source".to_string()),
        artifact: None,
        episode: Some(EpisodeContext {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: "portable-episode".to_string(),
            summary: Some("Portable roundtrip episode".to_string()),
            continuity_state: EpisodeContinuityState::Open,
            actor_ids: vec!["ava".to_string()],
            goal: Some("preserve episodic fields across export/import".to_string()),
            outcome: None,
            started_at_unix_ms: Some(1_700_000_000_000),
            ended_at_unix_ms: None,
            last_active_unix_ms: Some(1_700_000_000_200),
            recurrence_key: None,
            recurrence_interval_ms: None,
            boundary_label: Some("portable-handoff".to_string()),
            previous_record_id: Some("portable-0".to_string()),
            next_record_id: None,
            causal_record_ids: vec!["portable-0".to_string()],
            related_record_ids: vec!["portable-2".to_string()],
            linked_artifact_uris: vec!["file:///tmp/portable.md".to_string()],
            salience: EpisodeSalience {
                reuse_count: 3,
                novelty_score: 0.25,
                goal_relevance: 0.98,
                unresolved_weight: 0.85,
            },
            affective: None,
        }),
        historical_state: MemoryHistoricalState::Historical,
        lineage: vec![LineageLink {
            record_id: "portable-0".to_string(),
            relation: LineageRelationKind::DerivedFrom,
            confidence: 0.85,
        }],
    }
}

#[tokio::test]
async fn portable_export_import_roundtrips_between_file_and_sled() {
    let source_dir = temp_store_dir("source");
    let target_dir = temp_store_dir("target");
    let file_store = FileMemoryStore::open(FileStoreConfig::new(&source_dir)).unwrap();
    let sled_store = SledMemoryStore::open(SledStoreConfig::new(&target_dir)).unwrap();

    file_store
        .upsert(UpsertRequest {
            record: fixture_record(),
            idempotency_key: Some("portable-key".to_string()),
        })
        .await
        .unwrap();

    let package = file_store
        .export(ExportRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
            include_archived: true,
        })
        .await
        .unwrap();

    let report = sled_store
        .import(ImportRequest {
            package,
            mode: ImportMode::Replace,
            dry_run: false,
        })
        .await
        .unwrap();

    assert!(report.applied);
    assert!(report.compatible_package);
    assert_eq!(report.validated_records, 1);
    assert_eq!(report.imported_records, 1);
    assert!(report.replaced_existing);
    assert!(report.failed_records.is_empty());

    let recalled = sled_store
        .recall(RecallQuery {
            scope: fixture_record().scope,
            query_text: "portable export".to_string(),
            max_items: 3,
            token_budget: None,
            filters: RecallFilters::default(),
            include_explanation: true,
        })
        .await
        .unwrap();

    assert_eq!(recalled.hits.len(), 1);
    assert_eq!(recalled.hits[0].record.id, "portable-1");

    fs::remove_dir_all(source_dir).unwrap();
    fs::remove_dir_all(target_dir).unwrap();
}

#[tokio::test]
async fn validate_mode_reports_records_without_mutating_target_store() {
    let source_dir = temp_store_dir("validate-source");
    let target_dir = temp_store_dir("validate-target");
    let file_store = FileMemoryStore::open(FileStoreConfig::new(&source_dir)).unwrap();
    let sled_store = SledMemoryStore::open(SledStoreConfig::new(&target_dir)).unwrap();

    file_store
        .upsert(UpsertRequest {
            record: fixture_record(),
            idempotency_key: Some("portable-key".to_string()),
        })
        .await
        .unwrap();

    let package = file_store
        .export(ExportRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
            include_archived: true,
        })
        .await
        .unwrap();

    let report = sled_store
        .import(ImportRequest {
            package,
            mode: ImportMode::Validate,
            dry_run: false,
        })
        .await
        .unwrap();

    assert!(!report.applied);
    assert!(report.compatible_package);
    assert_eq!(report.validated_records, 1);
    assert_eq!(report.imported_records, 1);
    assert!(report.failed_records.is_empty());

    let recalled = sled_store
        .recall(RecallQuery {
            scope: fixture_record().scope,
            query_text: "portable export".to_string(),
            max_items: 3,
            token_budget: None,
            filters: RecallFilters::default(),
            include_explanation: false,
        })
        .await
        .unwrap();
    assert!(recalled.hits.is_empty());

    fs::remove_dir_all(source_dir).unwrap();
    fs::remove_dir_all(target_dir).unwrap();
}

#[tokio::test]
async fn dry_run_and_version_failures_are_reported_structurally() {
    let source_dir = temp_store_dir("dry-run-source");
    let target_dir = temp_store_dir("dry-run-target");
    let file_store = FileMemoryStore::open(FileStoreConfig::new(&source_dir)).unwrap();
    let sled_store = SledMemoryStore::open(SledStoreConfig::new(&target_dir)).unwrap();

    file_store
        .upsert(UpsertRequest {
            record: fixture_record(),
            idempotency_key: Some("portable-key".to_string()),
        })
        .await
        .unwrap();

    let mut package = file_store
        .export(ExportRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
            include_archived: true,
        })
        .await
        .unwrap();
    package.package_version = 99;
    package.records[0].record.id.clear();

    let report = sled_store
        .import(ImportRequest {
            package,
            mode: ImportMode::Merge,
            dry_run: true,
        })
        .await
        .unwrap();

    assert!(!report.applied);
    assert!(!report.compatible_package);
    assert_eq!(report.validated_records, 0);
    assert_eq!(report.imported_records, 0);
    assert_eq!(report.failed_records.len(), 2);

    let recalled = sled_store
        .recall(RecallQuery {
            scope: fixture_record().scope,
            query_text: "portable export".to_string(),
            max_items: 3,
            token_budget: None,
            filters: RecallFilters::default(),
            include_explanation: false,
        })
        .await
        .unwrap();
    assert!(recalled.hits.is_empty());

    fs::remove_dir_all(source_dir).unwrap();
    fs::remove_dir_all(target_dir).unwrap();
}

#[tokio::test]
async fn portable_roundtrip_preserves_episodic_and_lineage_fields() {
    let source_dir = temp_store_dir("episodic-source");
    let target_dir = temp_store_dir("episodic-target");
    let file_store = FileMemoryStore::open(FileStoreConfig::new(&source_dir)).unwrap();
    let sled_store = SledMemoryStore::open(SledStoreConfig::new(&target_dir)).unwrap();

    file_store
        .upsert(UpsertRequest {
            record: episodic_fixture_record(),
            idempotency_key: Some("portable-episode-key".to_string()),
        })
        .await
        .unwrap();

    let source_result = file_store
        .recall(RecallQuery {
            scope: episodic_fixture_record().scope,
            query_text: "portable export episode lineage".to_string(),
            max_items: 3,
            token_budget: None,
            filters: RecallFilters {
                episode_id: Some("portable-episode".to_string()),
                historical_mode: mnemara_core::RecallHistoricalMode::IncludeHistorical,
                ..RecallFilters::default()
            },
            include_explanation: true,
        })
        .await
        .unwrap();
    assert_eq!(source_result.hits.len(), 1);

    let package = file_store
        .export(ExportRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
            include_archived: true,
        })
        .await
        .unwrap();

    sled_store
        .import(ImportRequest {
            package,
            mode: ImportMode::Replace,
            dry_run: false,
        })
        .await
        .unwrap();

    let recalled = sled_store
        .recall(RecallQuery {
            scope: episodic_fixture_record().scope,
            query_text: "portable export episode lineage".to_string(),
            max_items: 3,
            token_budget: None,
            filters: RecallFilters {
                episode_id: Some("portable-episode".to_string()),
                historical_mode: mnemara_core::RecallHistoricalMode::IncludeHistorical,
                ..RecallFilters::default()
            },
            include_explanation: true,
        })
        .await
        .unwrap();

    assert_eq!(recalled.hits.len(), 1);
    let record = &recalled.hits[0].record;
    assert_eq!(record.historical_state, MemoryHistoricalState::Historical);
    assert_eq!(record.lineage.len(), 1);
    assert_eq!(
        record
            .episode
            .as_ref()
            .map(|episode| episode.schema_version),
        Some(EPISODE_SCHEMA_VERSION)
    );
    assert_eq!(record.lineage[0].relation, LineageRelationKind::DerivedFrom);
    assert_eq!(
        record
            .episode
            .as_ref()
            .map(|episode| episode.episode_id.as_str()),
        Some("portable-episode")
    );
    assert_eq!(
        record
            .episode
            .as_ref()
            .map(|episode| episode.salience.goal_relevance),
        Some(0.98)
    );

    fs::remove_dir_all(source_dir).unwrap();
    fs::remove_dir_all(target_dir).unwrap();
}

#[tokio::test]
async fn portable_import_defaults_missing_additive_episodic_fields() {
    let source_dir = temp_store_dir("episodic-backward-source");
    let target_dir = temp_store_dir("episodic-backward-target");
    let file_store = FileMemoryStore::open(FileStoreConfig::new(&source_dir)).unwrap();
    let sled_store = SledMemoryStore::open(SledStoreConfig::new(&target_dir)).unwrap();

    file_store
        .upsert(UpsertRequest {
            record: episodic_fixture_record(),
            idempotency_key: Some("portable-episode-key".to_string()),
        })
        .await
        .unwrap();

    let package = file_store
        .export(ExportRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
            include_archived: true,
        })
        .await
        .unwrap();

    let mut package_json = serde_json::to_value(&package).unwrap();
    let record = &mut package_json["records"][0]["record"];
    let object = record.as_object_mut().unwrap();
    object.remove("episode");
    object.remove("historical_state");
    object.remove("lineage");

    let downgraded_package = serde_json::from_value(package_json).unwrap();
    let report = sled_store
        .import(ImportRequest {
            package: downgraded_package,
            mode: ImportMode::Replace,
            dry_run: false,
        })
        .await
        .unwrap();

    assert!(report.applied);
    assert!(report.compatible_package);

    let recalled = sled_store
        .recall(RecallQuery {
            scope: episodic_fixture_record().scope,
            query_text: "portable export episode lineage".to_string(),
            max_items: 3,
            token_budget: None,
            filters: RecallFilters::default(),
            include_explanation: true,
        })
        .await
        .unwrap();

    assert_eq!(recalled.hits.len(), 1);
    let record = &recalled.hits[0].record;
    assert!(record.episode.is_none());
    assert_eq!(record.historical_state, MemoryHistoricalState::Current);
    assert!(record.lineage.is_empty());

    fs::remove_dir_all(source_dir).unwrap();
    fs::remove_dir_all(target_dir).unwrap();
}

#[tokio::test]
async fn portable_import_ignores_unknown_future_episodic_fields() {
    let source_dir = temp_store_dir("episodic-forward-source");
    let target_dir = temp_store_dir("episodic-forward-target");
    let file_store = FileMemoryStore::open(FileStoreConfig::new(&source_dir)).unwrap();
    let sled_store = SledMemoryStore::open(SledStoreConfig::new(&target_dir)).unwrap();

    file_store
        .upsert(UpsertRequest {
            record: episodic_fixture_record(),
            idempotency_key: Some("portable-episode-key".to_string()),
        })
        .await
        .unwrap();

    let package = file_store
        .export(ExportRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("conversation".to_string()),
            include_archived: true,
        })
        .await
        .unwrap();

    let mut package_json = serde_json::to_value(&package).unwrap();
    package_json["future_contract_version"] = json!(2);
    package_json["manifest"]["future_scope_hash"] = json!("episode-scope-v2");
    package_json["records"][0]["record"]["future_episode_priority"] = json!(0.77);
    package_json["records"][0]["record"]["episode"]["future_transition_reason"] =
        json!("rollout-checkpoint");

    let future_package = serde_json::from_value(package_json).unwrap();
    let report = sled_store
        .import(ImportRequest {
            package: future_package,
            mode: ImportMode::Replace,
            dry_run: false,
        })
        .await
        .unwrap();

    assert!(report.applied);
    assert!(report.compatible_package);

    let recalled = sled_store
        .recall(RecallQuery {
            scope: episodic_fixture_record().scope,
            query_text: "portable export episode lineage".to_string(),
            max_items: 3,
            token_budget: None,
            filters: RecallFilters {
                episode_id: Some("portable-episode".to_string()),
                historical_mode: mnemara_core::RecallHistoricalMode::IncludeHistorical,
                ..RecallFilters::default()
            },
            include_explanation: true,
        })
        .await
        .unwrap();

    assert_eq!(recalled.hits.len(), 1);
    let record = &recalled.hits[0].record;
    assert_eq!(record.historical_state, MemoryHistoricalState::Historical);
    assert_eq!(record.lineage.len(), 1);
    assert_eq!(
        record
            .episode
            .as_ref()
            .map(|episode| episode.schema_version),
        Some(EPISODE_SCHEMA_VERSION)
    );
    assert_eq!(record.lineage[0].relation, LineageRelationKind::DerivedFrom);
    assert_eq!(
        record
            .episode
            .as_ref()
            .map(|episode| episode.episode_id.as_str()),
        Some("portable-episode")
    );

    fs::remove_dir_all(source_dir).unwrap();
    fs::remove_dir_all(target_dir).unwrap();
}
