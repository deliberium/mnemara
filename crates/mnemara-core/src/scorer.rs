use crate::config::{EmbeddingProviderKind, EngineConfig, RecallScorerKind, RecallScoringProfile};
use crate::embedding::{ConfiguredSemanticEmbedder, SemanticEmbedder};
use crate::model::{MemoryQualityState, MemoryRecord, MemoryTrustLevel};
use crate::query::{RecallHit, RecallQuery, RecallScoreBreakdown};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq)]
pub struct ScoredRecallCandidate {
    pub hit: RecallHit,
    pub matched_terms: Vec<String>,
}

pub trait RecallScorer: Send + Sync {
    fn score(&self, record: &MemoryRecord, query: &RecallQuery) -> Option<ScoredRecallCandidate>;
    fn scorer_kind(&self) -> RecallScorerKind;
    fn scoring_profile(&self) -> RecallScoringProfile;
    fn profile_note(&self) -> &'static str;
    fn embedding_note(&self) -> Option<&'static str>;
}

fn semantic_similarity(
    embedder: &ConfiguredSemanticEmbedder,
    query_text: &str,
    haystack: &str,
) -> f32 {
    if embedder.provider_kind() == EmbeddingProviderKind::Disabled {
        return 0.0;
    }

    embedder
        .embed(query_text)
        .cosine_similarity(&embedder.embed(haystack))
        .max(0.0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileRecallScorer {
    profile: RecallScoringProfile,
    embedder: ConfiguredSemanticEmbedder,
}

impl ProfileRecallScorer {
    pub fn new(profile: RecallScoringProfile) -> Self {
        Self::with_embedder(
            profile,
            ConfiguredSemanticEmbedder::Disabled(Default::default()),
        )
    }

    pub fn with_embedder(
        profile: RecallScoringProfile,
        embedder: ConfiguredSemanticEmbedder,
    ) -> Self {
        Self { profile, embedder }
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

    fn temporal_score(&self, record: &MemoryRecord) -> f32 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(record.updated_at_unix_ms);
        let freshness_window_ms = 7 * 24 * 60 * 60 * 1000u64;
        let age_ms = now
            .saturating_sub(record.updated_at_unix_ms)
            .min(freshness_window_ms) as f32;
        1.0 - (age_ms / freshness_window_ms as f32)
    }
}

impl RecallScorer for ProfileRecallScorer {
    fn score(&self, record: &MemoryRecord, query: &RecallQuery) -> Option<ScoredRecallCandidate> {
        let (lexical_weight, semantic_weight, policy_weight) = self.weights();

        if query.query_text.trim().is_empty() {
            return Some(ScoredRecallCandidate {
                hit: RecallHit {
                    record: record.clone(),
                    breakdown: RecallScoreBreakdown {
                        lexical: 0.0,
                        semantic: 0.0,
                        graph: 0.0,
                        temporal: 1.0,
                        metadata: 0.0,
                        curation: 0.0,
                        policy: record.importance_score,
                        total: 1.0 + (record.importance_score * policy_weight),
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
        let temporal = self.temporal_score(record);
        let policy = record.importance_score;
        let total = (lexical * lexical_weight)
            + (semantic * semantic_weight)
            + (metadata * 0.4)
            + (temporal * 0.2)
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

    fn profile_note(&self) -> &'static str {
        match self.profile {
            RecallScoringProfile::Balanced => "scoring_profile=balanced",
            RecallScoringProfile::LexicalFirst => "scoring_profile=lexical_first",
            RecallScoringProfile::ImportanceFirst => "scoring_profile=importance_first",
        }
    }

    fn embedding_note(&self) -> Option<&'static str> {
        match self.embedder.provider_kind() {
            EmbeddingProviderKind::Disabled => None,
            EmbeddingProviderKind::DeterministicLocal => {
                Some("embedding_provider=deterministic_local")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CuratedRecallScorer {
    profile: RecallScoringProfile,
    embedder: ConfiguredSemanticEmbedder,
}

impl CuratedRecallScorer {
    pub fn new(profile: RecallScoringProfile) -> Self {
        Self::with_embedder(
            profile,
            ConfiguredSemanticEmbedder::Disabled(Default::default()),
        )
    }

    pub fn with_embedder(
        profile: RecallScoringProfile,
        embedder: ConfiguredSemanticEmbedder,
    ) -> Self {
        Self { profile, embedder }
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

    fn temporal_score(&self, record: &MemoryRecord) -> f32 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(record.updated_at_unix_ms);
        let freshness_window_ms = 30 * 24 * 60 * 60 * 1000u64;
        let age_ms = now
            .saturating_sub(record.updated_at_unix_ms)
            .min(freshness_window_ms) as f32;
        1.0 - (age_ms / freshness_window_ms as f32)
    }
}

impl RecallScorer for CuratedRecallScorer {
    fn score(&self, record: &MemoryRecord, query: &RecallQuery) -> Option<ScoredRecallCandidate> {
        let (lexical_weight, semantic_weight, policy_weight) = self.weights();
        let curated_policy = (record.importance_score + self.curation_bonus(record)).max(0.0);

        if query.query_text.trim().is_empty() {
            return Some(ScoredRecallCandidate {
                hit: RecallHit {
                    record: record.clone(),
                    breakdown: RecallScoreBreakdown {
                        lexical: 0.0,
                        semantic: 0.0,
                        graph: 0.0,
                        temporal: 1.0,
                        metadata: 0.0,
                        curation: self.curation_bonus(record),
                        policy: curated_policy,
                        total: 1.0 + (curated_policy * policy_weight),
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
        let temporal = self.temporal_score(record);
        let curation = self.curation_bonus(record);
        let total = (lexical * lexical_weight)
            + (semantic * semantic_weight)
            + (metadata * 0.45)
            + (temporal * 0.2)
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

    fn profile_note(&self) -> &'static str {
        match self.profile {
            RecallScoringProfile::Balanced => "scoring_profile=balanced",
            RecallScoringProfile::LexicalFirst => "scoring_profile=lexical_first",
            RecallScoringProfile::ImportanceFirst => "scoring_profile=importance_first",
        }
    }

    fn embedding_note(&self) -> Option<&'static str> {
        match self.embedder.provider_kind() {
            EmbeddingProviderKind::Disabled => None,
            EmbeddingProviderKind::DeterministicLocal => {
                Some("embedding_provider=deterministic_local")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
                embedder,
            )),
            RecallScorerKind::Curated => Self::Curated(CuratedRecallScorer::with_embedder(
                config.recall_scoring_profile,
                embedder,
            )),
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

    fn profile_note(&self) -> &'static str {
        match self {
            Self::Profile(scorer) => scorer.profile_note(),
            Self::Curated(scorer) => scorer.profile_note(),
        }
    }

    fn embedding_note(&self) -> Option<&'static str> {
        match self {
            Self::Profile(scorer) => scorer.embedding_note(),
            Self::Curated(scorer) => scorer.embedding_note(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::{ConfiguredRecallScorer, CuratedRecallScorer, ProfileRecallScorer, RecallScorer};
    use crate::config::{
        EmbeddingProviderKind, EngineConfig, RecallScorerKind, RecallScoringProfile,
    };
    use crate::model::{
        MemoryQualityState, MemoryRecord, MemoryRecordKind, MemoryScope, MemoryTrustLevel,
    };
    use crate::query::{RecallFilters, RecallQuery};
    use std::collections::BTreeMap;

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
        }
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
            Some("embedding_provider=deterministic_local")
        );
    }
}
