use crate::config::{EmbeddingProviderKind, EngineConfig};
use std::fmt;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingVector {
    pub values: Vec<f32>,
}

impl EmbeddingVector {
    pub fn cosine_similarity(&self, other: &Self) -> f32 {
        if self.values.is_empty() || self.values.len() != other.values.len() {
            return 0.0;
        }

        let mut dot = 0.0;
        let mut left_norm = 0.0;
        let mut right_norm = 0.0;
        for (left, right) in self.values.iter().zip(&other.values) {
            dot += left * right;
            left_norm += left * left;
            right_norm += right * right;
        }

        if left_norm == 0.0 || right_norm == 0.0 {
            return 0.0;
        }

        dot / (left_norm.sqrt() * right_norm.sqrt())
    }
}

pub trait SemanticEmbedder: Send + Sync {
    fn provider_kind(&self) -> EmbeddingProviderKind;
    fn dimensions(&self) -> usize;
    fn embed(&self, text: &str) -> EmbeddingVector;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DisabledEmbedder;

impl SemanticEmbedder for DisabledEmbedder {
    fn provider_kind(&self) -> EmbeddingProviderKind {
        EmbeddingProviderKind::Disabled
    }

    fn dimensions(&self) -> usize {
        0
    }

    fn embed(&self, _text: &str) -> EmbeddingVector {
        EmbeddingVector { values: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeterministicLocalEmbedder {
    dimensions: usize,
}

impl DeterministicLocalEmbedder {
    pub fn new(dimensions: usize) -> Self {
        Self {
            dimensions: dimensions.max(1),
        }
    }

    fn hash_with_seed(term: &str, seed: u64) -> u64 {
        let mut hash = seed;
        for byte in term.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(1099511628211);
        }
        hash
    }

    fn bucket_for(term: &str, dimensions: usize, seed: u64) -> usize {
        (Self::hash_with_seed(term, seed) as usize) % dimensions
    }

    fn signed_weight(term: &str) -> f32 {
        if Self::hash_with_seed(term, 7809847782465536322u64) & 1 == 0 {
            1.0
        } else {
            -1.0
        }
    }
}

impl SemanticEmbedder for DeterministicLocalEmbedder {
    fn provider_kind(&self) -> EmbeddingProviderKind {
        EmbeddingProviderKind::DeterministicLocal
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn embed(&self, text: &str) -> EmbeddingVector {
        let mut values = vec![0.0; self.dimensions];
        for term in text
            .split_whitespace()
            .map(|term| term.trim_matches(|ch: char| !ch.is_alphanumeric()))
            .filter(|term| !term.is_empty())
            .map(|term| term.to_ascii_lowercase())
        {
            let primary_bucket = Self::bucket_for(&term, self.dimensions, 1469598103934665603u64);
            let secondary_bucket = Self::bucket_for(&term, self.dimensions, 1099511628211u64);
            let sign = Self::signed_weight(&term);
            values[primary_bucket] += sign;
            if self.dimensions > 1 {
                values[secondary_bucket] += sign * 0.5;
            }
        }

        let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
        if norm > 0.0 {
            for value in &mut values {
                *value /= norm;
            }
        }

        EmbeddingVector { values }
    }
}

#[derive(Clone)]
pub struct SharedSemanticEmbedder {
    provider_note: String,
    embedder: Arc<dyn SemanticEmbedder>,
}

impl SharedSemanticEmbedder {
    pub fn new(embedder: Arc<dyn SemanticEmbedder>, provider_note: impl Into<String>) -> Self {
        Self {
            provider_note: provider_note.into(),
            embedder,
        }
    }

    pub fn provider_note(&self) -> &str {
        &self.provider_note
    }
}

impl fmt::Debug for SharedSemanticEmbedder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SharedSemanticEmbedder")
            .field("provider_note", &self.provider_note)
            .field("provider_kind", &self.embedder.provider_kind())
            .field("dimensions", &self.embedder.dimensions())
            .finish()
    }
}

impl SemanticEmbedder for SharedSemanticEmbedder {
    fn provider_kind(&self) -> EmbeddingProviderKind {
        self.embedder.provider_kind()
    }

    fn dimensions(&self) -> usize {
        self.embedder.dimensions()
    }

    fn embed(&self, text: &str) -> EmbeddingVector {
        self.embedder.embed(text)
    }
}

#[derive(Debug, Clone)]
pub enum ConfiguredSemanticEmbedder {
    Disabled(DisabledEmbedder),
    DeterministicLocal(DeterministicLocalEmbedder),
    Shared(SharedSemanticEmbedder),
}

impl ConfiguredSemanticEmbedder {
    pub fn from_engine_config(config: &EngineConfig) -> Self {
        match config.embedding_provider_kind {
            EmbeddingProviderKind::Disabled => Self::Disabled(DisabledEmbedder),
            EmbeddingProviderKind::DeterministicLocal => Self::DeterministicLocal(
                DeterministicLocalEmbedder::new(config.embedding_dimensions),
            ),
        }
    }

    pub fn shared(embedder: Arc<dyn SemanticEmbedder>, provider_note: impl Into<String>) -> Self {
        Self::Shared(SharedSemanticEmbedder::new(embedder, provider_note))
    }

    pub fn provider_note(&self) -> Option<String> {
        match self {
            Self::Disabled(_) => None,
            Self::DeterministicLocal(_) => {
                Some("embedding_provider=deterministic_local".to_string())
            }
            Self::Shared(embedder) => Some(embedder.provider_note().to_string()),
        }
    }
}

impl SemanticEmbedder for ConfiguredSemanticEmbedder {
    fn provider_kind(&self) -> EmbeddingProviderKind {
        match self {
            Self::Disabled(embedder) => embedder.provider_kind(),
            Self::DeterministicLocal(embedder) => embedder.provider_kind(),
            Self::Shared(embedder) => embedder.provider_kind(),
        }
    }

    fn dimensions(&self) -> usize {
        match self {
            Self::Disabled(embedder) => embedder.dimensions(),
            Self::DeterministicLocal(embedder) => embedder.dimensions(),
            Self::Shared(embedder) => embedder.dimensions(),
        }
    }

    fn embed(&self, text: &str) -> EmbeddingVector {
        match self {
            Self::Disabled(embedder) => embedder.embed(text),
            Self::DeterministicLocal(embedder) => embedder.embed(text),
            Self::Shared(embedder) => embedder.embed(text),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::{
        ConfiguredSemanticEmbedder, DeterministicLocalEmbedder, EmbeddingVector, SemanticEmbedder,
    };
    use crate::config::{EmbeddingProviderKind, EngineConfig};
    use std::sync::Arc;

    #[derive(Debug)]
    struct FixedEmbedder;

    impl SemanticEmbedder for FixedEmbedder {
        fn provider_kind(&self) -> EmbeddingProviderKind {
            EmbeddingProviderKind::Disabled
        }

        fn dimensions(&self) -> usize {
            2
        }

        fn embed(&self, text: &str) -> EmbeddingVector {
            if text.contains("storm") {
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

    #[test]
    fn deterministic_embedder_returns_stable_dimensions() {
        let embedder = DeterministicLocalEmbedder::new(8);
        let vector = embedder.embed("storm checklist storm");
        assert_eq!(vector.values.len(), 8);
        assert!(vector.values.iter().any(|value| *value > 0.0));
    }

    #[test]
    fn deterministic_embedder_scores_related_texts_higher() {
        let embedder = DeterministicLocalEmbedder::new(64);
        let related = embedder
            .embed("verified storm checklist")
            .cosine_similarity(&embedder.embed("storm checklist for verified runbook"));
        let unrelated = embedder
            .embed("verified storm checklist")
            .cosine_similarity(&embedder.embed("audio waveform synthesis"));
        assert!(related > unrelated);
    }

    #[test]
    fn configured_embedder_uses_engine_config_provider() {
        let mut config = EngineConfig::default();
        config.embedding_provider_kind = EmbeddingProviderKind::DeterministicLocal;
        config.embedding_dimensions = 12;

        let embedder = ConfiguredSemanticEmbedder::from_engine_config(&config);
        assert_eq!(
            embedder.provider_kind(),
            EmbeddingProviderKind::DeterministicLocal
        );
        assert_eq!(embedder.dimensions(), 12);
    }

    #[test]
    fn configured_embedder_disabled_is_safe_fallback() {
        let config = EngineConfig::default();

        let embedder = ConfiguredSemanticEmbedder::from_engine_config(&config);
        let vector = embedder.embed("storm checklist remediation");

        assert_eq!(embedder.provider_kind(), EmbeddingProviderKind::Disabled);
        assert_eq!(embedder.dimensions(), 0);
        assert!(vector.values.is_empty());
    }

    #[test]
    fn cosine_similarity_returns_zero_for_mismatched_vectors() {
        let left = DeterministicLocalEmbedder::new(8).embed("storm checklist");
        let right = DeterministicLocalEmbedder::new(16).embed("storm checklist");

        assert_eq!(left.cosine_similarity(&right), 0.0);
    }

    #[test]
    fn shared_embedder_keeps_custom_provider_note() {
        let embedder = ConfiguredSemanticEmbedder::shared(
            Arc::new(FixedEmbedder),
            "embedding_provider=fixture_custom",
        );

        assert_eq!(embedder.dimensions(), 2);
        assert_eq!(
            embedder.provider_note().as_deref(),
            Some("embedding_provider=fixture_custom")
        );
        assert!(
            embedder
                .embed("storm checklist")
                .cosine_similarity(&embedder.embed("storm runbook"))
                > 0.0
        );
    }
}
