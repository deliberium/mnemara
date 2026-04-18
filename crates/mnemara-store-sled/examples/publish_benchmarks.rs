#![allow(clippy::field_reassign_with_default)]

use mnemara_core::{
    BatchUpsertRequest, CompactionRequest, EmbeddingProviderKind, EngineConfig, ExportRequest,
    ImportMode, ImportRequest, MemoryQualityState, MemoryRecord, MemoryRecordKind, MemoryScope,
    MemoryStore, MemoryTrustLevel, RecallFilters, RecallQuery, RecallScorerKind,
    RecallScoringProfile, StoreStatsRequest, UpsertRequest, evaluate_rankings_at_k,
};
use mnemara_store_file::{FileMemoryStore, FileStoreConfig};
use mnemara_store_sled::{SledMemoryStore, SledStoreConfig};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const UPSERT_RUNS: usize = 6;
const RECALL_LOOPS: usize = 6;
const ADMIN_RUNS: usize = 4;

#[derive(Debug, Deserialize)]
struct Corpus {
    records: Vec<CorpusRecord>,
    cases: Vec<CorpusCase>,
}

#[derive(Debug, Deserialize)]
struct CorpusRecord {
    id: String,
    scenario: String,
    kind: String,
    content: String,
    summary: String,
    importance_score: f32,
    source: String,
    labels: Vec<String>,
    trust_level: String,
    quality_state: String,
    created_at_unix_ms: u64,
    updated_at_unix_ms: u64,
    #[serde(default)]
    metadata: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct CorpusCase {
    #[serde(rename = "name")]
    _name: String,
    scenario: String,
    query_text: String,
    relevant_record_ids: Vec<String>,
    #[serde(default = "default_max_items")]
    max_items: usize,
    #[serde(default)]
    include_archived: bool,
}

#[derive(Debug, Clone)]
struct PreparedCorpus {
    requests: Vec<UpsertRequest>,
    cases: Vec<PreparedCase>,
    export_request: ExportRequest,
}

#[derive(Debug, Clone)]
struct PreparedCase {
    scenario: String,
    relevant_record_ids: Vec<String>,
    query: RecallQuery,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    report_version: u32,
    generated_at_unix_ms: u64,
    corpus_path: String,
    environment: BenchmarkEnvironment,
    measurement: MeasurementConfig,
    profiles: Vec<ProfileBenchmark>,
}

#[derive(Debug, Serialize)]
struct BenchmarkEnvironment {
    os: String,
    arch: String,
    logical_cpus: usize,
}

#[derive(Debug, Serialize)]
struct MeasurementConfig {
    upsert_runs: usize,
    recall_loops: usize,
    admin_runs: usize,
}

#[derive(Debug, Serialize)]
struct ProfileBenchmark {
    scorer_kind: RecallScorerKind,
    scoring_profile: RecallScoringProfile,
    backend_results: Vec<BackendBenchmark>,
}

#[derive(Debug, Serialize)]
struct BackendBenchmark {
    backend: String,
    quality_overall: ScenarioMetrics,
    quality_by_scenario: Vec<ScenarioBenchmark>,
    ingest: DurationSummary,
    recall: DurationSummary,
    admin_operations: AdminOperationSummary,
    exported_storage_bytes: u64,
}

#[derive(Debug, Serialize)]
struct ScenarioBenchmark {
    scenario: String,
    metrics: ScenarioMetrics,
}

#[derive(Debug, Serialize)]
struct ScenarioMetrics {
    cases: usize,
    hit_rate_at_3: f32,
    recall_at_3: f32,
    mrr: f32,
    ndcg_at_3: f32,
}

#[derive(Debug, Serialize)]
struct DurationSummary {
    samples: usize,
    mean_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
    throughput_per_sec: Option<f64>,
}

#[derive(Debug, Serialize)]
struct AdminOperationSummary {
    snapshot: DurationSummary,
    stats: DurationSummary,
    export: DurationSummary,
    compact_dry_run: DurationSummary,
    import_replace: DurationSummary,
}

fn default_max_items() -> usize {
    5
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/evaluation/ranking-corpus-v1.json")
}

fn temp_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("mnemara-bench-{label}-{}", Uuid::new_v4()));
    fs::create_dir_all(&path).expect("temp dir should be created");
    path
}

fn parse_args() -> (PathBuf, PathBuf) {
    let mut output = PathBuf::from("docs/benchmark-artifacts/benchmark-report-v1.json");
    let mut summary = PathBuf::from("docs/benchmark-artifacts/benchmark-report-v1.md");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => {
                if let Some(path) = args.next() {
                    output = PathBuf::from(path);
                }
            }
            "--summary" => {
                if let Some(path) = args.next() {
                    summary = PathBuf::from(path);
                }
            }
            _ => {}
        }
    }
    (output, summary)
}

fn load_corpus() -> PreparedCorpus {
    let corpus: Corpus =
        serde_json::from_str(&fs::read_to_string(corpus_path()).expect("corpus file should exist"))
            .expect("corpus should decode");

    let requests = corpus
        .records
        .into_iter()
        .map(|record| {
            let mut metadata = record.metadata;
            metadata.insert("scenario".to_string(), record.scenario);
            let trust_level = match record.trust_level.as_str() {
                "pinned" => MemoryTrustLevel::Pinned,
                "observed" => MemoryTrustLevel::Observed,
                _ => MemoryTrustLevel::Verified,
            };
            let quality_state = match record.quality_state.as_str() {
                "active" => MemoryQualityState::Active,
                "archived" => MemoryQualityState::Archived,
                "draft" => MemoryQualityState::Draft,
                _ => MemoryQualityState::Verified,
            };
            let kind = match record.kind.as_str() {
                "summary" => MemoryRecordKind::Summary,
                "task" => MemoryRecordKind::Task,
                _ => MemoryRecordKind::Fact,
            };
            UpsertRequest {
                idempotency_key: Some(record.id.clone()),
                record: MemoryRecord {
                    id: record.id,
                    scope: MemoryScope {
                        tenant_id: "default".to_string(),
                        namespace: "evaluation".to_string(),
                        actor_id: "ava".to_string(),
                        conversation_id: Some("eval-thread".to_string()),
                        session_id: Some("eval-session".to_string()),
                        source: record.source,
                        labels: record.labels,
                        trust_level,
                    },
                    kind,
                    content: record.content,
                    summary: Some(record.summary),
                    source_id: None,
                    metadata,
                    quality_state,
                    created_at_unix_ms: record.created_at_unix_ms,
                    updated_at_unix_ms: record.updated_at_unix_ms,
                    expires_at_unix_ms: None,
                    importance_score: record.importance_score,
                    artifact: None,
                },
            }
        })
        .collect::<Vec<_>>();

    let cases = corpus
        .cases
        .into_iter()
        .map(|case| PreparedCase {
            scenario: case.scenario,
            relevant_record_ids: case.relevant_record_ids,
            query: RecallQuery {
                scope: MemoryScope {
                    tenant_id: "default".to_string(),
                    namespace: "evaluation".to_string(),
                    actor_id: "ava".to_string(),
                    conversation_id: Some("eval-thread".to_string()),
                    session_id: Some("eval-session".to_string()),
                    source: "benchmark-query".to_string(),
                    labels: Vec::new(),
                    trust_level: MemoryTrustLevel::Verified,
                },
                query_text: case.query_text,
                max_items: case.max_items,
                token_budget: None,
                filters: RecallFilters {
                    include_archived: case.include_archived,
                    ..RecallFilters::default()
                },
                include_explanation: true,
            },
        })
        .collect::<Vec<_>>();

    PreparedCorpus {
        requests,
        cases,
        export_request: ExportRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("evaluation".to_string()),
            include_archived: true,
        },
    }
}

fn engine_config(kind: RecallScorerKind, profile: RecallScoringProfile) -> EngineConfig {
    let mut config = EngineConfig::default();
    config.recall_scorer_kind = kind;
    config.recall_scoring_profile = profile;
    config.embedding_provider_kind = EmbeddingProviderKind::DeterministicLocal;
    config.embedding_dimensions = 64;
    config
}

fn summarize_ms(samples: &[f64], throughput_per_sec: Option<f64>) -> DurationSummary {
    let mut ordered = samples.to_vec();
    ordered.sort_by(|left, right| left.total_cmp(right));
    let mean_ms = if ordered.is_empty() {
        0.0
    } else {
        ordered.iter().sum::<f64>() / ordered.len() as f64
    };
    DurationSummary {
        samples: ordered.len(),
        mean_ms,
        p50_ms: percentile(&ordered, 0.50),
        p95_ms: percentile(&ordered, 0.95),
        max_ms: ordered.last().copied().unwrap_or(0.0),
        throughput_per_sec,
    }
}

fn percentile(samples: &[f64], percentile: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let index = ((samples.len() - 1) as f64 * percentile).round() as usize;
    samples[index.min(samples.len() - 1)]
}

fn metrics_from_pairs(rankings: &[(Vec<String>, Vec<String>)]) -> ScenarioMetrics {
    let rankings = rankings
        .iter()
        .map(|(ranked, relevant)| (ranked.as_slice(), relevant.as_slice()))
        .collect::<Vec<_>>();
    let metrics = evaluate_rankings_at_k(&rankings, 3);
    ScenarioMetrics {
        cases: metrics.cases,
        hit_rate_at_3: metrics.hit_rate_at_k,
        recall_at_3: metrics.recall_at_k,
        mrr: metrics.mrr,
        ndcg_at_3: metrics.ndcg_at_k,
    }
}

async fn evaluate_quality<S: MemoryStore>(
    store: &S,
    cases: &[PreparedCase],
) -> mnemara_core::Result<(ScenarioMetrics, Vec<ScenarioBenchmark>)> {
    let mut all_pairs = Vec::new();
    let mut grouped = BTreeMap::<String, Vec<(Vec<String>, Vec<String>)>>::new();

    for case in cases {
        let result = store.recall(case.query.clone()).await?;
        let ranked_ids = result
            .hits
            .into_iter()
            .map(|hit| hit.record.id)
            .collect::<Vec<_>>();
        all_pairs.push((ranked_ids.clone(), case.relevant_record_ids.clone()));
        grouped
            .entry(case.scenario.clone())
            .or_default()
            .push((ranked_ids, case.relevant_record_ids.clone()));
    }

    let by_scenario = grouped
        .into_iter()
        .map(|(scenario, rankings)| ScenarioBenchmark {
            scenario,
            metrics: metrics_from_pairs(&rankings),
        })
        .collect::<Vec<_>>();

    Ok((metrics_from_pairs(&all_pairs), by_scenario))
}

async fn seed_store<S: MemoryStore>(
    store: &S,
    corpus: &PreparedCorpus,
) -> mnemara_core::Result<()> {
    store
        .batch_upsert(BatchUpsertRequest {
            requests: corpus.requests.clone(),
        })
        .await?;
    Ok(())
}

async fn benchmark_backend<S, F>(
    backend: &str,
    config: EngineConfig,
    corpus: &PreparedCorpus,
    make_store: F,
) -> mnemara_core::Result<BackendBenchmark>
where
    S: MemoryStore,
    F: Fn(&Path, EngineConfig) -> mnemara_core::Result<S>,
{
    let mut upsert_ms = Vec::new();
    for _ in 0..UPSERT_RUNS {
        let dir = temp_dir(&format!("{backend}-upsert"));
        let store = make_store(&dir, config.clone())?;
        let started = Instant::now();
        seed_store(&store, corpus).await?;
        upsert_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        fs::remove_dir_all(dir).ok();
    }

    let quality_dir = temp_dir(&format!("{backend}-quality"));
    let quality_store = make_store(&quality_dir, config.clone())?;
    seed_store(&quality_store, corpus).await?;
    let (quality_overall, quality_by_scenario) =
        evaluate_quality(&quality_store, &corpus.cases).await?;

    let mut recall_ms = Vec::new();
    for _ in 0..RECALL_LOOPS {
        for case in &corpus.cases {
            let started = Instant::now();
            let _ = quality_store.recall(case.query.clone()).await?;
            recall_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        }
    }

    let mut snapshot_ms = Vec::new();
    let mut stats_ms = Vec::new();
    let mut export_ms = Vec::new();
    let mut compact_ms = Vec::new();
    for _ in 0..ADMIN_RUNS {
        let started = Instant::now();
        let _ = quality_store.snapshot().await?;
        snapshot_ms.push(started.elapsed().as_secs_f64() * 1000.0);

        let started = Instant::now();
        let _ = quality_store
            .stats(StoreStatsRequest {
                tenant_id: Some("default".to_string()),
                namespace: Some("evaluation".to_string()),
            })
            .await?;
        stats_ms.push(started.elapsed().as_secs_f64() * 1000.0);

        let started = Instant::now();
        let _ = quality_store.export(corpus.export_request.clone()).await?;
        export_ms.push(started.elapsed().as_secs_f64() * 1000.0);

        let started = Instant::now();
        let _ = quality_store
            .compact(CompactionRequest {
                tenant_id: "default".to_string(),
                namespace: Some("evaluation".to_string()),
                dry_run: true,
                reason: "benchmark".to_string(),
            })
            .await?;
        compact_ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    let exported = quality_store.export(corpus.export_request.clone()).await?;
    let mut import_ms = Vec::new();
    for _ in 0..ADMIN_RUNS {
        let dir = temp_dir(&format!("{backend}-import"));
        let store = make_store(&dir, config.clone())?;
        let started = Instant::now();
        let report = store
            .import(ImportRequest {
                package: exported.clone(),
                mode: ImportMode::Replace,
                dry_run: false,
            })
            .await?;
        assert!(report.applied, "import benchmark should apply changes");
        import_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        fs::remove_dir_all(dir).ok();
    }

    fs::remove_dir_all(quality_dir).ok();

    let total_records = corpus.requests.len() as f64;
    let ingest = summarize_ms(
        &upsert_ms,
        Some(total_records / (upsert_ms.iter().sum::<f64>() / upsert_ms.len() as f64 / 1000.0)),
    );

    Ok(BackendBenchmark {
        backend: backend.to_string(),
        quality_overall,
        quality_by_scenario,
        ingest,
        recall: summarize_ms(&recall_ms, None),
        admin_operations: AdminOperationSummary {
            snapshot: summarize_ms(&snapshot_ms, None),
            stats: summarize_ms(&stats_ms, None),
            export: summarize_ms(&export_ms, None),
            compact_dry_run: summarize_ms(&compact_ms, None),
            import_replace: summarize_ms(&import_ms, None),
        },
        exported_storage_bytes: exported.manifest.storage_bytes,
    })
}

fn markdown_summary(report: &BenchmarkReport) -> String {
    let mut output = String::new();
    output.push_str("# Benchmark report v1\n\n");
    output.push_str(&format!(
        "Environment: `{}` `{}` with {} logical CPUs.\n\n",
        report.environment.os, report.environment.arch, report.environment.logical_cpus
    ));
    for profile in &report.profiles {
        output.push_str(&format!(
            "## {:?} / {:?}\n\n",
            profile.scorer_kind, profile.scoring_profile
        ));
        output.push_str("| backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms | import mean ms |\n");
        output.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
        for backend in &profile.backend_results {
            output.push_str(&format!(
                "| {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} |\n",
                backend.backend,
                backend.quality_overall.hit_rate_at_3,
                backend.quality_overall.recall_at_3,
                backend.quality_overall.mrr,
                backend.quality_overall.ndcg_at_3,
                backend.ingest.mean_ms,
                backend.recall.p95_ms,
                backend.admin_operations.import_replace.mean_ms,
            ));
        }
        output.push('\n');
    }
    output
}

fn ensure_parent(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent directory should exist");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (output_path, summary_path) = parse_args();
    let runtime = tokio::runtime::Runtime::new()?;
    let corpus = load_corpus();

    let profiles = runtime.block_on(async {
        let configs = [
            (RecallScorerKind::Profile, RecallScoringProfile::Balanced),
            (
                RecallScorerKind::Profile,
                RecallScoringProfile::LexicalFirst,
            ),
            (RecallScorerKind::Curated, RecallScoringProfile::Balanced),
            (
                RecallScorerKind::Curated,
                RecallScoringProfile::ImportanceFirst,
            ),
        ];
        let mut profiles = Vec::new();
        for (kind, profile) in configs {
            let config = engine_config(kind, profile);
            let sled = benchmark_backend("sled", config.clone(), &corpus, |path, engine| {
                SledMemoryStore::open(SledStoreConfig::new(path).with_engine_config(engine))
            })
            .await?;
            let file = benchmark_backend("file", config.clone(), &corpus, |path, engine| {
                FileMemoryStore::open(FileStoreConfig::new(path).with_engine_config(engine))
            })
            .await?;
            profiles.push(ProfileBenchmark {
                scorer_kind: kind,
                scoring_profile: profile,
                backend_results: vec![sled, file],
            });
        }
        Ok::<_, mnemara_core::Error>(profiles)
    })?;

    let report = BenchmarkReport {
        report_version: 1,
        generated_at_unix_ms: now_unix_ms(),
        corpus_path: corpus_path().display().to_string(),
        environment: BenchmarkEnvironment {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            logical_cpus: std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1),
        },
        measurement: MeasurementConfig {
            upsert_runs: UPSERT_RUNS,
            recall_loops: RECALL_LOOPS,
            admin_runs: ADMIN_RUNS,
        },
        profiles,
    };

    ensure_parent(&output_path);
    ensure_parent(&summary_path);
    fs::write(&output_path, serde_json::to_vec_pretty(&report)?)?;
    fs::write(&summary_path, markdown_summary(&report))?;
    println!("wrote {}", output_path.display());
    println!("wrote {}", summary_path.display());
    Ok(())
}
