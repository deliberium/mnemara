use mnemara_core::{
    ExportRequest, ImportMode, ImportRequest, MemoryQualityState, MemoryRecord, MemoryRecordKind,
    MemoryScope, MemoryStore, MemoryTrustLevel, RecallFilters, RecallQuery, UpsertRequest,
};
use mnemara_store_file::{FileMemoryStore, FileStoreConfig};
use mnemara_store_sled::{SledMemoryStore, SledStoreConfig};
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
