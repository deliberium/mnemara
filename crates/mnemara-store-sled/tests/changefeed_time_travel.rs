use mnemara_core::{
    ChangefeedEventKind, ChangefeedRequest, MemoryQualityState, MemoryRecord, MemoryRecordKind,
    MemoryScope, MemoryStore, MemoryTrustLevel, RecallFilters, RecallQuery,
    TimeTravelRecallRequest, UpsertRequest,
};
use mnemara_store_sled::{SledMemoryStore, SledStoreConfig};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn temp_store_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mnemara-sled-{label}-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn scope() -> MemoryScope {
    MemoryScope {
        tenant_id: "tenant-a".to_string(),
        namespace: "ops".to_string(),
        actor_id: "ava".to_string(),
        conversation_id: Some("thread-a".to_string()),
        session_id: Some("session-a".to_string()),
        source: "test".to_string(),
        labels: vec!["changefeed".to_string()],
        trust_level: MemoryTrustLevel::Verified,
    }
}

fn record(content: &str, updated_at_unix_ms: u64) -> MemoryRecord {
    MemoryRecord {
        id: "record-a".to_string(),
        scope: scope(),
        kind: MemoryRecordKind::Fact,
        content: content.to_string(),
        summary: Some(content.to_string()),
        source_id: None,
        metadata: BTreeMap::new(),
        quality_state: MemoryQualityState::Active,
        created_at_unix_ms: 100,
        updated_at_unix_ms,
        expires_at_unix_ms: None,
        importance_score: 0.8,
        artifact: None,
        episode: None,
        historical_state: Default::default(),
        lineage: Vec::new(),
        conflict: None,
    }
}

fn query(text: &str) -> RecallQuery {
    RecallQuery {
        scope: scope(),
        query_text: text.to_string(),
        max_items: 5,
        token_budget: None,
        filters: RecallFilters::default(),
        include_explanation: true,
    }
}

#[tokio::test]
async fn changefeed_lists_mutations_and_time_travel_recall_uses_versions() {
    let store = SledMemoryStore::open(SledStoreConfig::new(temp_store_dir("changefeed"))).unwrap();

    store
        .upsert(UpsertRequest {
            record: record("legacy auth token rotation", 100),
            idempotency_key: Some("record-a-v1".to_string()),
        })
        .await
        .unwrap();
    store
        .upsert(UpsertRequest {
            record: record("current passkey rotation", 200),
            idempotency_key: Some("record-a-v2".to_string()),
        })
        .await
        .unwrap();

    let feed = store
        .changefeed(ChangefeedRequest {
            tenant_id: Some("tenant-a".to_string()),
            namespace: Some("ops".to_string()),
            after_sequence: None,
            limit: Some(10),
        })
        .await
        .unwrap();

    assert_eq!(feed.events.len(), 2);
    assert_eq!(feed.events[0].kind, ChangefeedEventKind::Upserted);
    assert_eq!(feed.events[1].record_id.as_deref(), Some("record-a"));
    assert_eq!(feed.last_sequence, Some(2));
    assert!(!feed.truncated);

    let historical = store
        .recall_as_of(TimeTravelRecallRequest {
            query: query("legacy"),
            as_of_unix_ms: 150,
        })
        .await
        .unwrap();
    assert_eq!(historical.hits.len(), 1);
    assert!(historical.hits[0].record.content.contains("legacy"));

    let current = store.recall(query("passkey")).await.unwrap();
    assert_eq!(current.hits.len(), 1);
    assert!(current.hits[0].record.content.contains("passkey"));
}
