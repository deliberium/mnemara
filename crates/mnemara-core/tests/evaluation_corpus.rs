use mnemara_core::{
    CuratedRecallScorer, MemoryQualityState, MemoryRecord, MemoryRecordKind, MemoryScope,
    MemoryTrustLevel, RecallFilters, RecallQuery, RecallScorer, RecallScoringProfile,
    evaluate_rankings_at_k,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

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
    name: String,
    scenario: String,
    query_text: String,
    relevant_record_ids: Vec<String>,
    #[serde(default = "default_max_items")]
    max_items: usize,
    #[serde(default)]
    include_archived: bool,
}

fn default_max_items() -> usize {
    5
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/evaluation/ranking-corpus-v1.json")
}

fn load_corpus() -> Corpus {
    serde_json::from_str(&fs::read_to_string(corpus_path()).expect("corpus file should exist"))
        .expect("corpus should decode")
}

fn map_kind(value: &str) -> MemoryRecordKind {
    match value {
        "summary" => MemoryRecordKind::Summary,
        "task" => MemoryRecordKind::Task,
        _ => MemoryRecordKind::Fact,
    }
}

fn map_trust(value: &str) -> MemoryTrustLevel {
    match value {
        "pinned" => MemoryTrustLevel::Pinned,
        "observed" => MemoryTrustLevel::Observed,
        _ => MemoryTrustLevel::Verified,
    }
}

fn map_quality(value: &str) -> MemoryQualityState {
    match value {
        "active" => MemoryQualityState::Active,
        "archived" => MemoryQualityState::Archived,
        "draft" => MemoryQualityState::Draft,
        _ => MemoryQualityState::Verified,
    }
}

fn base_scope(source: &str, labels: Vec<String>, trust_level: MemoryTrustLevel) -> MemoryScope {
    MemoryScope {
        tenant_id: "default".to_string(),
        namespace: "evaluation".to_string(),
        actor_id: "ava".to_string(),
        conversation_id: Some("eval-thread".to_string()),
        session_id: Some("eval-session".to_string()),
        source: source.to_string(),
        labels,
        trust_level,
    }
}

#[test]
fn curated_scorer_meets_stratified_quality_baseline_on_ranked_corpus() {
    let corpus = load_corpus();
    let records = corpus
        .records
        .into_iter()
        .map(|record| {
            let mut metadata = record.metadata;
            metadata.insert("scenario".to_string(), record.scenario);
            MemoryRecord {
                id: record.id,
                scope: base_scope(
                    &record.source,
                    record.labels,
                    map_trust(&record.trust_level),
                ),
                kind: map_kind(&record.kind),
                content: record.content,
                summary: Some(record.summary),
                metadata,
                quality_state: map_quality(&record.quality_state),
                created_at_unix_ms: record.created_at_unix_ms,
                updated_at_unix_ms: record.updated_at_unix_ms,
                expires_at_unix_ms: None,
                importance_score: record.importance_score,
                source_id: None,
                artifact: None,
            }
        })
        .collect::<Vec<_>>();

    let scorer = CuratedRecallScorer::new(RecallScoringProfile::Balanced);
    let mut overall_ranked = Vec::new();
    let mut overall_relevant = Vec::new();
    let mut cases_by_scenario = BTreeMap::<String, Vec<(Vec<String>, Vec<String>)>>::new();

    for case in corpus.cases {
        let query = RecallQuery {
            scope: base_scope("eval-query", vec![], MemoryTrustLevel::Verified),
            query_text: case.query_text,
            max_items: case.max_items,
            token_budget: None,
            filters: RecallFilters {
                include_archived: case.include_archived,
                ..RecallFilters::default()
            },
            include_explanation: false,
        };

        let mut ranked = records
            .iter()
            .filter(|record| {
                case.include_archived || record.quality_state != MemoryQualityState::Archived
            })
            .filter_map(|record| scorer.score(record, &query))
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .hit
                .breakdown
                .total
                .total_cmp(&left.hit.breakdown.total)
                .then_with(|| left.hit.record.id.cmp(&right.hit.record.id))
        });
        let ranked_ids = ranked
            .into_iter()
            .map(|candidate| candidate.hit.record.id)
            .collect::<Vec<_>>();

        assert!(
            !ranked_ids.is_empty(),
            "case {} produced no ranked candidates",
            case.name
        );
        assert!(
            case.relevant_record_ids.iter().any(|record_id| ranked_ids
                .iter()
                .take(case.max_items)
                .any(|ranked_id| ranked_id == record_id)),
            "case {} missed relevant records in top {}",
            case.name,
            case.max_items
        );

        overall_ranked.push(ranked_ids.clone());
        overall_relevant.push(case.relevant_record_ids.clone());
        cases_by_scenario
            .entry(case.scenario)
            .or_default()
            .push((ranked_ids, case.relevant_record_ids));
    }

    let overall_pairs = overall_ranked
        .iter()
        .zip(overall_relevant.iter())
        .map(|(ranked, relevant)| (ranked.as_slice(), relevant.as_slice()))
        .collect::<Vec<_>>();
    let overall = evaluate_rankings_at_k(&overall_pairs, 3);

    assert_eq!(overall.cases, 9);
    assert!(overall.hit_rate_at_k >= 1.0);
    assert!(overall.recall_at_k >= 0.88);
    assert!(overall.mrr >= 0.88);
    assert!(overall.ndcg_at_k >= 0.9);

    let required_scenarios = BTreeSet::from([
        "exact_lookup",
        "duplicate_heavy",
        "recent_thread",
        "durable_high_trust",
        "archival_cold_tier",
        "noisy_distractor",
        "portability_regression",
        "fairness_runtime",
        "deployment_transport",
    ]);

    for scenario in required_scenarios {
        let scenario_pairs = cases_by_scenario
            .get(scenario)
            .unwrap_or_else(|| panic!("missing scenario slice {scenario}"))
            .iter()
            .map(|(ranked, relevant)| (ranked.as_slice(), relevant.as_slice()))
            .collect::<Vec<_>>();
        let metrics = evaluate_rankings_at_k(&scenario_pairs, 3);
        assert!(metrics.hit_rate_at_k >= 1.0, "scenario {scenario} missed");
        assert!(metrics.mrr >= 0.75, "scenario {scenario} regressed");
    }
}
