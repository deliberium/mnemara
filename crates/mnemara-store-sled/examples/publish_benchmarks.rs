#![allow(clippy::field_reassign_with_default)]

use mnemara_core::{
    AffectiveAnnotation, AffectiveAnnotationProvenance, BatchUpsertRequest, CompactionRequest,
    DeterministicLocalEmbedder, EPISODE_SCHEMA_VERSION, EmbeddingProviderKind, EngineConfig,
    EpisodeContext, EpisodeContinuityState, EpisodeSalience, ExportRequest, ImportMode,
    ImportRequest, IntegrityCheckRequest, LineageLink, LineageRelationKind, MemoryHistoricalState,
    MemoryQualityState, MemoryRecord, MemoryRecordKind, MemoryScope, MemoryStore, MemoryTrustLevel,
    RecallFilters, RecallHistoricalMode, RecallPlanner, RecallPlanningProfile, RecallPolicyProfile,
    RecallQuery, RecallScorerKind, RecallScoringProfile, RecallTemporalOrder, RepairRequest,
    StoreStatsRequest, UpsertRequest, evaluate_rankings_at_k,
};
use mnemara_store_file::{FileMemoryStore, FileStoreConfig};
use mnemara_store_sled::{SledMemoryStore, SledStoreConfig};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
    source_id: Option<String>,
    #[serde(default = "default_historical_state")]
    historical_state: String,
    #[serde(default)]
    episode: Option<CorpusEpisode>,
    #[serde(default)]
    lineage: Vec<CorpusLineageLink>,
    #[serde(default)]
    metadata: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct CorpusEpisode {
    #[serde(default = "default_episode_schema_version")]
    schema_version: u32,
    episode_id: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default = "default_continuity_state")]
    continuity_state: String,
    #[serde(default)]
    actor_ids: Vec<String>,
    #[serde(default)]
    goal: Option<String>,
    #[serde(default)]
    outcome: Option<String>,
    #[serde(default)]
    started_at_unix_ms: Option<u64>,
    #[serde(default)]
    ended_at_unix_ms: Option<u64>,
    #[serde(default)]
    last_active_unix_ms: Option<u64>,
    #[serde(default)]
    recurrence_key: Option<String>,
    #[serde(default)]
    recurrence_interval_ms: Option<u64>,
    #[serde(default)]
    boundary_label: Option<String>,
    #[serde(default)]
    previous_record_id: Option<String>,
    #[serde(default)]
    next_record_id: Option<String>,
    #[serde(default)]
    causal_record_ids: Vec<String>,
    #[serde(default)]
    related_record_ids: Vec<String>,
    #[serde(default)]
    linked_artifact_uris: Vec<String>,
    #[serde(default)]
    salience: CorpusEpisodeSalience,
    #[serde(default)]
    affective: Option<CorpusAffective>,
}

#[derive(Debug, Deserialize, Default)]
struct CorpusEpisodeSalience {
    #[serde(default)]
    reuse_count: u32,
    #[serde(default)]
    novelty_score: f32,
    #[serde(default)]
    goal_relevance: f32,
    #[serde(default)]
    unresolved_weight: f32,
}

#[derive(Debug, Deserialize)]
struct CorpusAffective {
    #[serde(default)]
    tone: Option<String>,
    #[serde(default)]
    sentiment: Option<String>,
    #[serde(default)]
    urgency: f32,
    #[serde(default = "default_confidence")]
    confidence: f32,
    #[serde(default)]
    tension: f32,
    #[serde(default = "default_affective_provenance")]
    provenance: String,
}

#[derive(Debug, Deserialize)]
struct CorpusLineageLink {
    record_id: String,
    #[serde(default = "default_lineage_relation")]
    relation: String,
    #[serde(default = "default_confidence")]
    confidence: f32,
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
    #[serde(default)]
    filters: CorpusCaseFilters,
}

#[derive(Debug, Deserialize, Default)]
struct CorpusCaseFilters {
    #[serde(default)]
    episode_id: Option<String>,
    #[serde(default)]
    continuity_states: Vec<String>,
    #[serde(default)]
    unresolved_only: bool,
    #[serde(default)]
    temporal_order: Option<String>,
    #[serde(default)]
    historical_mode: Option<String>,
    #[serde(default)]
    lineage_record_id: Option<String>,
}

#[derive(Debug, Clone)]
struct PreparedCorpus {
    records: Vec<MemoryRecord>,
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
    salience_profiles: Vec<SalienceBenchmark>,
    shared_embedder_profiles: Vec<SharedEmbedderBenchmark>,
    planner_stage_profiles: Vec<PlannerStageBenchmark>,
    provenance_policy_profiles: Vec<PolicyProfileBenchmark>,
    maintenance_profiles: Vec<MaintenanceBenchmark>,
}

#[derive(Debug, Serialize)]
struct SalienceBenchmark {
    scorer_kind: RecallScorerKind,
    scoring_profile: RecallScoringProfile,
    planning_profile: RecallPlanningProfile,
    policy_profile: RecallPolicyProfile,
    condition_results: Vec<SalienceConditionBenchmark>,
}

#[derive(Debug, Serialize)]
struct SalienceConditionBenchmark {
    condition: String,
    backend_results: Vec<BackendBenchmark>,
}

#[derive(Debug, Serialize)]
struct SharedEmbedderBenchmark {
    scorer_kind: RecallScorerKind,
    scoring_profile: RecallScoringProfile,
    planning_profile: RecallPlanningProfile,
    policy_profile: RecallPolicyProfile,
    condition_results: Vec<SharedEmbedderConditionBenchmark>,
}

#[derive(Debug, Serialize)]
struct SharedEmbedderConditionBenchmark {
    condition: String,
    backend_results: Vec<BackendBenchmark>,
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
    planning_profile: RecallPlanningProfile,
    policy_profile: RecallPolicyProfile,
    backend_results: Vec<BackendBenchmark>,
}

#[derive(Debug, Serialize)]
struct PlannerStageBenchmark {
    scorer_kind: RecallScorerKind,
    scoring_profile: RecallScoringProfile,
    planning_profile: RecallPlanningProfile,
    policy_profile: RecallPolicyProfile,
    stage_timings: PlannerStageTimingSummary,
}

#[derive(Debug, Serialize)]
struct PlannerStageTimingSummary {
    candidate_generation: DurationSummary,
    graph_expansion: DurationSummary,
    total_planning: DurationSummary,
    mean_seeded_candidates: f64,
    mean_expanded_candidates: f64,
    max_hops_applied: u8,
}

#[derive(Debug, Serialize)]
struct PolicyProfileBenchmark {
    scorer_kind: RecallScorerKind,
    scoring_profile: RecallScoringProfile,
    planning_profile: RecallPlanningProfile,
    policy_profile: RecallPolicyProfile,
    backend_results: Vec<BackendBenchmark>,
}

#[derive(Debug, Serialize)]
struct MaintenanceBenchmark {
    backend: String,
    records_benchmarked: usize,
    consolidation_apply: DurationSummary,
    recall_during_maintenance: DurationSummary,
    integrity_check: DurationSummary,
    repair_rebuild: DurationSummary,
    recovery_import_replace: DurationSummary,
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

fn default_historical_state() -> String {
    "current".to_string()
}

fn default_continuity_state() -> String {
    "open".to_string()
}

fn default_episode_schema_version() -> u32 {
    EPISODE_SCHEMA_VERSION
}

fn default_lineage_relation() -> String {
    "derived_from".to_string()
}

fn default_affective_provenance() -> String {
    "authored".to_string()
}

fn default_confidence() -> f32 {
    1.0
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
                "untrusted" => MemoryTrustLevel::Untrusted,
                "pinned" => MemoryTrustLevel::Pinned,
                "observed" => MemoryTrustLevel::Observed,
                "derived" => MemoryTrustLevel::Derived,
                _ => MemoryTrustLevel::Verified,
            };
            let quality_state = match record.quality_state.as_str() {
                "active" => MemoryQualityState::Active,
                "archived" => MemoryQualityState::Archived,
                "draft" => MemoryQualityState::Draft,
                "suppressed" => MemoryQualityState::Suppressed,
                "deleted" => MemoryQualityState::Deleted,
                _ => MemoryQualityState::Verified,
            };
            let kind = match record.kind.as_str() {
                "episodic" => MemoryRecordKind::Episodic,
                "summary" => MemoryRecordKind::Summary,
                "preference" => MemoryRecordKind::Preference,
                "task" => MemoryRecordKind::Task,
                "artifact" => MemoryRecordKind::Artifact,
                "hypothesis" => MemoryRecordKind::Hypothesis,
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
                    metadata,
                    quality_state,
                    created_at_unix_ms: record.created_at_unix_ms,
                    updated_at_unix_ms: record.updated_at_unix_ms,
                    expires_at_unix_ms: None,
                    importance_score: record.importance_score,
                    artifact: None,
                    source_id: record.source_id,
                    episode: map_episode(record.episode),
                    historical_state: map_historical_state(&record.historical_state),
                    lineage: map_lineage(record.lineage),
                },
            }
        })
        .collect::<Vec<_>>();

    let records = requests
        .iter()
        .map(|request| request.record.clone())
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
                    episode_id: case.filters.episode_id,
                    continuity_states: case
                        .filters
                        .continuity_states
                        .iter()
                        .map(|value| map_continuity_state(value))
                        .collect(),
                    unresolved_only: case.filters.unresolved_only,
                    temporal_order: map_temporal_order(case.filters.temporal_order.as_deref()),
                    historical_mode: map_historical_mode(case.filters.historical_mode.as_deref()),
                    lineage_record_id: case.filters.lineage_record_id,
                    ..RecallFilters::default()
                },
                include_explanation: true,
            },
        })
        .collect::<Vec<_>>();

    PreparedCorpus {
        records,
        requests,
        cases,
        export_request: ExportRequest {
            tenant_id: Some("default".to_string()),
            namespace: Some("evaluation".to_string()),
            include_archived: true,
        },
    }
}

fn map_historical_state(value: &str) -> MemoryHistoricalState {
    match value {
        "historical" => MemoryHistoricalState::Historical,
        "superseded" => MemoryHistoricalState::Superseded,
        _ => MemoryHistoricalState::Current,
    }
}

fn map_continuity_state(value: &str) -> EpisodeContinuityState {
    match value {
        "resolved" => EpisodeContinuityState::Resolved,
        "superseded" => EpisodeContinuityState::Superseded,
        "abandoned" => EpisodeContinuityState::Abandoned,
        _ => EpisodeContinuityState::Open,
    }
}

fn map_lineage_relation(value: &str) -> LineageRelationKind {
    match value {
        "consolidated_from" => LineageRelationKind::ConsolidatedFrom,
        "supersedes" => LineageRelationKind::Supersedes,
        "superseded_by" => LineageRelationKind::SupersededBy,
        "conflicts_with" => LineageRelationKind::ConflictsWith,
        _ => LineageRelationKind::DerivedFrom,
    }
}

fn map_affective_provenance(value: &str) -> AffectiveAnnotationProvenance {
    match value {
        "imported" => AffectiveAnnotationProvenance::Imported,
        "derived" => AffectiveAnnotationProvenance::Derived,
        _ => AffectiveAnnotationProvenance::Authored,
    }
}

fn map_episode(value: Option<CorpusEpisode>) -> Option<EpisodeContext> {
    value.map(|episode| EpisodeContext {
        schema_version: episode.schema_version,
        episode_id: episode.episode_id,
        summary: episode.summary,
        continuity_state: map_continuity_state(&episode.continuity_state),
        actor_ids: episode.actor_ids,
        goal: episode.goal,
        outcome: episode.outcome,
        started_at_unix_ms: episode.started_at_unix_ms,
        ended_at_unix_ms: episode.ended_at_unix_ms,
        last_active_unix_ms: episode.last_active_unix_ms,
        recurrence_key: episode.recurrence_key,
        recurrence_interval_ms: episode.recurrence_interval_ms,
        boundary_label: episode.boundary_label,
        previous_record_id: episode.previous_record_id,
        next_record_id: episode.next_record_id,
        causal_record_ids: episode.causal_record_ids,
        related_record_ids: episode.related_record_ids,
        linked_artifact_uris: episode.linked_artifact_uris,
        salience: EpisodeSalience {
            reuse_count: episode.salience.reuse_count,
            novelty_score: episode.salience.novelty_score,
            goal_relevance: episode.salience.goal_relevance,
            unresolved_weight: episode.salience.unresolved_weight,
        },
        affective: episode.affective.map(|affective| AffectiveAnnotation {
            tone: affective.tone,
            sentiment: affective.sentiment,
            urgency: affective.urgency,
            confidence: affective.confidence,
            tension: affective.tension,
            provenance: map_affective_provenance(&affective.provenance),
        }),
    })
}

fn map_lineage(value: Vec<CorpusLineageLink>) -> Vec<LineageLink> {
    value
        .into_iter()
        .map(|link| LineageLink {
            record_id: link.record_id,
            relation: map_lineage_relation(&link.relation),
            confidence: link.confidence,
        })
        .collect()
}

fn map_temporal_order(value: Option<&str>) -> RecallTemporalOrder {
    match value {
        Some("chronological_asc") => RecallTemporalOrder::ChronologicalAsc,
        Some("chronological_desc") => RecallTemporalOrder::ChronologicalDesc,
        _ => RecallTemporalOrder::Relevance,
    }
}

fn map_historical_mode(value: Option<&str>) -> RecallHistoricalMode {
    match value {
        Some("include_historical") => RecallHistoricalMode::IncludeHistorical,
        Some("historical_only") => RecallHistoricalMode::HistoricalOnly,
        _ => RecallHistoricalMode::CurrentOnly,
    }
}

fn engine_config(
    kind: RecallScorerKind,
    profile: RecallScoringProfile,
    planning_profile: RecallPlanningProfile,
    policy_profile: RecallPolicyProfile,
) -> EngineConfig {
    let mut config = EngineConfig::default();
    config.recall_scorer_kind = kind;
    config.recall_scoring_profile = profile;
    config.recall_planning_profile = planning_profile;
    config.recall_policy_profile = policy_profile;
    config.embedding_provider_kind = EmbeddingProviderKind::DeterministicLocal;
    config.embedding_dimensions = 64;
    config
}

fn shared_embedder_engine_config(
    kind: RecallScorerKind,
    profile: RecallScoringProfile,
    planning_profile: RecallPlanningProfile,
    policy_profile: RecallPolicyProfile,
) -> EngineConfig {
    let mut config = engine_config(kind, profile, planning_profile, policy_profile);
    config.embedding_provider_kind = EmbeddingProviderKind::Disabled;
    config
}

fn mean_usize(values: &[usize]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<usize>() as f64 / values.len() as f64
}

fn benchmark_planner_stages(
    config: EngineConfig,
    corpus: &PreparedCorpus,
) -> PlannerStageBenchmark {
    let planner = RecallPlanner::from_engine_config(&config);
    let mut candidate_generation_ms = Vec::new();
    let mut graph_expansion_ms = Vec::new();
    let mut total_planning_ms = Vec::new();
    let mut seeded_candidates = Vec::new();
    let mut expanded_candidates = Vec::new();
    let mut max_hops_applied = 0u8;

    for _ in 0..RECALL_LOOPS {
        for case in &corpus.cases {
            let (_, metrics) = planner.plan_with_metrics(&corpus.records, &case.query);
            candidate_generation_ms.push(metrics.candidate_generation_ns as f64 / 1_000_000.0);
            graph_expansion_ms.push(metrics.graph_expansion_ns as f64 / 1_000_000.0);
            total_planning_ms.push(metrics.total_ns as f64 / 1_000_000.0);
            seeded_candidates.push(metrics.seeded_candidates);
            expanded_candidates.push(metrics.expanded_candidates);
            max_hops_applied = max_hops_applied.max(metrics.hops_applied);
        }
    }

    PlannerStageBenchmark {
        scorer_kind: config.recall_scorer_kind,
        scoring_profile: config.recall_scoring_profile,
        planning_profile: config.recall_planning_profile,
        policy_profile: config.recall_policy_profile,
        stage_timings: PlannerStageTimingSummary {
            candidate_generation: summarize_ms(&candidate_generation_ms, None),
            graph_expansion: summarize_ms(&graph_expansion_ms, None),
            total_planning: summarize_ms(&total_planning_ms, None),
            mean_seeded_candidates: mean_usize(&seeded_candidates),
            mean_expanded_candidates: mean_usize(&expanded_candidates),
            max_hops_applied,
        },
    }
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

fn corpus_without_salience(corpus: &PreparedCorpus) -> PreparedCorpus {
    let mut rewritten = corpus.clone();
    for request in &mut rewritten.requests {
        if let Some(episode) = &mut request.record.episode {
            episode.salience = EpisodeSalience::default();
        }
    }
    rewritten.records = rewritten
        .requests
        .iter()
        .map(|request| request.record.clone())
        .collect();
    rewritten
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

fn hex_key(input: &str) -> String {
    let mut output = String::with_capacity(input.len() * 2);
    for byte in input.as_bytes() {
        output.push(char::from(b"0123456789abcdef"[(byte >> 4) as usize]));
        output.push(char::from(b"0123456789abcdef"[(byte & 0x0f) as usize]));
    }
    output
}

fn remove_file_idempotency_key(
    dir: &Path,
    scope: &MemoryScope,
    key: &str,
) -> mnemara_core::Result<()> {
    let scoped = idempotency_scoped_key(scope, key);
    let path = dir
        .join("idempotency")
        .join(format!("{}.txt", hex_key(&scoped)));
    fs::remove_file(&path).map_err(|err| {
        mnemara_core::Error::Backend(format!(
            "failed to remove idempotency file {}: {err}",
            path.display()
        ))
    })
}

fn remove_sled_idempotency_key(
    dir: &Path,
    scope: &MemoryScope,
    key: &str,
) -> mnemara_core::Result<()> {
    let db = sled::open(dir).map_err(|err| {
        mnemara_core::Error::Backend(format!(
            "failed to open sled db for repair benchmark: {err}"
        ))
    })?;
    let tree = db.open_tree("idempotency").map_err(|err| {
        mnemara_core::Error::Backend(format!(
            "failed to open idempotency tree for repair benchmark: {err}"
        ))
    })?;
    tree.remove(idempotency_scoped_key(scope, key).as_bytes())
        .map_err(|err| {
            mnemara_core::Error::Backend(format!(
                "failed to remove sled idempotency key for repair benchmark: {err}"
            ))
        })?;
    db.flush().map_err(|err| {
        mnemara_core::Error::Backend(format!(
            "failed to flush sled db for repair benchmark: {err}"
        ))
    })?;
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

async fn benchmark_maintenance_backend<S, F, C>(
    backend: &str,
    mut config: EngineConfig,
    corpus: &PreparedCorpus,
    make_store: F,
    corrupt_idempotency: C,
) -> mnemara_core::Result<MaintenanceBenchmark>
where
    S: MemoryStore + Send + Sync + 'static,
    F: Fn(&Path, EngineConfig) -> mnemara_core::Result<S>,
    C: Fn(&Path, &MemoryScope, &str) -> mnemara_core::Result<()>,
{
    config.compaction.summarize_after_record_count =
        config.compaction.summarize_after_record_count.max(2);
    config.compaction.cold_archive_after_days = 1;
    config
        .compaction
        .cold_archive_importance_threshold_per_mille = config
        .compaction
        .cold_archive_importance_threshold_per_mille
        .max(1000);

    let total_records = corpus.requests.len() as f64;
    let repair_seed = corpus
        .requests
        .first()
        .expect("benchmark corpus should have at least one record");
    let repair_scope = repair_seed.record.scope.clone();
    let repair_key = repair_seed
        .idempotency_key
        .clone()
        .expect("benchmark corpus request should have idempotency key");

    let mut consolidation_ms = Vec::new();
    for _ in 0..ADMIN_RUNS {
        let dir = temp_dir(&format!("{backend}-maintenance-compact"));
        let store = make_store(&dir, config.clone())?;
        seed_store(&store, corpus).await?;
        let started = Instant::now();
        let report = store
            .compact(CompactionRequest {
                tenant_id: "default".to_string(),
                namespace: Some("evaluation".to_string()),
                dry_run: false,
                reason: "benchmark-maintenance".to_string(),
            })
            .await?;
        assert!(
            report.archived_records > 0
                || report.deduplicated_records > 0
                || report.summarized_clusters > 0,
            "maintenance benchmark should exercise archival or consolidation work"
        );
        consolidation_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        fs::remove_dir_all(dir).ok();
    }

    let recall_dir = temp_dir(&format!("{backend}-maintenance-recall"));
    let recall_store = Arc::new(make_store(&recall_dir, config.clone())?);
    seed_store(recall_store.as_ref(), corpus).await?;
    let mut recall_during_ms = Vec::new();
    for _ in 0..RECALL_LOOPS {
        for case in &corpus.cases {
            let maintenance_store = Arc::clone(&recall_store);
            let maintenance = tokio::spawn(async move {
                maintenance_store
                    .compact(CompactionRequest {
                        tenant_id: "default".to_string(),
                        namespace: Some("evaluation".to_string()),
                        dry_run: true,
                        reason: "benchmark-concurrent-maintenance".to_string(),
                    })
                    .await
            });
            let started = Instant::now();
            let result = recall_store.recall(case.query.clone()).await?;
            assert!(
                !result.hits.is_empty(),
                "maintenance recall benchmark should return results"
            );
            recall_during_ms.push(started.elapsed().as_secs_f64() * 1000.0);
            maintenance.await.map_err(|err| {
                mnemara_core::Error::Backend(format!("maintenance join failed: {err}"))
            })??;
        }
    }
    fs::remove_dir_all(recall_dir).ok();

    let mut integrity_ms = Vec::new();
    let mut repair_ms = Vec::new();
    for _ in 0..ADMIN_RUNS {
        let dir = temp_dir(&format!("{backend}-maintenance-repair"));
        let store = make_store(&dir, config.clone())?;
        seed_store(&store, corpus).await?;
        drop(store);
        corrupt_idempotency(&dir, &repair_scope, &repair_key)?;

        let reopened = make_store(&dir, config.clone())?;
        let started = Instant::now();
        let integrity = reopened
            .integrity_check(IntegrityCheckRequest {
                tenant_id: Some("default".to_string()),
                namespace: Some("evaluation".to_string()),
            })
            .await?;
        assert_eq!(integrity.missing_idempotency_keys, 1);
        integrity_ms.push(started.elapsed().as_secs_f64() * 1000.0);

        let started = Instant::now();
        let repair = reopened
            .repair(RepairRequest {
                tenant_id: Some("default".to_string()),
                namespace: Some("evaluation".to_string()),
                dry_run: false,
                reason: "benchmark-maintenance".to_string(),
                remove_stale_idempotency_keys: false,
                rebuild_missing_idempotency_keys: true,
            })
            .await?;
        assert_eq!(repair.rebuilt_missing_idempotency_keys, 1);
        repair_ms.push(started.elapsed().as_secs_f64() * 1000.0);

        drop(reopened);
        let repaired = make_store(&dir, config.clone())?;
        let integrity_after = repaired
            .integrity_check(IntegrityCheckRequest {
                tenant_id: Some("default".to_string()),
                namespace: Some("evaluation".to_string()),
            })
            .await?;
        assert!(integrity_after.healthy);
        fs::remove_dir_all(dir).ok();
    }

    let import_dir = temp_dir(&format!("{backend}-maintenance-export"));
    let import_store = make_store(&import_dir, config.clone())?;
    seed_store(&import_store, corpus).await?;
    let exported = import_store.export(corpus.export_request.clone()).await?;
    fs::remove_dir_all(import_dir).ok();

    let mut recovery_import_ms = Vec::new();
    for _ in 0..ADMIN_RUNS {
        let dir = temp_dir(&format!("{backend}-maintenance-import"));
        let store = make_store(&dir, config.clone())?;
        let started = Instant::now();
        let report = store
            .import(ImportRequest {
                package: exported.clone(),
                mode: ImportMode::Replace,
                dry_run: false,
            })
            .await?;
        assert!(
            report.applied,
            "maintenance recovery import should apply changes"
        );
        recovery_import_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        fs::remove_dir_all(dir).ok();
    }

    Ok(MaintenanceBenchmark {
        backend: backend.to_string(),
        records_benchmarked: corpus.requests.len(),
        consolidation_apply: summarize_ms(
            &consolidation_ms,
            Some(
                total_records
                    / (consolidation_ms.iter().sum::<f64>()
                        / consolidation_ms.len() as f64
                        / 1000.0),
            ),
        ),
        recall_during_maintenance: summarize_ms(&recall_during_ms, None),
        integrity_check: summarize_ms(&integrity_ms, None),
        repair_rebuild: summarize_ms(&repair_ms, None),
        recovery_import_replace: summarize_ms(&recovery_import_ms, None),
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
            "## {:?} / {:?} / {:?} / {:?}\n\n",
            profile.scorer_kind,
            profile.scoring_profile,
            profile.planning_profile,
            profile.policy_profile
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

    output.push_str("## Salience-isolated comparison\n\n");
    output.push_str("| scorer / profile | planner | policy | condition | backend | hit@3 | recall@3 | mrr | ndcg@3 | recall p95 ms |\n");
    output.push_str("| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: |\n");
    for profile in &report.salience_profiles {
        for condition in &profile.condition_results {
            for backend in &condition.backend_results {
                output.push_str(&format!(
                    "| {:?} / {:?} | {:?} | {:?} | {} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} |\n",
                    profile.scorer_kind,
                    profile.scoring_profile,
                    profile.planning_profile,
                    profile.policy_profile,
                    condition.condition,
                    backend.backend,
                    backend.quality_overall.hit_rate_at_3,
                    backend.quality_overall.recall_at_3,
                    backend.quality_overall.mrr,
                    backend.quality_overall.ndcg_at_3,
                    backend.recall.p95_ms,
                ));
            }
        }
    }
    output.push('\n');

    output.push_str("## Shared embedder injection comparison\n\n");
    output.push_str("| scorer / profile | planner | policy | condition | backend | hit@3 | recall@3 | mrr | ndcg@3 | ingest mean ms | recall p95 ms |\n");
    output.push_str("| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for profile in &report.shared_embedder_profiles {
        for condition in &profile.condition_results {
            for backend in &condition.backend_results {
                output.push_str(&format!(
                    "| {:?} / {:?} | {:?} | {:?} | {} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} |\n",
                    profile.scorer_kind,
                    profile.scoring_profile,
                    profile.planning_profile,
                    profile.policy_profile,
                    condition.condition,
                    backend.backend,
                    backend.quality_overall.hit_rate_at_3,
                    backend.quality_overall.recall_at_3,
                    backend.quality_overall.mrr,
                    backend.quality_overall.ndcg_at_3,
                    backend.ingest.mean_ms,
                    backend.recall.p95_ms,
                ));
            }
        }
    }
    output.push('\n');

    output.push_str("## Planner stage timings\n\n");
    output.push_str("| scorer / profile | planner | policy | candidate mean ms | graph p95 ms | total mean ms | mean seeded | mean expanded | max hops |\n");
    output.push_str("| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for profile in &report.planner_stage_profiles {
        output.push_str(&format!(
            "| {:?} / {:?} | {:?} | {:?} | {:.4} | {:.4} | {:.4} | {:.2} | {:.2} | {} |\n",
            profile.scorer_kind,
            profile.scoring_profile,
            profile.planning_profile,
            profile.policy_profile,
            profile.stage_timings.candidate_generation.mean_ms,
            profile.stage_timings.graph_expansion.p95_ms,
            profile.stage_timings.total_planning.mean_ms,
            profile.stage_timings.mean_seeded_candidates,
            profile.stage_timings.mean_expanded_candidates,
            profile.stage_timings.max_hops_applied,
        ));
    }
    output.push('\n');

    output.push_str("## Provenance policy profile comparison\n\n");
    output.push_str(
        "| policy profile | backend | hit@3 | recall@3 | mrr | ndcg@3 | recall p95 ms |\n",
    );
    output.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: |\n");
    for profile in &report.provenance_policy_profiles {
        for backend in &profile.backend_results {
            output.push_str(&format!(
                "| {:?} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} |\n",
                profile.policy_profile,
                backend.backend,
                backend.quality_overall.hit_rate_at_3,
                backend.quality_overall.recall_at_3,
                backend.quality_overall.mrr,
                backend.quality_overall.ndcg_at_3,
                backend.recall.p95_ms,
            ));
        }
    }

    output.push_str("\n## Lifecycle maintenance timings\n\n");
    output.push_str("| backend | records | consolidation rec/s | consolidation mean ms | recall-during-maintenance p95 ms | integrity mean ms | repair mean ms | recovery import mean ms |\n");
    output.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for profile in &report.maintenance_profiles {
        output.push_str(&format!(
            "| {} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} |\n",
            profile.backend,
            profile.records_benchmarked,
            profile
                .consolidation_apply
                .throughput_per_sec
                .unwrap_or_default(),
            profile.consolidation_apply.mean_ms,
            profile.recall_during_maintenance.p95_ms,
            profile.integrity_check.mean_ms,
            profile.repair_rebuild.mean_ms,
            profile.recovery_import_replace.mean_ms,
        ));
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
            (
                RecallScorerKind::Profile,
                RecallScoringProfile::Balanced,
                RecallPlanningProfile::FastPath,
                RecallPolicyProfile::General,
            ),
            (
                RecallScorerKind::Profile,
                RecallScoringProfile::Balanced,
                RecallPlanningProfile::ContinuityAware,
                RecallPolicyProfile::General,
            ),
            (
                RecallScorerKind::Profile,
                RecallScoringProfile::LexicalFirst,
                RecallPlanningProfile::FastPath,
                RecallPolicyProfile::General,
            ),
            (
                RecallScorerKind::Curated,
                RecallScoringProfile::Balanced,
                RecallPlanningProfile::FastPath,
                RecallPolicyProfile::General,
            ),
            (
                RecallScorerKind::Curated,
                RecallScoringProfile::ImportanceFirst,
                RecallPlanningProfile::FastPath,
                RecallPolicyProfile::General,
            ),
        ];
        let mut profiles = Vec::new();
        for (kind, profile, planning_profile, policy_profile) in configs {
            let config = engine_config(kind, profile, planning_profile, policy_profile);
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
                planning_profile,
                policy_profile,
                backend_results: vec![sled, file],
            });
        }
        Ok::<_, mnemara_core::Error>(profiles)
    })?;

    let salience_profiles = runtime.block_on(async {
        let config = engine_config(
            RecallScorerKind::Profile,
            RecallScoringProfile::Balanced,
            RecallPlanningProfile::ContinuityAware,
            RecallPolicyProfile::General,
        );
        let neutralized_corpus = corpus_without_salience(&corpus);
        Ok::<_, mnemara_core::Error>(vec![SalienceBenchmark {
            scorer_kind: config.recall_scorer_kind,
            scoring_profile: config.recall_scoring_profile,
            planning_profile: config.recall_planning_profile,
            policy_profile: config.recall_policy_profile,
            condition_results: vec![
                SalienceConditionBenchmark {
                    condition: "salience_enabled".to_string(),
                    backend_results: vec![
                        benchmark_backend("sled", config.clone(), &corpus, |path, engine| {
                            SledMemoryStore::open(
                                SledStoreConfig::new(path).with_engine_config(engine),
                            )
                        })
                        .await?,
                        benchmark_backend("file", config.clone(), &corpus, |path, engine| {
                            FileMemoryStore::open(
                                FileStoreConfig::new(path).with_engine_config(engine),
                            )
                        })
                        .await?,
                    ],
                },
                SalienceConditionBenchmark {
                    condition: "salience_neutralized".to_string(),
                    backend_results: vec![
                        benchmark_backend(
                            "sled",
                            config.clone(),
                            &neutralized_corpus,
                            |path, engine| {
                                SledMemoryStore::open(
                                    SledStoreConfig::new(path).with_engine_config(engine),
                                )
                            },
                        )
                        .await?,
                        benchmark_backend(
                            "file",
                            config.clone(),
                            &neutralized_corpus,
                            |path, engine| {
                                FileMemoryStore::open(
                                    FileStoreConfig::new(path).with_engine_config(engine),
                                )
                            },
                        )
                        .await?,
                    ],
                },
            ],
        }])
    })?;

    let shared_embedder_profiles = runtime.block_on(async {
        let baseline = engine_config(
            RecallScorerKind::Profile,
            RecallScoringProfile::Balanced,
            RecallPlanningProfile::ContinuityAware,
            RecallPolicyProfile::General,
        );
        let injected = shared_embedder_engine_config(
            RecallScorerKind::Profile,
            RecallScoringProfile::Balanced,
            RecallPlanningProfile::ContinuityAware,
            RecallPolicyProfile::General,
        );
        Ok::<_, mnemara_core::Error>(vec![SharedEmbedderBenchmark {
            scorer_kind: baseline.recall_scorer_kind,
            scoring_profile: baseline.recall_scoring_profile,
            planning_profile: baseline.recall_planning_profile,
            policy_profile: baseline.recall_policy_profile,
            condition_results: vec![
                SharedEmbedderConditionBenchmark {
                    condition: "engine_config_deterministic_local".to_string(),
                    backend_results: vec![
                        benchmark_backend("sled", baseline.clone(), &corpus, |path, engine| {
                            SledMemoryStore::open(
                                SledStoreConfig::new(path).with_engine_config(engine),
                            )
                        })
                        .await?,
                        benchmark_backend("file", baseline.clone(), &corpus, |path, engine| {
                            FileMemoryStore::open(
                                FileStoreConfig::new(path).with_engine_config(engine),
                            )
                        })
                        .await?,
                    ],
                },
                SharedEmbedderConditionBenchmark {
                    condition: "shared_injected_deterministic_local".to_string(),
                    backend_results: vec![
                        benchmark_backend("sled", injected.clone(), &corpus, |path, engine| {
                            SledMemoryStore::open(
                                SledStoreConfig::new(path)
                                    .with_engine_config(engine.clone())
                                    .with_shared_embedder(
                                        Arc::new(DeterministicLocalEmbedder::new(
                                            engine.embedding_dimensions,
                                        )),
                                        "embedding_provider=benchmark_shared_deterministic_local",
                                    ),
                            )
                        })
                        .await?,
                        benchmark_backend("file", injected.clone(), &corpus, |path, engine| {
                            FileMemoryStore::open(
                                FileStoreConfig::new(path)
                                    .with_engine_config(engine.clone())
                                    .with_shared_embedder(
                                        Arc::new(DeterministicLocalEmbedder::new(
                                            engine.embedding_dimensions,
                                        )),
                                        "embedding_provider=benchmark_shared_deterministic_local",
                                    ),
                            )
                        })
                        .await?,
                    ],
                },
            ],
        }])
    })?;

    let planner_stage_profiles = [
        engine_config(
            RecallScorerKind::Profile,
            RecallScoringProfile::Balanced,
            RecallPlanningProfile::FastPath,
            RecallPolicyProfile::General,
        ),
        engine_config(
            RecallScorerKind::Profile,
            RecallScoringProfile::Balanced,
            RecallPlanningProfile::ContinuityAware,
            RecallPolicyProfile::General,
        ),
        engine_config(
            RecallScorerKind::Profile,
            RecallScoringProfile::Balanced,
            RecallPlanningProfile::ContinuityAware,
            RecallPolicyProfile::Support,
        ),
    ]
    .into_iter()
    .map(|config| benchmark_planner_stages(config, &corpus))
    .collect::<Vec<_>>();

    let provenance_policy_profiles = runtime.block_on(async {
        let mut policies = Vec::new();
        for policy_profile in [
            RecallPolicyProfile::General,
            RecallPolicyProfile::Support,
            RecallPolicyProfile::Research,
            RecallPolicyProfile::Assistant,
            RecallPolicyProfile::AutonomousAgent,
        ] {
            let config = engine_config(
                RecallScorerKind::Profile,
                RecallScoringProfile::Balanced,
                RecallPlanningProfile::ContinuityAware,
                policy_profile,
            );
            let sled = benchmark_backend("sled", config.clone(), &corpus, |path, engine| {
                SledMemoryStore::open(SledStoreConfig::new(path).with_engine_config(engine))
            })
            .await?;
            let file = benchmark_backend("file", config.clone(), &corpus, |path, engine| {
                FileMemoryStore::open(FileStoreConfig::new(path).with_engine_config(engine))
            })
            .await?;
            policies.push(PolicyProfileBenchmark {
                scorer_kind: config.recall_scorer_kind,
                scoring_profile: config.recall_scoring_profile,
                planning_profile: config.recall_planning_profile,
                policy_profile,
                backend_results: vec![sled, file],
            });
        }
        Ok::<_, mnemara_core::Error>(policies)
    })?;

    let maintenance_profiles = runtime.block_on(async {
        let config = engine_config(
            RecallScorerKind::Profile,
            RecallScoringProfile::Balanced,
            RecallPlanningProfile::ContinuityAware,
            RecallPolicyProfile::General,
        );
        let sled = benchmark_maintenance_backend(
            "sled",
            config.clone(),
            &corpus,
            |path, engine| {
                SledMemoryStore::open(SledStoreConfig::new(path).with_engine_config(engine))
            },
            remove_sled_idempotency_key,
        )
        .await?;
        let file = benchmark_maintenance_backend(
            "file",
            config,
            &corpus,
            |path, engine| {
                FileMemoryStore::open(FileStoreConfig::new(path).with_engine_config(engine))
            },
            remove_file_idempotency_key,
        )
        .await?;
        Ok::<_, mnemara_core::Error>(vec![sled, file])
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
        salience_profiles,
        shared_embedder_profiles,
        planner_stage_profiles,
        provenance_policy_profiles,
        maintenance_profiles,
    };

    ensure_parent(&output_path);
    ensure_parent(&summary_path);
    fs::write(&output_path, serde_json::to_vec_pretty(&report)?)?;
    fs::write(&summary_path, markdown_summary(&report))?;
    println!("wrote {}", output_path.display());
    println!("wrote {}", summary_path.display());
    Ok(())
}
