use crate::config::{
    EngineConfig, RecallPlanningProfile, RecallPolicyProfile, RecallScorerKind,
    RecallScoringProfile,
};
use crate::embedding::{ConfiguredSemanticEmbedder, SemanticEmbedder};
use crate::model::{
    EpisodeContext, MemoryHistoricalState, MemoryQualityState, MemoryRecord, MemoryTrustLevel,
};
use crate::query::{
    RecallCandidateSource, RecallHit, RecallPlannerStage, RecallQuery, RecallScoreBreakdown,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq)]
pub struct ScoredRecallCandidate {
    pub hit: RecallHit,
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedRecallCandidate {
    pub hit: RecallHit,
    pub matched_terms: Vec<String>,
    pub candidate_sources: Vec<RecallCandidateSource>,
    pub planner_stage: RecallPlannerStage,
}

#[derive(Debug, Clone)]
pub struct RecallPlanner {
    profile: RecallPlanningProfile,
    graph_expansion_max_hops: u8,
    scorer: ConfiguredRecallScorer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecallPlannerMetrics {
    pub candidate_generation_ns: u128,
    pub graph_expansion_ns: u128,
    pub total_ns: u128,
    pub seeded_candidates: usize,
    pub expanded_candidates: usize,
    pub hops_applied: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum GraphExpansionRelation {
    EpisodeMembership,
    Chronology,
    Causal,
    Related,
    Lineage,
}

pub trait RecallScorer: Send + Sync {
    fn score(&self, record: &MemoryRecord, query: &RecallQuery) -> Option<ScoredRecallCandidate>;
    fn scorer_kind(&self) -> RecallScorerKind;
    fn scoring_profile(&self) -> RecallScoringProfile;
    fn policy_profile(&self) -> RecallPolicyProfile;
    fn profile_note(&self) -> &'static str;
    fn policy_profile_note(&self) -> &'static str;
    fn embedding_note(&self) -> Option<String>;
}

#[derive(Debug, Clone, Copy)]
struct ProvenancePolicyWeights {
    source_bonus: f32,
    pinned_bonus: f32,
    verified_bonus: f32,
    observed_bonus: f32,
    derived_bonus: f32,
    untrusted_penalty: f32,
    verified_state_bonus: f32,
    active_state_bonus: f32,
    draft_penalty: f32,
    archived_penalty_fast_path: f32,
    archived_penalty_continuity: f32,
    suppressed_penalty: f32,
    deleted_penalty: f32,
    current_bonus: f32,
    historical_bonus: f32,
    historical_penalty: f32,
    superseded_bonus: f32,
    superseded_penalty: f32,
}

fn provenance_policy_weights(profile: RecallPolicyProfile) -> ProvenancePolicyWeights {
    match profile {
        RecallPolicyProfile::General => ProvenancePolicyWeights {
            source_bonus: 0.12,
            pinned_bonus: 0.14,
            verified_bonus: 0.09,
            observed_bonus: 0.03,
            derived_bonus: 0.0,
            untrusted_penalty: -0.08,
            verified_state_bonus: 0.08,
            active_state_bonus: 0.03,
            draft_penalty: -0.03,
            archived_penalty_fast_path: -0.08,
            archived_penalty_continuity: -0.02,
            suppressed_penalty: -0.2,
            deleted_penalty: -0.3,
            current_bonus: 0.06,
            historical_bonus: 0.04,
            historical_penalty: -0.05,
            superseded_bonus: -0.02,
            superseded_penalty: -0.12,
        },
        RecallPolicyProfile::Support => ProvenancePolicyWeights {
            source_bonus: 0.16,
            pinned_bonus: 0.2,
            verified_bonus: 0.13,
            observed_bonus: 0.01,
            derived_bonus: -0.03,
            untrusted_penalty: -0.14,
            verified_state_bonus: 0.12,
            active_state_bonus: 0.05,
            draft_penalty: -0.06,
            archived_penalty_fast_path: -0.12,
            archived_penalty_continuity: -0.05,
            suppressed_penalty: -0.28,
            deleted_penalty: -0.4,
            current_bonus: 0.09,
            historical_bonus: 0.01,
            historical_penalty: -0.09,
            superseded_bonus: -0.05,
            superseded_penalty: -0.16,
        },
        RecallPolicyProfile::Research => ProvenancePolicyWeights {
            source_bonus: 0.04,
            pinned_bonus: 0.1,
            verified_bonus: 0.07,
            observed_bonus: 0.04,
            derived_bonus: 0.03,
            untrusted_penalty: -0.04,
            verified_state_bonus: 0.05,
            active_state_bonus: 0.02,
            draft_penalty: -0.01,
            archived_penalty_fast_path: -0.02,
            archived_penalty_continuity: 0.01,
            suppressed_penalty: -0.12,
            deleted_penalty: -0.18,
            current_bonus: 0.03,
            historical_bonus: 0.08,
            historical_penalty: -0.01,
            superseded_bonus: 0.02,
            superseded_penalty: -0.04,
        },
        RecallPolicyProfile::Assistant => ProvenancePolicyWeights {
            source_bonus: 0.1,
            pinned_bonus: 0.13,
            verified_bonus: 0.1,
            observed_bonus: 0.03,
            derived_bonus: 0.01,
            untrusted_penalty: -0.07,
            verified_state_bonus: 0.09,
            active_state_bonus: 0.04,
            draft_penalty: -0.03,
            archived_penalty_fast_path: -0.06,
            archived_penalty_continuity: -0.01,
            suppressed_penalty: -0.18,
            deleted_penalty: -0.28,
            current_bonus: 0.07,
            historical_bonus: 0.04,
            historical_penalty: -0.03,
            superseded_bonus: -0.01,
            superseded_penalty: -0.1,
        },
        RecallPolicyProfile::AutonomousAgent => ProvenancePolicyWeights {
            source_bonus: 0.14,
            pinned_bonus: 0.18,
            verified_bonus: 0.12,
            observed_bonus: 0.02,
            derived_bonus: 0.0,
            untrusted_penalty: -0.1,
            verified_state_bonus: 0.1,
            active_state_bonus: 0.05,
            draft_penalty: -0.04,
            archived_penalty_fast_path: -0.08,
            archived_penalty_continuity: -0.02,
            suppressed_penalty: -0.24,
            deleted_penalty: -0.36,
            current_bonus: 0.08,
            historical_bonus: 0.03,
            historical_penalty: -0.06,
            superseded_bonus: -0.03,
            superseded_penalty: -0.14,
        },
    }
}

fn semantic_similarity(
    embedder: &ConfiguredSemanticEmbedder,
    query_text: &str,
    haystack: &str,
) -> f32 {
    if embedder.dimensions() == 0 {
        return 0.0;
    }

    embedder
        .embed(query_text)
        .cosine_similarity(&embedder.embed(haystack))
        .max(0.0)
}

fn episode_haystack(episode: &EpisodeContext) -> String {
    format!(
        "{} {} {} {} {} {} {} {}",
        episode.summary.clone().unwrap_or_default(),
        episode.goal.clone().unwrap_or_default(),
        episode.outcome.clone().unwrap_or_default(),
        episode.actor_ids.join(" "),
        episode.recurrence_key.clone().unwrap_or_default(),
        episode.boundary_label.clone().unwrap_or_default(),
        episode.related_record_ids.join(" "),
        episode.linked_artifact_uris.join(" "),
    )
    .to_ascii_lowercase()
}

fn has_sequence_intent(query_text: &str) -> bool {
    let lowered = query_text.to_ascii_lowercase();
    ["timeline", "before", "after", "first", "last", "sequence"]
        .iter()
        .any(|needle| lowered.contains(needle))
}

fn has_duration_intent(query_text: &str) -> bool {
    let lowered = query_text.to_ascii_lowercase();
    [
        "how long",
        "duration",
        "lasting",
        "long-running",
        "long running",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

fn has_recurrence_intent(query_text: &str) -> bool {
    let lowered = query_text.to_ascii_lowercase();
    ["recurring", "repeat", "repeats", "again", "every ", "habit"]
        .iter()
        .any(|needle| lowered.contains(needle))
}

fn has_boundary_intent(query_text: &str) -> bool {
    let lowered = query_text.to_ascii_lowercase();
    ["boundary", "handoff", "checkpoint", "session", "window"]
        .iter()
        .any(|needle| lowered.contains(needle))
}

fn temporal_context_bonus(record: &MemoryRecord, query: &RecallQuery) -> f32 {
    let Some(episode) = record.episode.as_ref() else {
        return 0.0;
    };

    let mut score = 0.0;
    if has_sequence_intent(&query.query_text)
        && (episode.previous_record_id.is_some()
            || episode.next_record_id.is_some()
            || !episode.causal_record_ids.is_empty())
    {
        score += 0.2;
    }
    if has_duration_intent(&query.query_text) {
        if let Some(duration_ms) = episode.duration_hint_ms() {
            let normalized_hours = (duration_ms as f32 / (60.0 * 60.0 * 1000.0)).min(24.0) / 24.0;
            score += 0.15 + (normalized_hours * 0.2);
        }
    }
    if has_recurrence_intent(&query.query_text) {
        if episode.recurrence_key.is_some() {
            score += 0.2;
        }
        if let Some(interval_ms) = episode.recurrence_interval_ms {
            let normalized_days =
                (interval_ms as f32 / (24.0 * 60.0 * 60.0 * 1000.0)).min(30.0) / 30.0;
            score += 0.1 + (normalized_days * 0.1);
        }
    }
    if has_boundary_intent(&query.query_text) {
        if episode.boundary_label.is_some() {
            score += 0.18;
        }
        if record.scope.session_id.is_some() || record.scope.conversation_id.is_some() {
            score += 0.08;
        }
    }
    score
}

fn episodic_score(record: &MemoryRecord, query: &RecallQuery, query_terms: &[String]) -> f32 {
    let Some(episode) = record.episode.as_ref() else {
        return 0.0;
    };

    let mut score = 0.0;
    if let Some(filter_episode_id) = &query.filters.episode_id
        && &episode.episode_id == filter_episode_id
    {
        score += 1.0;
    }

    if query.filters.unresolved_only && episode.continuity_state.is_unresolved() {
        score += 0.35;
    }

    let haystack = episode_haystack(episode);
    let term_matches = query_terms
        .iter()
        .filter(|term| haystack.contains(term.as_str()))
        .count() as f32;
    score += term_matches * 0.2;

    let lowered_query = query.query_text.to_ascii_lowercase();
    if lowered_query.contains("what changed") && episode.previous_record_id.is_some() {
        score += 0.25;
    }
    if lowered_query.contains("what happened next") && episode.next_record_id.is_some() {
        score += 0.25;
    }
    if lowered_query.contains("what led") && !episode.causal_record_ids.is_empty() {
        score += 0.25;
    }

    score
}

fn salience_score(record: &MemoryRecord) -> f32 {
    let Some(episode) = record.episode.as_ref() else {
        return 0.0;
    };

    let mut score = ((episode.salience.reuse_count.min(10) as f32) / 10.0) * 0.35
        + episode.salience.novelty_score.clamp(0.0, 1.0) * 0.15
        + episode.salience.goal_relevance.clamp(0.0, 1.0) * 0.35
        + episode.salience.unresolved_weight.clamp(0.0, 1.0) * 0.25;
    if let Some(affective) = &episode.affective {
        score += affective.urgency.clamp(0.0, 1.0) * 0.1;
        score += affective.tension.clamp(0.0, 1.0) * 0.05;
    }
    score
}

fn tokenize_query(query_text: &str) -> Vec<String> {
    query_text
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .collect()
}

fn has_continuity_intent(query_text: &str) -> bool {
    let lowered = query_text.to_ascii_lowercase();
    [
        "what led",
        "what changed",
        "what happened next",
        "unresolved",
        "follow up",
        "follow-up",
        "same episode",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

fn scope_matches_query(record: &MemoryRecord, query: &RecallQuery) -> bool {
    if record.scope.tenant_id != query.scope.tenant_id
        || record.scope.namespace != query.scope.namespace
        || record.scope.actor_id != query.scope.actor_id
    {
        return false;
    }

    if query.scope.conversation_id.is_some()
        && record.scope.conversation_id != query.scope.conversation_id
    {
        return false;
    }

    if query.scope.session_id.is_some() && record.scope.session_id != query.scope.session_id {
        return false;
    }

    true
}

fn graph_relations(
    record: &MemoryRecord,
    frontier_ids: &BTreeSet<String>,
    frontier_episode_ids: &BTreeSet<String>,
    profile: RecallPlanningProfile,
) -> Vec<GraphExpansionRelation> {
    if !matches!(profile, RecallPlanningProfile::ContinuityAware) {
        return Vec::new();
    }

    let mut relations = Vec::new();
    if let Some(episode) = &record.episode {
        if frontier_episode_ids.contains(&episode.episode_id) {
            relations.push(GraphExpansionRelation::EpisodeMembership);
        }
        if episode
            .previous_record_id
            .as_ref()
            .is_some_and(|value| frontier_ids.contains(value))
            || episode
                .next_record_id
                .as_ref()
                .is_some_and(|value| frontier_ids.contains(value))
        {
            relations.push(GraphExpansionRelation::Chronology);
        }
        if episode
            .causal_record_ids
            .iter()
            .any(|value| frontier_ids.contains(value))
        {
            relations.push(GraphExpansionRelation::Causal);
        }
        if episode
            .related_record_ids
            .iter()
            .any(|value| frontier_ids.contains(value))
        {
            relations.push(GraphExpansionRelation::Related);
        }
    }
    if record
        .lineage
        .iter()
        .any(|link| frontier_ids.contains(&link.record_id))
    {
        relations.push(GraphExpansionRelation::Lineage);
    }
    relations.sort();
    relations.dedup();
    relations
}

fn graph_relation_bonus(relations: &[GraphExpansionRelation], hop: u8) -> f32 {
    let strongest = relations
        .iter()
        .map(|relation| match relation {
            GraphExpansionRelation::EpisodeMembership => 0.45,
            GraphExpansionRelation::Chronology => 0.35,
            GraphExpansionRelation::Causal => 0.32,
            GraphExpansionRelation::Related => 0.24,
            GraphExpansionRelation::Lineage => 0.28,
        })
        .fold(0.0, f32::max);
    let stacked_bonus = relations.len().saturating_sub(1) as f32 * 0.04;
    let hop_penalty = hop.saturating_sub(1) as f32 * 0.08;
    (strongest + stacked_bonus - hop_penalty).max(0.0)
}

fn seed_candidate_sources(
    hit: &RecallHit,
    matched_terms: &[String],
    empty_query: bool,
) -> Vec<RecallCandidateSource> {
    let mut sources = Vec::new();
    if !matched_terms.is_empty() {
        sources.push(RecallCandidateSource::Lexical);
    }
    if hit.breakdown.semantic > 0.0 {
        sources.push(RecallCandidateSource::Semantic);
    }
    if hit.breakdown.metadata > 0.0 {
        sources.push(RecallCandidateSource::Metadata);
    }
    if hit.breakdown.episodic > 0.0 {
        sources.push(RecallCandidateSource::Episode);
    }
    if empty_query || hit.breakdown.temporal > 0.0 {
        sources.push(RecallCandidateSource::Temporal);
    }
    sources.sort();
    sources.dedup();
    sources
}

fn provenance_bonus(
    record: &MemoryRecord,
    query: &RecallQuery,
    profile: RecallPlanningProfile,
    policy_profile: RecallPolicyProfile,
) -> f32 {
    let weights = provenance_policy_weights(policy_profile);
    let source_bonus = if record.scope.source == query.scope.source {
        weights.source_bonus
    } else {
        0.0
    };
    let trust_bonus = match record.scope.trust_level {
        MemoryTrustLevel::Pinned => weights.pinned_bonus,
        MemoryTrustLevel::Verified => weights.verified_bonus,
        MemoryTrustLevel::Observed => weights.observed_bonus,
        MemoryTrustLevel::Derived => weights.derived_bonus,
        MemoryTrustLevel::Untrusted => weights.untrusted_penalty,
    };
    let quality_bonus = match record.quality_state {
        MemoryQualityState::Verified => weights.verified_state_bonus,
        MemoryQualityState::Active => weights.active_state_bonus,
        MemoryQualityState::Draft => weights.draft_penalty,
        MemoryQualityState::Archived => {
            if matches!(profile, RecallPlanningProfile::ContinuityAware) {
                weights.archived_penalty_continuity
            } else {
                weights.archived_penalty_fast_path
            }
        }
        MemoryQualityState::Suppressed => weights.suppressed_penalty,
        MemoryQualityState::Deleted => weights.deleted_penalty,
    };
    let historical_bonus = match record.historical_state {
        MemoryHistoricalState::Current => weights.current_bonus,
        MemoryHistoricalState::Historical => {
            if matches!(
                query.filters.historical_mode,
                crate::query::RecallHistoricalMode::HistoricalOnly
            ) {
                weights.historical_bonus
            } else {
                weights.historical_penalty
            }
        }
        MemoryHistoricalState::Superseded => {
            if matches!(
                query.filters.historical_mode,
                crate::query::RecallHistoricalMode::HistoricalOnly
            ) {
                weights.superseded_bonus
            } else {
                weights.superseded_penalty
            }
        }
    };
    source_bonus + trust_bonus + quality_bonus + historical_bonus
}

#[derive(Debug, Clone)]
pub struct ProfileRecallScorer {
    profile: RecallScoringProfile,
    policy_profile: RecallPolicyProfile,
    embedder: ConfiguredSemanticEmbedder,
}

impl ProfileRecallScorer {
    pub fn new(profile: RecallScoringProfile) -> Self {
        Self::with_embedder(
            profile,
            RecallPolicyProfile::General,
            ConfiguredSemanticEmbedder::Disabled(Default::default()),
        )
    }

    pub fn with_embedder(
        profile: RecallScoringProfile,
        policy_profile: RecallPolicyProfile,
        embedder: ConfiguredSemanticEmbedder,
    ) -> Self {
        Self {
            profile,
            policy_profile,
            embedder,
        }
    }

    pub fn with_shared_embedder(
        profile: RecallScoringProfile,
        policy_profile: RecallPolicyProfile,
        embedder: Arc<dyn SemanticEmbedder>,
        provider_note: impl Into<String>,
    ) -> Self {
        Self::with_embedder(
            profile,
            policy_profile,
            ConfiguredSemanticEmbedder::shared(embedder, provider_note),
        )
    }

    pub fn profile(&self) -> RecallScoringProfile {
        self.profile
    }

    fn weights(&self) -> (f32, f32, f32) {
        match self.profile {
            RecallScoringProfile::Balanced => (1.0, 0.45, 1.0),
            RecallScoringProfile::LexicalFirst => (1.25, 0.25, 0.35),
            RecallScoringProfile::ImportanceFirst => (0.75, 0.35, 1.5),
        }
    }

    fn metadata_score(&self, record: &MemoryRecord, query_terms: &[String]) -> f32 {
        let source = record.scope.source.to_ascii_lowercase();
        let label_matches = record
            .scope
            .labels
            .iter()
            .map(|label| label.to_ascii_lowercase())
            .filter(|label| query_terms.iter().any(|term| label.contains(term)))
            .count() as f32;
        let source_bonus = if query_terms.iter().any(|term| source.contains(term)) {
            0.5
        } else {
            0.0
        };
        label_matches + source_bonus
    }

    fn temporal_score(&self, record: &MemoryRecord, query: &RecallQuery) -> f32 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(record.updated_at_unix_ms);
        let freshness_window_ms = 7 * 24 * 60 * 60 * 1000u64;
        let age_ms = now
            .saturating_sub(record.updated_at_unix_ms)
            .min(freshness_window_ms) as f32;
        (1.0 - (age_ms / freshness_window_ms as f32)) + temporal_context_bonus(record, query)
    }
}

impl RecallScorer for ProfileRecallScorer {
    fn score(&self, record: &MemoryRecord, query: &RecallQuery) -> Option<ScoredRecallCandidate> {
        let (lexical_weight, semantic_weight, policy_weight) = self.weights();

        if query.query_text.trim().is_empty() {
            let episodic = episodic_score(record, query, &[]);
            let salience = salience_score(record);
            return Some(ScoredRecallCandidate {
                hit: RecallHit {
                    record: record.clone(),
                    breakdown: RecallScoreBreakdown {
                        lexical: 0.0,
                        semantic: 0.0,
                        graph: 0.0,
                        temporal: 1.0,
                        metadata: 0.0,
                        episodic,
                        salience,
                        curation: 0.0,
                        policy: record.importance_score,
                        total: 1.0
                            + (episodic * 0.35)
                            + (salience * 0.25)
                            + (record.importance_score * policy_weight),
                    },
                    explanation: None,
                },
                matched_terms: Vec::new(),
            });
        }

        let query_terms = query
            .query_text
            .split_whitespace()
            .map(|term| term.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let haystack = format!(
            "{} {}",
            record.content.to_ascii_lowercase(),
            record
                .summary
                .clone()
                .unwrap_or_default()
                .to_ascii_lowercase()
        );
        let semantic = semantic_similarity(&self.embedder, &query.query_text, &haystack);
        let matched_terms = query_terms
            .iter()
            .filter(|term| haystack.contains(term.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let lexical = matched_terms.len() as f32;
        if lexical == 0.0 && semantic == 0.0 {
            return None;
        }

        let metadata = self.metadata_score(record, &query_terms);
        let temporal = self.temporal_score(record, query);
        let episodic = episodic_score(record, query, &query_terms);
        let salience = salience_score(record);
        let policy = record.importance_score;
        let total = (lexical * lexical_weight)
            + (semantic * semantic_weight)
            + (metadata * 0.4)
            + (temporal * 0.2)
            + (episodic * 0.35)
            + (salience * 0.25)
            + (policy * policy_weight);
        Some(ScoredRecallCandidate {
            hit: RecallHit {
                record: record.clone(),
                breakdown: RecallScoreBreakdown {
                    lexical,
                    semantic,
                    graph: 0.0,
                    temporal,
                    metadata,
                    episodic,
                    salience,
                    curation: 0.0,
                    policy,
                    total,
                },
                explanation: None,
            },
            matched_terms,
        })
    }

    fn scorer_kind(&self) -> RecallScorerKind {
        RecallScorerKind::Profile
    }

    fn scoring_profile(&self) -> RecallScoringProfile {
        self.profile
    }

    fn policy_profile(&self) -> RecallPolicyProfile {
        self.policy_profile
    }

    fn profile_note(&self) -> &'static str {
        match self.profile {
            RecallScoringProfile::Balanced => "scoring_profile=balanced",
            RecallScoringProfile::LexicalFirst => "scoring_profile=lexical_first",
            RecallScoringProfile::ImportanceFirst => "scoring_profile=importance_first",
        }
    }

    fn policy_profile_note(&self) -> &'static str {
        match self.policy_profile {
            RecallPolicyProfile::General => "policy_profile=general",
            RecallPolicyProfile::Support => "policy_profile=support",
            RecallPolicyProfile::Research => "policy_profile=research",
            RecallPolicyProfile::Assistant => "policy_profile=assistant",
            RecallPolicyProfile::AutonomousAgent => "policy_profile=autonomous_agent",
        }
    }

    fn embedding_note(&self) -> Option<String> {
        self.embedder.provider_note()
    }
}

#[derive(Debug, Clone)]
pub struct CuratedRecallScorer {
    profile: RecallScoringProfile,
    policy_profile: RecallPolicyProfile,
    embedder: ConfiguredSemanticEmbedder,
}

impl CuratedRecallScorer {
    pub fn new(profile: RecallScoringProfile) -> Self {
        Self::with_embedder(
            profile,
            RecallPolicyProfile::General,
            ConfiguredSemanticEmbedder::Disabled(Default::default()),
        )
    }

    pub fn with_embedder(
        profile: RecallScoringProfile,
        policy_profile: RecallPolicyProfile,
        embedder: ConfiguredSemanticEmbedder,
    ) -> Self {
        Self {
            profile,
            policy_profile,
            embedder,
        }
    }

    pub fn with_shared_embedder(
        profile: RecallScoringProfile,
        policy_profile: RecallPolicyProfile,
        embedder: Arc<dyn SemanticEmbedder>,
        provider_note: impl Into<String>,
    ) -> Self {
        Self::with_embedder(
            profile,
            policy_profile,
            ConfiguredSemanticEmbedder::shared(embedder, provider_note),
        )
    }

    fn weights(&self) -> (f32, f32, f32) {
        match self.profile {
            RecallScoringProfile::Balanced => (1.0, 0.50, 1.0),
            RecallScoringProfile::LexicalFirst => (1.15, 0.30, 0.65),
            RecallScoringProfile::ImportanceFirst => (0.75, 0.40, 1.7),
        }
    }

    fn curation_bonus(&self, record: &MemoryRecord) -> f32 {
        let trust_bonus = match record.scope.trust_level {
            MemoryTrustLevel::Pinned => 0.45,
            MemoryTrustLevel::Verified => 0.25,
            MemoryTrustLevel::Observed => 0.10,
            MemoryTrustLevel::Derived => 0.05,
            MemoryTrustLevel::Untrusted => -0.10,
        };
        let quality_bonus = match record.quality_state {
            MemoryQualityState::Verified => 0.20,
            MemoryQualityState::Active => 0.0,
            MemoryQualityState::Draft => -0.05,
            MemoryQualityState::Archived => -0.15,
            MemoryQualityState::Suppressed => -0.40,
            MemoryQualityState::Deleted => -0.50,
        };
        trust_bonus + quality_bonus
    }

    fn metadata_score(&self, record: &MemoryRecord, query_terms: &[String]) -> f32 {
        let source = record.scope.source.to_ascii_lowercase();
        let label_hits = record
            .scope
            .labels
            .iter()
            .map(|label| label.to_ascii_lowercase())
            .filter(|label| query_terms.iter().any(|term| label.contains(term)))
            .count() as f32;
        let source_hits = query_terms
            .iter()
            .filter(|term| source.contains(term.as_str()))
            .count() as f32;
        label_hits + (source_hits * 0.5)
    }

    fn temporal_score(&self, record: &MemoryRecord, query: &RecallQuery) -> f32 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(record.updated_at_unix_ms);
        let freshness_window_ms = 30 * 24 * 60 * 60 * 1000u64;
        let age_ms = now
            .saturating_sub(record.updated_at_unix_ms)
            .min(freshness_window_ms) as f32;
        (1.0 - (age_ms / freshness_window_ms as f32)) + temporal_context_bonus(record, query)
    }
}

impl RecallScorer for CuratedRecallScorer {
    fn score(&self, record: &MemoryRecord, query: &RecallQuery) -> Option<ScoredRecallCandidate> {
        let (lexical_weight, semantic_weight, policy_weight) = self.weights();
        let curated_policy = (record.importance_score + self.curation_bonus(record)).max(0.0);

        if query.query_text.trim().is_empty() {
            let episodic = episodic_score(record, query, &[]);
            let salience = salience_score(record);
            return Some(ScoredRecallCandidate {
                hit: RecallHit {
                    record: record.clone(),
                    breakdown: RecallScoreBreakdown {
                        lexical: 0.0,
                        semantic: 0.0,
                        graph: 0.0,
                        temporal: 1.0,
                        metadata: 0.0,
                        episodic,
                        salience,
                        curation: self.curation_bonus(record),
                        policy: curated_policy,
                        total: 1.0
                            + (episodic * 0.35)
                            + (salience * 0.25)
                            + (curated_policy * policy_weight),
                    },
                    explanation: None,
                },
                matched_terms: Vec::new(),
            });
        }

        let query_terms = query
            .query_text
            .split_whitespace()
            .map(|term| term.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let haystack = format!(
            "{} {} {}",
            record.content.to_ascii_lowercase(),
            record
                .summary
                .clone()
                .unwrap_or_default()
                .to_ascii_lowercase(),
            record.scope.source.to_ascii_lowercase(),
        );
        let semantic = semantic_similarity(&self.embedder, &query.query_text, &haystack);
        let matched_terms = query_terms
            .iter()
            .filter(|term| haystack.contains(term.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let lexical = matched_terms.len() as f32;
        if lexical == 0.0 && semantic == 0.0 {
            return None;
        }

        let metadata = self.metadata_score(record, &query_terms);
        let temporal = self.temporal_score(record, query);
        let episodic = episodic_score(record, query, &query_terms);
        let salience = salience_score(record);
        let curation = self.curation_bonus(record);
        let total = (lexical * lexical_weight)
            + (semantic * semantic_weight)
            + (metadata * 0.45)
            + (temporal * 0.2)
            + (episodic * 0.35)
            + (salience * 0.25)
            + (curation * 0.5)
            + (curated_policy * policy_weight);
        Some(ScoredRecallCandidate {
            hit: RecallHit {
                record: record.clone(),
                breakdown: RecallScoreBreakdown {
                    lexical,
                    semantic,
                    graph: 0.0,
                    temporal,
                    metadata,
                    episodic,
                    salience,
                    curation,
                    policy: curated_policy,
                    total,
                },
                explanation: None,
            },
            matched_terms,
        })
    }

    fn scorer_kind(&self) -> RecallScorerKind {
        RecallScorerKind::Curated
    }

    fn scoring_profile(&self) -> RecallScoringProfile {
        self.profile
    }

    fn policy_profile(&self) -> RecallPolicyProfile {
        self.policy_profile
    }

    fn profile_note(&self) -> &'static str {
        match self.profile {
            RecallScoringProfile::Balanced => "scoring_profile=balanced",
            RecallScoringProfile::LexicalFirst => "scoring_profile=lexical_first",
            RecallScoringProfile::ImportanceFirst => "scoring_profile=importance_first",
        }
    }

    fn policy_profile_note(&self) -> &'static str {
        match self.policy_profile {
            RecallPolicyProfile::General => "policy_profile=general",
            RecallPolicyProfile::Support => "policy_profile=support",
            RecallPolicyProfile::Research => "policy_profile=research",
            RecallPolicyProfile::Assistant => "policy_profile=assistant",
            RecallPolicyProfile::AutonomousAgent => "policy_profile=autonomous_agent",
        }
    }

    fn embedding_note(&self) -> Option<String> {
        self.embedder.provider_note()
    }
}

#[derive(Debug, Clone)]
pub enum ConfiguredRecallScorer {
    Profile(ProfileRecallScorer),
    Curated(CuratedRecallScorer),
}

impl ConfiguredRecallScorer {
    pub fn from_engine_config(config: &EngineConfig) -> Self {
        let embedder = ConfiguredSemanticEmbedder::from_engine_config(config);
        match config.recall_scorer_kind {
            RecallScorerKind::Profile => Self::Profile(ProfileRecallScorer::with_embedder(
                config.recall_scoring_profile,
                config.recall_policy_profile,
                embedder,
            )),
            RecallScorerKind::Curated => Self::Curated(CuratedRecallScorer::with_embedder(
                config.recall_scoring_profile,
                config.recall_policy_profile,
                embedder,
            )),
        }
    }

    pub fn with_embedder(
        scorer_kind: RecallScorerKind,
        scoring_profile: RecallScoringProfile,
        policy_profile: RecallPolicyProfile,
        embedder: ConfiguredSemanticEmbedder,
    ) -> Self {
        match scorer_kind {
            RecallScorerKind::Profile => Self::Profile(ProfileRecallScorer::with_embedder(
                scoring_profile,
                policy_profile,
                embedder,
            )),
            RecallScorerKind::Curated => Self::Curated(CuratedRecallScorer::with_embedder(
                scoring_profile,
                policy_profile,
                embedder,
            )),
        }
    }

    pub fn with_shared_embedder(
        scorer_kind: RecallScorerKind,
        scoring_profile: RecallScoringProfile,
        policy_profile: RecallPolicyProfile,
        embedder: Arc<dyn SemanticEmbedder>,
        provider_note: impl Into<String>,
    ) -> Self {
        Self::with_embedder(
            scorer_kind,
            scoring_profile,
            policy_profile,
            ConfiguredSemanticEmbedder::shared(embedder, provider_note),
        )
    }

    fn curation_bonus_for(&self, record: &MemoryRecord) -> f32 {
        match self {
            Self::Profile(_) => 0.0,
            Self::Curated(scorer) => scorer.curation_bonus(record),
        }
    }

    fn temporal_score_for(&self, record: &MemoryRecord, query: &RecallQuery) -> f32 {
        match self {
            Self::Profile(scorer) => scorer.temporal_score(record, query),
            Self::Curated(scorer) => scorer.temporal_score(record, query),
        }
    }
}

impl RecallScorer for ConfiguredRecallScorer {
    fn score(&self, record: &MemoryRecord, query: &RecallQuery) -> Option<ScoredRecallCandidate> {
        match self {
            Self::Profile(scorer) => scorer.score(record, query),
            Self::Curated(scorer) => scorer.score(record, query),
        }
    }

    fn scorer_kind(&self) -> RecallScorerKind {
        match self {
            Self::Profile(scorer) => scorer.scorer_kind(),
            Self::Curated(scorer) => scorer.scorer_kind(),
        }
    }

    fn scoring_profile(&self) -> RecallScoringProfile {
        match self {
            Self::Profile(scorer) => scorer.scoring_profile(),
            Self::Curated(scorer) => scorer.scoring_profile(),
        }
    }

    fn policy_profile(&self) -> RecallPolicyProfile {
        match self {
            Self::Profile(scorer) => scorer.policy_profile(),
            Self::Curated(scorer) => scorer.policy_profile(),
        }
    }

    fn profile_note(&self) -> &'static str {
        match self {
            Self::Profile(scorer) => scorer.profile_note(),
            Self::Curated(scorer) => scorer.profile_note(),
        }
    }

    fn policy_profile_note(&self) -> &'static str {
        match self {
            Self::Profile(scorer) => scorer.policy_profile_note(),
            Self::Curated(scorer) => scorer.policy_profile_note(),
        }
    }

    fn embedding_note(&self) -> Option<String> {
        match self {
            Self::Profile(scorer) => scorer.embedding_note(),
            Self::Curated(scorer) => scorer.embedding_note(),
        }
    }
}

impl RecallPlanner {
    pub fn from_engine_config(config: &EngineConfig) -> Self {
        Self {
            profile: config.recall_planning_profile,
            graph_expansion_max_hops: config.graph_expansion_max_hops,
            scorer: ConfiguredRecallScorer::from_engine_config(config),
        }
    }

    pub fn with_scorer(
        profile: RecallPlanningProfile,
        graph_expansion_max_hops: u8,
        scorer: ConfiguredRecallScorer,
    ) -> Self {
        Self {
            profile,
            graph_expansion_max_hops,
            scorer,
        }
    }

    pub fn with_shared_embedder(
        profile: RecallPlanningProfile,
        graph_expansion_max_hops: u8,
        scorer_kind: RecallScorerKind,
        scoring_profile: RecallScoringProfile,
        policy_profile: RecallPolicyProfile,
        embedder: Arc<dyn SemanticEmbedder>,
        provider_note: impl Into<String>,
    ) -> Self {
        Self::with_scorer(
            profile,
            graph_expansion_max_hops,
            ConfiguredRecallScorer::with_shared_embedder(
                scorer_kind,
                scoring_profile,
                policy_profile,
                embedder,
                provider_note,
            ),
        )
    }

    pub fn scorer(&self) -> ConfiguredRecallScorer {
        self.scorer.clone()
    }

    pub fn effective_profile(&self, query: &RecallQuery) -> RecallPlanningProfile {
        if matches!(self.profile, RecallPlanningProfile::ContinuityAware)
            || query.filters.episode_id.is_some()
            || query.filters.unresolved_only
            || has_continuity_intent(&query.query_text)
        {
            RecallPlanningProfile::ContinuityAware
        } else {
            RecallPlanningProfile::FastPath
        }
    }

    fn apply_overlay(&self, hit: &mut RecallHit, graph_bonus: f32, provenance_delta: f32) {
        if graph_bonus > 0.0 {
            hit.breakdown.graph += graph_bonus;
            hit.breakdown.total += graph_bonus;
        }
        if provenance_delta != 0.0 {
            hit.breakdown.policy = (hit.breakdown.policy + provenance_delta).max(0.0);
            hit.breakdown.total += provenance_delta;
        }
    }

    fn contextual_candidate(
        &self,
        record: &MemoryRecord,
        query: &RecallQuery,
        query_terms: &[String],
        graph_bonus: f32,
        provenance_delta: f32,
    ) -> PlannedRecallCandidate {
        let temporal = self.scorer.temporal_score_for(record, query);
        let episodic = episodic_score(record, query, query_terms);
        let salience = salience_score(record);
        let curation = self.scorer.curation_bonus_for(record);
        let policy = (record.importance_score + provenance_delta).max(0.0);
        let total = graph_bonus
            + (temporal * 0.15)
            + (episodic * 0.35)
            + (salience * 0.25)
            + (curation * 0.25)
            + (policy * 0.5);

        let mut candidate_sources = vec![RecallCandidateSource::Graph];
        if episodic > 0.0 {
            candidate_sources.push(RecallCandidateSource::Episode);
        }
        if temporal > 0.0 {
            candidate_sources.push(RecallCandidateSource::Temporal);
        }
        if provenance_delta != 0.0 {
            candidate_sources.push(RecallCandidateSource::Provenance);
        }
        candidate_sources.sort();
        candidate_sources.dedup();

        PlannedRecallCandidate {
            hit: RecallHit {
                record: record.clone(),
                breakdown: RecallScoreBreakdown {
                    lexical: 0.0,
                    semantic: 0.0,
                    graph: graph_bonus,
                    temporal,
                    metadata: 0.0,
                    episodic,
                    salience,
                    curation,
                    policy,
                    total,
                },
                explanation: None,
            },
            matched_terms: Vec::new(),
            candidate_sources,
            planner_stage: RecallPlannerStage::GraphExpansion,
        }
    }

    pub fn plan(
        &self,
        records: &[MemoryRecord],
        query: &RecallQuery,
    ) -> Vec<PlannedRecallCandidate> {
        self.plan_with_metrics(records, query).0
    }

    pub fn plan_with_metrics(
        &self,
        records: &[MemoryRecord],
        query: &RecallQuery,
    ) -> (Vec<PlannedRecallCandidate>, RecallPlannerMetrics) {
        let total_started = Instant::now();
        let empty_query = query.query_text.trim().is_empty();
        let effective_profile = self.effective_profile(query);
        let query_terms = tokenize_query(&query.query_text);
        let mut candidates = BTreeMap::<String, PlannedRecallCandidate>::new();

        let candidate_generation_started = Instant::now();

        for record in records {
            if !scope_matches_query(record, query) {
                continue;
            }
            if let Some(scored) = self.scorer.score(record, query) {
                let provenance_delta = provenance_bonus(
                    record,
                    query,
                    effective_profile,
                    self.scorer.policy_profile(),
                );
                let mut hit = scored.hit;
                self.apply_overlay(&mut hit, 0.0, provenance_delta);
                let mut candidate_sources =
                    seed_candidate_sources(&hit, &scored.matched_terms, empty_query);
                if provenance_delta != 0.0 {
                    candidate_sources.push(RecallCandidateSource::Provenance);
                }
                candidate_sources.sort();
                candidate_sources.dedup();
                candidates.insert(
                    record.id.clone(),
                    PlannedRecallCandidate {
                        hit,
                        matched_terms: scored.matched_terms,
                        candidate_sources,
                        planner_stage: RecallPlannerStage::CandidateGeneration,
                    },
                );
            }
        }

        let candidate_generation_ns = candidate_generation_started.elapsed().as_nanos();
        let seeded_candidates = candidates.len();
        let mut graph_expansion_ns = 0;
        let mut expanded_candidates = 0usize;
        let mut hops_applied = 0u8;

        if matches!(effective_profile, RecallPlanningProfile::ContinuityAware)
            && self.graph_expansion_max_hops > 0
        {
            let graph_expansion_started = Instant::now();
            let mut frontier_ids = candidates.keys().cloned().collect::<BTreeSet<_>>();
            let mut frontier_episode_ids = candidates
                .values()
                .filter_map(|candidate| {
                    candidate
                        .hit
                        .record
                        .episode
                        .as_ref()
                        .map(|episode| episode.episode_id.clone())
                })
                .collect::<BTreeSet<_>>();

            for hop in 1..=self.graph_expansion_max_hops {
                if frontier_ids.is_empty() {
                    break;
                }

                let mut next_frontier_ids = BTreeSet::new();
                let mut next_frontier_episode_ids = BTreeSet::new();

                for record in records {
                    if !scope_matches_query(record, query) || candidates.contains_key(&record.id) {
                        continue;
                    }

                    let relations = graph_relations(
                        record,
                        &frontier_ids,
                        &frontier_episode_ids,
                        effective_profile,
                    );
                    if relations.is_empty() {
                        continue;
                    }

                    let graph_bonus = graph_relation_bonus(&relations, hop);
                    if graph_bonus <= 0.0 {
                        continue;
                    }

                    let provenance_delta = provenance_bonus(
                        record,
                        query,
                        effective_profile,
                        self.scorer.policy_profile(),
                    );
                    let planned = if let Some(scored) = self.scorer.score(record, query) {
                        let mut hit = scored.hit;
                        self.apply_overlay(&mut hit, graph_bonus, provenance_delta);
                        let mut candidate_sources =
                            seed_candidate_sources(&hit, &scored.matched_terms, empty_query);
                        candidate_sources.push(RecallCandidateSource::Graph);
                        if hit.breakdown.episodic > 0.0 {
                            candidate_sources.push(RecallCandidateSource::Episode);
                        }
                        if provenance_delta != 0.0 {
                            candidate_sources.push(RecallCandidateSource::Provenance);
                        }
                        candidate_sources.sort();
                        candidate_sources.dedup();
                        PlannedRecallCandidate {
                            hit,
                            matched_terms: scored.matched_terms,
                            candidate_sources,
                            planner_stage: RecallPlannerStage::GraphExpansion,
                        }
                    } else {
                        self.contextual_candidate(
                            record,
                            query,
                            &query_terms,
                            graph_bonus,
                            provenance_delta,
                        )
                    };

                    if let Some(episode) = &planned.hit.record.episode {
                        next_frontier_episode_ids.insert(episode.episode_id.clone());
                    }
                    next_frontier_ids.insert(planned.hit.record.id.clone());
                    candidates.insert(record.id.clone(), planned);
                    expanded_candidates += 1;
                }

                if next_frontier_ids.is_empty() {
                    break;
                }

                hops_applied = hop;
                frontier_ids = next_frontier_ids;
                frontier_episode_ids = next_frontier_episode_ids;
            }

            graph_expansion_ns = graph_expansion_started.elapsed().as_nanos();
        }

        (
            candidates.into_values().collect(),
            RecallPlannerMetrics {
                candidate_generation_ns,
                graph_expansion_ns,
                total_ns: total_started.elapsed().as_nanos(),
                seeded_candidates,
                expanded_candidates,
                hops_applied,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::{
        ConfiguredRecallScorer, CuratedRecallScorer, ProfileRecallScorer, RecallPlanner,
        RecallScorer, provenance_bonus,
    };
    use crate::config::{
        EmbeddingProviderKind, EngineConfig, RecallPlanningProfile, RecallPolicyProfile,
        RecallScorerKind, RecallScoringProfile,
    };
    use crate::embedding::{EmbeddingVector, SemanticEmbedder};
    use crate::model::{
        EPISODE_SCHEMA_VERSION, EpisodeContext, EpisodeContinuityState, EpisodeSalience,
        LineageLink, LineageRelationKind, MemoryHistoricalState, MemoryQualityState, MemoryRecord,
        MemoryRecordKind, MemoryScope, MemoryTrustLevel,
    };
    use crate::query::{RecallCandidateSource, RecallFilters, RecallPlannerStage, RecallQuery};
    use std::collections::BTreeMap;
    use std::sync::Arc;

    #[derive(Debug)]
    struct FixtureEmbedder;

    impl SemanticEmbedder for FixtureEmbedder {
        fn provider_kind(&self) -> EmbeddingProviderKind {
            EmbeddingProviderKind::Disabled
        }

        fn dimensions(&self) -> usize {
            2
        }

        fn embed(&self, text: &str) -> EmbeddingVector {
            if text.to_ascii_lowercase().contains("storm") {
                EmbeddingVector {
                    values: vec![1.0, 0.0],
                }
            } else {
                EmbeddingVector {
                    values: vec![0.0, 1.0],
                }
            }
        }
    }

    fn scope() -> MemoryScope {
        MemoryScope {
            tenant_id: "default".to_string(),
            namespace: "conversation".to_string(),
            actor_id: "ava".to_string(),
            conversation_id: Some("thread-a".to_string()),
            session_id: Some("session-a".to_string()),
            source: "test".to_string(),
            labels: vec![],
            trust_level: MemoryTrustLevel::Verified,
        }
    }

    fn query() -> RecallQuery {
        RecallQuery {
            scope: scope(),
            query_text: "storm checklist".to_string(),
            max_items: 5,
            token_budget: None,
            filters: RecallFilters::default(),
            include_explanation: false,
        }
    }

    #[test]
    fn configured_scorer_uses_engine_policy_profile() {
        let mut config = EngineConfig::default();
        config.recall_policy_profile = RecallPolicyProfile::Research;

        let scorer = ConfiguredRecallScorer::from_engine_config(&config);
        assert_eq!(scorer.policy_profile(), RecallPolicyProfile::Research);
        assert_eq!(scorer.policy_profile_note(), "policy_profile=research");
    }

    #[test]
    fn support_policy_prefers_current_verified_records() {
        let mut current = record("current", "storm guidance", "storm guidance", 0.5);
        current.scope.trust_level = MemoryTrustLevel::Verified;
        current.quality_state = MemoryQualityState::Verified;
        current.historical_state = MemoryHistoricalState::Current;

        let mut historical = current.clone();
        historical.id = "historical".to_string();
        historical.historical_state = MemoryHistoricalState::Historical;
        historical.quality_state = MemoryQualityState::Archived;

        let query = query();
        let current_bonus = provenance_bonus(
            &current,
            &query,
            RecallPlanningProfile::FastPath,
            RecallPolicyProfile::Support,
        );
        let historical_bonus = provenance_bonus(
            &historical,
            &query,
            RecallPlanningProfile::FastPath,
            RecallPolicyProfile::Support,
        );

        assert!(current_bonus > historical_bonus);
    }

    #[test]
    fn research_policy_is_more_tolerant_of_historical_context() {
        let mut historical = record("historical", "storm history", "storm history", 0.5);
        historical.historical_state = MemoryHistoricalState::Historical;
        historical.quality_state = MemoryQualityState::Archived;

        let query = query();
        let support_bonus = provenance_bonus(
            &historical,
            &query,
            RecallPlanningProfile::FastPath,
            RecallPolicyProfile::Support,
        );
        let research_bonus = provenance_bonus(
            &historical,
            &query,
            RecallPlanningProfile::FastPath,
            RecallPolicyProfile::Research,
        );

        assert!(research_bonus > support_bonus);
    }

    #[test]
    fn support_policy_penalizes_superseded_conflicts_until_history_is_requested() {
        let mut current = record(
            "current",
            "storm rollback is disabled",
            "rollback disabled",
            0.7,
        );
        current.scope.trust_level = MemoryTrustLevel::Verified;
        current.quality_state = MemoryQualityState::Verified;
        current.historical_state = MemoryHistoricalState::Current;

        let mut superseded = current.clone();
        superseded.id = "superseded".to_string();
        superseded.historical_state = MemoryHistoricalState::Superseded;
        superseded.quality_state = MemoryQualityState::Archived;
        superseded.lineage = vec![LineageLink {
            record_id: current.id.clone(),
            relation: LineageRelationKind::ConflictsWith,
            confidence: 0.92,
        }];

        let current_query = query();
        let mut historical_query = query();
        historical_query.filters.historical_mode =
            crate::query::RecallHistoricalMode::HistoricalOnly;

        let current_bonus = provenance_bonus(
            &current,
            &current_query,
            RecallPlanningProfile::FastPath,
            RecallPolicyProfile::Support,
        );
        let superseded_default_bonus = provenance_bonus(
            &superseded,
            &current_query,
            RecallPlanningProfile::FastPath,
            RecallPolicyProfile::Support,
        );
        let superseded_historical_bonus = provenance_bonus(
            &superseded,
            &historical_query,
            RecallPlanningProfile::FastPath,
            RecallPolicyProfile::Support,
        );

        assert!(current_bonus > superseded_default_bonus);
        assert!(superseded_historical_bonus > superseded_default_bonus);
        assert_eq!(
            superseded.lineage[0].relation,
            LineageRelationKind::ConflictsWith
        );
    }

    fn record(id: &str, content: &str, summary: &str, importance_score: f32) -> MemoryRecord {
        MemoryRecord {
            id: id.to_string(),
            scope: scope(),
            kind: MemoryRecordKind::Fact,
            content: content.to_string(),
            summary: Some(summary.to_string()),
            source_id: None,
            metadata: BTreeMap::new(),
            quality_state: MemoryQualityState::Active,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
            expires_at_unix_ms: None,
            importance_score,
            artifact: None,
            episode: None,
            historical_state: MemoryHistoricalState::Current,
            lineage: Vec::new(),
        }
    }

    #[test]
    fn episodic_and_salience_scores_are_reported_when_present() {
        let mut episodic = record(
            "episodic",
            "storm response open loop",
            "storm response",
            0.3,
        );
        episodic.episode = Some(EpisodeContext {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: "storm-episode".to_string(),
            summary: Some("storm response episode".to_string()),
            continuity_state: EpisodeContinuityState::Open,
            actor_ids: vec!["ava".to_string()],
            goal: Some("close the storm remediation checklist".to_string()),
            outcome: None,
            started_at_unix_ms: Some(1),
            ended_at_unix_ms: None,
            last_active_unix_ms: Some(2),
            recurrence_key: None,
            recurrence_interval_ms: None,
            boundary_label: None,
            previous_record_id: Some("storm-previous".to_string()),
            next_record_id: Some("storm-next".to_string()),
            causal_record_ids: vec!["storm-root".to_string()],
            related_record_ids: vec!["storm-related".to_string()],
            linked_artifact_uris: vec![],
            salience: EpisodeSalience {
                reuse_count: 5,
                novelty_score: 0.4,
                goal_relevance: 0.9,
                unresolved_weight: 0.8,
            },
            affective: None,
        });
        let mut query = query();
        query.filters.episode_id = Some("storm-episode".to_string());
        query.filters.unresolved_only = true;

        let scored = ProfileRecallScorer::new(RecallScoringProfile::Balanced)
            .score(&episodic, &query)
            .unwrap();

        assert!(scored.hit.breakdown.episodic > 0.0);
        assert!(scored.hit.breakdown.salience > 0.0);
    }

    #[test]
    fn continuity_intent_queries_trigger_episode_reasoning_and_continuity_planning() {
        let mut episodic = record(
            "episodic",
            "storm response open loop",
            "storm response",
            0.3,
        );
        episodic.episode = Some(EpisodeContext {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: "storm-episode".to_string(),
            summary: Some("storm response episode".to_string()),
            continuity_state: EpisodeContinuityState::Open,
            actor_ids: vec!["ava".to_string()],
            goal: Some("close the storm remediation checklist".to_string()),
            outcome: None,
            started_at_unix_ms: Some(1),
            ended_at_unix_ms: None,
            last_active_unix_ms: Some(2),
            recurrence_key: None,
            recurrence_interval_ms: None,
            boundary_label: None,
            previous_record_id: Some("storm-previous".to_string()),
            next_record_id: Some("storm-next".to_string()),
            causal_record_ids: vec!["storm-root".to_string()],
            related_record_ids: vec!["storm-related".to_string()],
            linked_artifact_uris: vec![],
            salience: EpisodeSalience {
                reuse_count: 5,
                novelty_score: 0.4,
                goal_relevance: 0.9,
                unresolved_weight: 0.8,
            },
            affective: None,
        });

        let planner = RecallPlanner::from_engine_config(&EngineConfig::default());
        for query_text in [
            "what led to this storm state",
            "what changed in the storm episode",
            "what happened next in the storm episode",
        ] {
            let query = RecallQuery {
                query_text: query_text.to_string(),
                ..query()
            };
            let scored = ProfileRecallScorer::new(RecallScoringProfile::Balanced)
                .score(&episodic, &query)
                .unwrap();
            assert_eq!(
                planner.effective_profile(&query),
                RecallPlanningProfile::ContinuityAware
            );
            assert!(scored.hit.breakdown.episodic > 0.0);
        }

        let mut unresolved_query = query();
        unresolved_query.filters.unresolved_only = true;
        let unresolved_scored = ProfileRecallScorer::new(RecallScoringProfile::Balanced)
            .score(&episodic, &unresolved_query)
            .unwrap();
        assert_eq!(
            planner.effective_profile(&unresolved_query),
            RecallPlanningProfile::ContinuityAware
        );
        assert!(unresolved_scored.hit.breakdown.episodic > 0.0);
    }

    #[test]
    fn profile_scorer_changes_total_weighting() {
        let lexical = record(
            "lexical",
            "storm mitigation storm checklist",
            "storm checklist",
            0.1,
        );
        let importance = record("importance", "storm memo", "storm memo", 0.95);
        let query = query();

        let lexical_first = ProfileRecallScorer::new(RecallScoringProfile::LexicalFirst);
        let importance_first = ProfileRecallScorer::new(RecallScoringProfile::ImportanceFirst);

        let lexical_first_lexical = lexical_first.score(&lexical, &query).unwrap();
        let lexical_first_importance = lexical_first.score(&importance, &query).unwrap();
        assert!(
            lexical_first_lexical.hit.breakdown.total
                > lexical_first_importance.hit.breakdown.total
        );

        let importance_first_lexical = importance_first.score(&lexical, &query).unwrap();
        let importance_first_importance = importance_first.score(&importance, &query).unwrap();
        assert!(
            importance_first_importance.hit.breakdown.total
                > importance_first_lexical.hit.breakdown.total
        );
    }

    #[test]
    fn curated_scorer_rewards_verified_high_trust_records() {
        let lexical = record(
            "lexical",
            "storm mitigation storm checklist",
            "storm checklist",
            0.1,
        );
        let mut curated = record("curated", "storm memo", "storm memo", 0.4);
        curated.scope.trust_level = crate::model::MemoryTrustLevel::Pinned;
        curated.quality_state = crate::model::MemoryQualityState::Verified;
        let query = query();

        let scorer = CuratedRecallScorer::new(RecallScoringProfile::ImportanceFirst);
        let lexical_score = scorer.score(&lexical, &query).unwrap();
        let curated_score = scorer.score(&curated, &query).unwrap();
        assert!(curated_score.hit.breakdown.total > lexical_score.hit.breakdown.total);
    }

    #[test]
    fn configured_scorer_uses_engine_config_kind() {
        let mut config = EngineConfig::default();
        config.recall_scorer_kind = RecallScorerKind::Curated;
        config.recall_scoring_profile = RecallScoringProfile::ImportanceFirst;
        let scorer = ConfiguredRecallScorer::from_engine_config(&config);
        assert_eq!(scorer.scorer_kind(), RecallScorerKind::Curated);
        assert_eq!(
            scorer.scoring_profile(),
            RecallScoringProfile::ImportanceFirst
        );
    }

    #[test]
    fn deterministic_embedder_populates_semantic_score() {
        let mut config = EngineConfig::default();
        config.embedding_provider_kind = EmbeddingProviderKind::DeterministicLocal;
        config.embedding_dimensions = 64;
        let scorer = ConfiguredRecallScorer::from_engine_config(&config);
        let scored = scorer
            .score(
                &record(
                    "semantic",
                    "storm checklist remediation notes",
                    "storm remediation",
                    0.4,
                ),
                &query(),
            )
            .unwrap();
        assert!(scored.hit.breakdown.semantic > 0.0);
        assert_eq!(
            scorer.embedding_note(),
            Some("embedding_provider=deterministic_local".to_string())
        );
    }

    #[test]
    fn planner_can_use_shared_embedder_and_report_provider_note() {
        let planner = RecallPlanner::with_shared_embedder(
            RecallPlanningProfile::FastPath,
            0,
            RecallScorerKind::Profile,
            RecallScoringProfile::Balanced,
            RecallPolicyProfile::General,
            Arc::new(FixtureEmbedder),
            "embedding_provider=fixture_custom",
        );

        let planned = planner.plan(
            &[record(
                "semantic",
                "storm checklist remediation notes",
                "storm remediation",
                0.4,
            )],
            &query(),
        );

        assert_eq!(
            planner.scorer().embedding_note(),
            Some("embedding_provider=fixture_custom".to_string())
        );
        assert_eq!(planned.len(), 1);
        assert!(planned[0].hit.breakdown.semantic > 0.0);
    }

    #[test]
    fn continuity_planner_expands_graph_neighbors() {
        let mut config = EngineConfig::default();
        config.recall_planning_profile = RecallPlanningProfile::ContinuityAware;
        let planner = RecallPlanner::from_engine_config(&config);

        let mut seed = record(
            "seed",
            "storm checklist remediation",
            "storm checklist",
            0.5,
        );
        seed.episode = Some(EpisodeContext {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: "storm-episode".to_string(),
            summary: Some("storm remediation thread".to_string()),
            continuity_state: EpisodeContinuityState::Open,
            actor_ids: vec!["ava".to_string()],
            goal: Some("close follow-up actions".to_string()),
            outcome: None,
            started_at_unix_ms: Some(1),
            ended_at_unix_ms: None,
            last_active_unix_ms: Some(2),
            recurrence_key: None,
            recurrence_interval_ms: None,
            boundary_label: None,
            previous_record_id: None,
            next_record_id: Some("neighbor".to_string()),
            causal_record_ids: vec![],
            related_record_ids: vec!["neighbor".to_string()],
            linked_artifact_uris: vec![],
            salience: EpisodeSalience::default(),
            affective: None,
        });

        let mut neighbor = record("neighbor", "follow-up note", "follow-up note", 0.2);
        neighbor.episode = Some(EpisodeContext {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: "storm-episode".to_string(),
            summary: Some("storm remediation thread".to_string()),
            continuity_state: EpisodeContinuityState::Open,
            actor_ids: vec!["ava".to_string()],
            goal: Some("close follow-up actions".to_string()),
            outcome: None,
            started_at_unix_ms: Some(2),
            ended_at_unix_ms: None,
            last_active_unix_ms: Some(3),
            recurrence_key: None,
            recurrence_interval_ms: None,
            boundary_label: None,
            previous_record_id: Some("seed".to_string()),
            next_record_id: None,
            causal_record_ids: vec!["seed".to_string()],
            related_record_ids: vec![],
            linked_artifact_uris: vec![],
            salience: EpisodeSalience::default(),
            affective: None,
        });

        let planned = planner.plan(&[seed, neighbor], &query());
        assert_eq!(planned.len(), 2);
        let expanded = planned
            .iter()
            .find(|candidate| candidate.hit.record.id == "neighbor")
            .unwrap();
        assert_eq!(expanded.planner_stage, RecallPlannerStage::GraphExpansion);
        assert!(
            expanded
                .candidate_sources
                .contains(&RecallCandidateSource::Graph)
        );
        assert!(expanded.hit.breakdown.graph > 0.0);
    }

    #[test]
    fn fast_path_planner_skips_graph_expansion() {
        let planner = RecallPlanner::from_engine_config(&EngineConfig::default());

        let mut seed = record(
            "seed",
            "storm checklist remediation",
            "storm checklist",
            0.5,
        );
        seed.episode = Some(EpisodeContext {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: "storm-episode".to_string(),
            summary: Some("storm remediation thread".to_string()),
            continuity_state: EpisodeContinuityState::Open,
            actor_ids: vec!["ava".to_string()],
            goal: Some("close follow-up actions".to_string()),
            outcome: None,
            started_at_unix_ms: Some(1),
            ended_at_unix_ms: None,
            last_active_unix_ms: Some(2),
            recurrence_key: None,
            recurrence_interval_ms: None,
            boundary_label: None,
            previous_record_id: None,
            next_record_id: Some("neighbor".to_string()),
            causal_record_ids: vec![],
            related_record_ids: vec!["neighbor".to_string()],
            linked_artifact_uris: vec![],
            salience: EpisodeSalience::default(),
            affective: None,
        });

        let mut neighbor = record("neighbor", "follow-up note", "follow-up note", 0.2);
        neighbor.episode = Some(EpisodeContext {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: "storm-episode".to_string(),
            summary: Some("storm remediation thread".to_string()),
            continuity_state: EpisodeContinuityState::Open,
            actor_ids: vec!["ava".to_string()],
            goal: Some("close follow-up actions".to_string()),
            outcome: None,
            started_at_unix_ms: Some(2),
            ended_at_unix_ms: None,
            last_active_unix_ms: Some(3),
            recurrence_key: None,
            recurrence_interval_ms: None,
            boundary_label: None,
            previous_record_id: Some("seed".to_string()),
            next_record_id: None,
            causal_record_ids: vec!["seed".to_string()],
            related_record_ids: vec![],
            linked_artifact_uris: vec![],
            salience: EpisodeSalience::default(),
            affective: None,
        });

        let planned = planner.plan(&[seed, neighbor], &query());
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].hit.record.id, "seed");
    }

    #[test]
    fn continuity_planner_respects_graph_hop_limit() {
        let mut config = EngineConfig::default();
        config.recall_planning_profile = RecallPlanningProfile::ContinuityAware;
        config.graph_expansion_max_hops = 1;
        let one_hop_planner = RecallPlanner::from_engine_config(&config);

        config.graph_expansion_max_hops = 2;
        let two_hop_planner = RecallPlanner::from_engine_config(&config);

        let mut seed = record(
            "seed",
            "storm checklist remediation",
            "storm checklist",
            0.5,
        );
        seed.episode = Some(EpisodeContext {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: "storm-episode".to_string(),
            summary: Some("storm remediation thread".to_string()),
            continuity_state: EpisodeContinuityState::Open,
            actor_ids: vec!["ava".to_string()],
            goal: Some("close follow-up actions".to_string()),
            outcome: None,
            started_at_unix_ms: Some(1),
            ended_at_unix_ms: None,
            last_active_unix_ms: Some(2),
            recurrence_key: None,
            recurrence_interval_ms: None,
            boundary_label: None,
            previous_record_id: None,
            next_record_id: Some("middle".to_string()),
            causal_record_ids: vec![],
            related_record_ids: vec![],
            linked_artifact_uris: vec![],
            salience: EpisodeSalience::default(),
            affective: None,
        });

        let mut middle = record("middle", "follow-up note", "follow-up note", 0.2);
        middle.episode = Some(EpisodeContext {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: "storm-episode".to_string(),
            summary: Some("storm remediation thread".to_string()),
            continuity_state: EpisodeContinuityState::Open,
            actor_ids: vec!["ava".to_string()],
            goal: Some("close follow-up actions".to_string()),
            outcome: None,
            started_at_unix_ms: Some(2),
            ended_at_unix_ms: None,
            last_active_unix_ms: Some(3),
            recurrence_key: None,
            recurrence_interval_ms: None,
            boundary_label: None,
            previous_record_id: Some("seed".to_string()),
            next_record_id: Some("far".to_string()),
            causal_record_ids: vec!["seed".to_string()],
            related_record_ids: vec![],
            linked_artifact_uris: vec![],
            salience: EpisodeSalience::default(),
            affective: None,
        });

        let mut far = record("far", "final action", "final action", 0.1);
        far.episode = Some(EpisodeContext {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: "storm-secondary-episode".to_string(),
            summary: Some("storm secondary thread".to_string()),
            continuity_state: EpisodeContinuityState::Open,
            actor_ids: vec!["ava".to_string()],
            goal: Some("close follow-up actions".to_string()),
            outcome: None,
            started_at_unix_ms: Some(3),
            ended_at_unix_ms: None,
            last_active_unix_ms: Some(4),
            recurrence_key: None,
            recurrence_interval_ms: None,
            boundary_label: None,
            previous_record_id: Some("middle".to_string()),
            next_record_id: None,
            causal_record_ids: vec!["middle".to_string()],
            related_record_ids: vec![],
            linked_artifact_uris: vec![],
            salience: EpisodeSalience::default(),
            affective: None,
        });

        let one_hop = one_hop_planner.plan(&[seed.clone(), middle.clone(), far.clone()], &query());
        assert!(
            one_hop
                .iter()
                .any(|candidate| candidate.hit.record.id == "middle")
        );
        assert!(
            !one_hop
                .iter()
                .any(|candidate| candidate.hit.record.id == "far")
        );

        let (two_hop, metrics) = two_hop_planner.plan_with_metrics(&[seed, middle, far], &query());
        assert!(
            two_hop
                .iter()
                .any(|candidate| candidate.hit.record.id == "far")
        );
        assert_eq!(metrics.hops_applied, 2);
        assert!(metrics.graph_expansion_ns > 0);
    }

    #[test]
    fn continuity_planner_stays_within_query_scope_boundaries() {
        let mut config = EngineConfig::default();
        config.recall_planning_profile = RecallPlanningProfile::ContinuityAware;
        config.graph_expansion_max_hops = 2;
        let planner = RecallPlanner::from_engine_config(&config);

        let mut seed = record(
            "seed",
            "storm checklist remediation",
            "storm checklist",
            0.5,
        );
        seed.episode = Some(EpisodeContext {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: "storm-episode".to_string(),
            summary: Some("storm remediation thread".to_string()),
            continuity_state: EpisodeContinuityState::Open,
            actor_ids: vec!["ava".to_string()],
            goal: Some("close follow-up actions".to_string()),
            outcome: None,
            started_at_unix_ms: Some(1),
            ended_at_unix_ms: None,
            last_active_unix_ms: Some(2),
            recurrence_key: None,
            recurrence_interval_ms: None,
            boundary_label: None,
            previous_record_id: None,
            next_record_id: Some("cross-scope".to_string()),
            causal_record_ids: vec![],
            related_record_ids: vec!["cross-scope".to_string()],
            linked_artifact_uris: vec![],
            salience: EpisodeSalience::default(),
            affective: None,
        });

        let mut cross_scope = record("cross-scope", "storm follow-up note", "follow-up", 0.2);
        cross_scope.scope.namespace = "other".to_string();
        cross_scope.episode = Some(EpisodeContext {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: "storm-episode".to_string(),
            summary: Some("storm remediation thread".to_string()),
            continuity_state: EpisodeContinuityState::Open,
            actor_ids: vec!["ava".to_string()],
            goal: Some("close follow-up actions".to_string()),
            outcome: None,
            started_at_unix_ms: Some(2),
            ended_at_unix_ms: None,
            last_active_unix_ms: Some(3),
            recurrence_key: None,
            recurrence_interval_ms: None,
            boundary_label: None,
            previous_record_id: Some("seed".to_string()),
            next_record_id: None,
            causal_record_ids: vec!["seed".to_string()],
            related_record_ids: vec![],
            linked_artifact_uris: vec![],
            salience: EpisodeSalience::default(),
            affective: None,
        });

        let planned = planner.plan(&[seed, cross_scope], &query());
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].hit.record.id, "seed");
    }

    #[test]
    fn temporal_score_rewards_recurrence_duration_and_boundary_cues() {
        let mut recurring = record(
            "recurring",
            "weekly release retrospective handoff",
            "weekly release retrospective",
            0.4,
        );
        recurring.episode = Some(EpisodeContext {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: "release-retro".to_string(),
            summary: Some("Recurring release retrospective".to_string()),
            continuity_state: EpisodeContinuityState::Open,
            actor_ids: vec!["ava".to_string()],
            goal: Some("review each release boundary".to_string()),
            outcome: None,
            started_at_unix_ms: Some(1),
            ended_at_unix_ms: Some(1 + (4 * 60 * 60 * 1000)),
            last_active_unix_ms: Some(1 + (4 * 60 * 60 * 1000)),
            recurrence_key: Some("weekly-release-retro".to_string()),
            recurrence_interval_ms: Some(7 * 24 * 60 * 60 * 1000),
            boundary_label: Some("weekly-release-boundary".to_string()),
            previous_record_id: Some("retro-previous".to_string()),
            next_record_id: Some("retro-next".to_string()),
            causal_record_ids: vec!["retro-root".to_string()],
            related_record_ids: vec![],
            linked_artifact_uris: vec![],
            salience: EpisodeSalience::default(),
            affective: None,
        });

        let plain = record(
            "plain",
            "weekly release retrospective handoff",
            "weekly release retrospective",
            0.4,
        );

        let query = RecallQuery {
            query_text: "how long does the recurring weekly release handoff boundary last"
                .to_string(),
            ..query()
        };

        let scorer = ProfileRecallScorer::new(RecallScoringProfile::Balanced);
        let recurring_scored = scorer.score(&recurring, &query).unwrap();
        let plain_scored = scorer.score(&plain, &query).unwrap();
        assert!(recurring_scored.hit.breakdown.temporal > plain_scored.hit.breakdown.temporal);
    }
}
