use criterion::{Criterion, criterion_group, criterion_main};
use mnemara_core::{
    BatchUpsertRequest, MemoryQualityState, MemoryRecord, MemoryRecordKind, MemoryScope,
    MemoryStore, MemoryTrustLevel, RecallFilters, RecallQuery, UpsertRequest,
};
use mnemara_store_file::{FileMemoryStore, FileStoreConfig};
use mnemara_store_sled::{SledMemoryStore, SledStoreConfig};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use tokio::runtime::Runtime;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct ReplayFixtureSet {
    exact_lookup: Vec<FixtureRecord>,
    duplicate_heavy: Vec<FixtureRecord>,
}

#[derive(Debug, Deserialize)]
struct FixtureRecord {
    id: String,
    timestamp_unix_ms: u64,
    prompt: String,
    answer: String,
    importance_score: f32,
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

fn map_fixture_record(record: &FixtureRecord) -> MemoryRecord {
    MemoryRecord {
        id: record.id.clone(),
        scope: MemoryScope {
            tenant_id: "default".to_string(),
            namespace: "conversation".to_string(),
            actor_id: "ava".to_string(),
            conversation_id: Some("thread-a".to_string()),
            session_id: Some("session-a".to_string()),
            source: "bench".to_string(),
            labels: vec!["benchmark".to_string()],
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

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mnemara-bench-{label}-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn benchmark_backends(c: &mut Criterion) {
    let fixtures = load_fixtures();
    let ingest_requests = fixtures
        .duplicate_heavy
        .iter()
        .chain(fixtures.exact_lookup.iter())
        .map(|record| UpsertRequest {
            record: map_fixture_record(record),
            idempotency_key: Some(record.id.clone()),
        })
        .collect::<Vec<_>>();
    let recall_query = RecallQuery {
        scope: MemoryScope {
            tenant_id: "default".to_string(),
            namespace: "conversation".to_string(),
            actor_id: "ava".to_string(),
            conversation_id: Some("thread-a".to_string()),
            session_id: Some("session-a".to_string()),
            source: "bench".to_string(),
            labels: vec![],
            trust_level: MemoryTrustLevel::Verified,
        },
        query_text: "websocket reconnect storm".to_string(),
        max_items: 3,
        token_budget: None,
        filters: RecallFilters::default(),
        include_explanation: false,
    };

    let runtime = Runtime::new().unwrap();
    let sled_recall_query = recall_query.clone();
    let file_recall_query = recall_query.clone();

    c.bench_function("sled_batch_upsert", |b| {
        b.to_async(&runtime).iter(|| async {
            let store =
                SledMemoryStore::open(SledStoreConfig::new(temp_dir("sled-ingest"))).unwrap();
            store
                .batch_upsert(BatchUpsertRequest {
                    requests: ingest_requests.clone(),
                })
                .await
                .unwrap();
        });
    });

    c.bench_function("file_batch_upsert", |b| {
        b.to_async(&runtime).iter(|| async {
            let store =
                FileMemoryStore::open(FileStoreConfig::new(temp_dir("file-ingest"))).unwrap();
            store
                .batch_upsert(BatchUpsertRequest {
                    requests: ingest_requests.clone(),
                })
                .await
                .unwrap();
        });
    });

    c.bench_function("sled_recall", |b| {
        b.to_async(&runtime).iter_batched(
            || {
                let store =
                    SledMemoryStore::open(SledStoreConfig::new(temp_dir("sled-recall"))).unwrap();
                let requests = ingest_requests.clone();
                let query = sled_recall_query.clone();
                (store, requests, query)
            },
            |(store, requests, query)| async move {
                store
                    .batch_upsert(BatchUpsertRequest { requests })
                    .await
                    .unwrap();
                store.recall(query).await.unwrap();
            },
            criterion::BatchSize::SmallInput,
        );
    });

    c.bench_function("file_recall", |b| {
        b.to_async(&runtime).iter_batched(
            || {
                let store =
                    FileMemoryStore::open(FileStoreConfig::new(temp_dir("file-recall"))).unwrap();
                let requests = ingest_requests.clone();
                let query = file_recall_query.clone();
                (store, requests, query)
            },
            |(store, requests, query)| async move {
                store
                    .batch_upsert(BatchUpsertRequest { requests })
                    .await
                    .unwrap();
                store.recall(query).await.unwrap();
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, benchmark_backends);
criterion_main!(benches);
