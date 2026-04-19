use mnemara_core::{
    AffectiveAnnotation, AffectiveAnnotationProvenance, CuratedRecallScorer,
    EPISODE_SCHEMA_VERSION, EpisodeContext, EpisodeContinuityState, EpisodeSalience, LineageLink,
    LineageRelationKind, MemoryHistoricalState, MemoryQualityState, MemoryRecord, MemoryRecordKind,
    MemoryScope, MemoryTrustLevel, RecallFilters, RecallHistoricalMode, RecallQuery, RecallScorer,
    RecallScoringProfile, RecallTemporalOrder, evaluate_rankings_at_k,
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
    name: String,
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

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/evaluation/ranking-corpus-v1.json")
}

fn load_corpus() -> Corpus {
    serde_json::from_str(&fs::read_to_string(corpus_path()).expect("corpus file should exist"))
        .expect("corpus should decode")
}

fn map_kind(value: &str) -> MemoryRecordKind {
    match value {
        "episodic" => MemoryRecordKind::Episodic,
        "summary" => MemoryRecordKind::Summary,
        "preference" => MemoryRecordKind::Preference,
        "task" => MemoryRecordKind::Task,
        "artifact" => MemoryRecordKind::Artifact,
        "hypothesis" => MemoryRecordKind::Hypothesis,
        _ => MemoryRecordKind::Fact,
    }
}

fn map_trust(value: &str) -> MemoryTrustLevel {
    match value {
        "untrusted" => MemoryTrustLevel::Untrusted,
        "pinned" => MemoryTrustLevel::Pinned,
        "observed" => MemoryTrustLevel::Observed,
        "derived" => MemoryTrustLevel::Derived,
        _ => MemoryTrustLevel::Verified,
    }
}

fn map_quality(value: &str) -> MemoryQualityState {
    match value {
        "active" => MemoryQualityState::Active,
        "archived" => MemoryQualityState::Archived,
        "draft" => MemoryQualityState::Draft,
        "suppressed" => MemoryQualityState::Suppressed,
        "deleted" => MemoryQualityState::Deleted,
        _ => MemoryQualityState::Verified,
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

fn record_temporal_anchor(record: &MemoryRecord) -> u64 {
    record
        .episode
        .as_ref()
        .and_then(|episode| episode.last_active_unix_ms.or(episode.started_at_unix_ms))
        .unwrap_or(record.updated_at_unix_ms)
}

fn record_matches_filters(record: &MemoryRecord, query: &RecallQuery) -> bool {
    let filters = &query.filters;

    if !filters.kinds.is_empty() && !filters.kinds.contains(&record.kind) {
        return false;
    }
    if !filters.required_labels.is_empty()
        && !filters
            .required_labels
            .iter()
            .all(|label| record.scope.labels.iter().any(|value| value == label))
    {
        return false;
    }
    if let Some(source) = &filters.source
        && &record.scope.source != source
    {
        return false;
    }
    if let Some(from_unix_ms) = filters.from_unix_ms
        && record.updated_at_unix_ms < from_unix_ms
    {
        return false;
    }
    if let Some(to_unix_ms) = filters.to_unix_ms
        && record.updated_at_unix_ms > to_unix_ms
    {
        return false;
    }
    if let Some(min_importance_score) = filters.min_importance_score
        && record.importance_score < min_importance_score
    {
        return false;
    }
    if !filters.trust_levels.is_empty() && !filters.trust_levels.contains(&record.scope.trust_level)
    {
        return false;
    }
    if !filters.states.is_empty() && !filters.states.contains(&record.quality_state) {
        return false;
    }
    if !filters.include_archived && matches!(record.quality_state, MemoryQualityState::Archived) {
        return false;
    }
    if let Some(episode_id) = &filters.episode_id
        && record.episode.as_ref().map(|episode| &episode.episode_id) != Some(episode_id)
    {
        return false;
    }
    if !filters.continuity_states.is_empty()
        && !record.episode.as_ref().is_some_and(|episode| {
            filters
                .continuity_states
                .contains(&episode.continuity_state)
        })
    {
        return false;
    }
    if filters.unresolved_only
        && !record
            .episode
            .as_ref()
            .is_some_and(|episode| episode.continuity_state.is_unresolved())
    {
        return false;
    }
    if let Some(lineage_record_id) = &filters.lineage_record_id
        && record.id != *lineage_record_id
        && !record
            .lineage
            .iter()
            .any(|link| &link.record_id == lineage_record_id)
    {
        return false;
    }

    match filters.historical_mode {
        RecallHistoricalMode::CurrentOnly => {
            if !matches!(record.historical_state, MemoryHistoricalState::Current) {
                return false;
            }
        }
        RecallHistoricalMode::HistoricalOnly => {
            if matches!(record.historical_state, MemoryHistoricalState::Current) {
                return false;
            }
        }
        RecallHistoricalMode::IncludeHistorical => {}
    }

    true
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
                source_id: record.source_id,
                artifact: None,
                episode: map_episode(record.episode),
                historical_state: map_historical_state(&record.historical_state),
                lineage: map_lineage(record.lineage),
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
            include_explanation: false,
        };

        let mut ranked = records
            .iter()
            .filter(|record| record_matches_filters(record, &query))
            .filter_map(|record| scorer.score(record, &query))
            .collect::<Vec<_>>();
        match query.filters.temporal_order {
            RecallTemporalOrder::Relevance if query.query_text.trim().is_empty() => {
                ranked.sort_by(|left, right| {
                    record_temporal_anchor(&right.hit.record)
                        .cmp(&record_temporal_anchor(&left.hit.record))
                        .then_with(|| {
                            right
                                .hit
                                .record
                                .importance_score
                                .total_cmp(&left.hit.record.importance_score)
                        })
                        .then_with(|| left.hit.record.id.cmp(&right.hit.record.id))
                });
            }
            RecallTemporalOrder::Relevance => {
                ranked.sort_by(|left, right| {
                    right
                        .hit
                        .breakdown
                        .total
                        .total_cmp(&left.hit.breakdown.total)
                        .then_with(|| left.hit.record.id.cmp(&right.hit.record.id))
                });
            }
            RecallTemporalOrder::ChronologicalAsc => {
                ranked.sort_by(|left, right| {
                    record_temporal_anchor(&left.hit.record)
                        .cmp(&record_temporal_anchor(&right.hit.record))
                        .then_with(|| {
                            right
                                .hit
                                .breakdown
                                .total
                                .total_cmp(&left.hit.breakdown.total)
                        })
                        .then_with(|| left.hit.record.id.cmp(&right.hit.record.id))
                });
            }
            RecallTemporalOrder::ChronologicalDesc => {
                ranked.sort_by(|left, right| {
                    record_temporal_anchor(&right.hit.record)
                        .cmp(&record_temporal_anchor(&left.hit.record))
                        .then_with(|| {
                            right
                                .hit
                                .breakdown
                                .total
                                .total_cmp(&left.hit.breakdown.total)
                        })
                        .then_with(|| left.hit.record.id.cmp(&right.hit.record.id))
                });
            }
        }
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

    assert_eq!(overall.cases, 18);
    assert!(overall.hit_rate_at_k >= 1.0);
    assert!(overall.recall_at_k >= 0.88);
    assert!(overall.mrr >= 0.88);
    assert!(overall.ndcg_at_k >= 0.9);

    let required_scenarios = BTreeSet::from([
        "chronology_reconstruction",
        "continuity_unresolved",
        "contradiction_handling",
        "preference_change",
        "operational_drift",
        "long_horizon_task",
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
