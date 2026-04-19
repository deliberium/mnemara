use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const EPISODE_SCHEMA_VERSION: u32 = 1;

fn default_episode_schema_version() -> u32 {
    EPISODE_SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MemoryScope {
    pub tenant_id: String,
    pub namespace: String,
    pub actor_id: String,
    pub conversation_id: Option<String>,
    pub session_id: Option<String>,
    pub source: String,
    pub labels: Vec<String>,
    pub trust_level: MemoryTrustLevel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MemoryTrustLevel {
    Untrusted,
    Observed,
    #[default]
    Derived,
    Verified,
    Pinned,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryRecordKind {
    Episodic,
    Summary,
    Fact,
    Preference,
    Task,
    Artifact,
    Hypothesis,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryQualityState {
    Draft,
    Active,
    Verified,
    Archived,
    Suppressed,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactPointer {
    pub uri: String,
    pub media_type: Option<String>,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum EpisodeContinuityState {
    #[default]
    Open,
    Resolved,
    Superseded,
    Abandoned,
}

impl EpisodeContinuityState {
    pub fn is_unresolved(self) -> bool {
        matches!(self, Self::Open | Self::Abandoned)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MemoryHistoricalState {
    #[default]
    Current,
    Historical,
    Superseded,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum LineageRelationKind {
    #[default]
    DerivedFrom,
    ConsolidatedFrom,
    Supersedes,
    SupersededBy,
    ConflictsWith,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineageLink {
    pub record_id: String,
    pub relation: LineageRelationKind,
    pub confidence: f32,
}

impl Default for LineageLink {
    fn default() -> Self {
        Self {
            record_id: String::new(),
            relation: LineageRelationKind::default(),
            confidence: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum AffectiveAnnotationProvenance {
    #[default]
    Authored,
    Imported,
    Derived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AffectiveAnnotation {
    pub tone: Option<String>,
    pub sentiment: Option<String>,
    pub urgency: f32,
    pub confidence: f32,
    pub tension: f32,
    pub provenance: AffectiveAnnotationProvenance,
}

impl Default for AffectiveAnnotation {
    fn default() -> Self {
        Self {
            tone: None,
            sentiment: None,
            urgency: 0.0,
            confidence: 1.0,
            tension: 0.0,
            provenance: AffectiveAnnotationProvenance::default(),
        }
    }
}

impl AffectiveAnnotation {
    pub fn validate(&self) -> Result<()> {
        if self
            .tone
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(Error::InvalidRequest(
                "affective tone cannot be empty when provided".to_string(),
            ));
        }
        if self
            .sentiment
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(Error::InvalidRequest(
                "affective sentiment cannot be empty when provided".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&self.urgency) {
            return Err(Error::InvalidRequest(
                "affective urgency must be within 0.0..=1.0".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(Error::InvalidRequest(
                "affective confidence must be within 0.0..=1.0".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&self.tension) {
            return Err(Error::InvalidRequest(
                "affective tension must be within 0.0..=1.0".to_string(),
            ));
        }
        if matches!(self.provenance, AffectiveAnnotationProvenance::Derived)
            && self.confidence >= 1.0
        {
            return Err(Error::InvalidRequest(
                "derived affective confidence must remain below certainty".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpisodeSalience {
    pub reuse_count: u32,
    pub novelty_score: f32,
    pub goal_relevance: f32,
    pub unresolved_weight: f32,
}

impl Default for EpisodeSalience {
    fn default() -> Self {
        Self {
            reuse_count: 0,
            novelty_score: 0.0,
            goal_relevance: 0.0,
            unresolved_weight: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpisodeContext {
    #[serde(default = "default_episode_schema_version")]
    pub schema_version: u32,
    pub episode_id: String,
    pub summary: Option<String>,
    pub continuity_state: EpisodeContinuityState,
    pub actor_ids: Vec<String>,
    pub goal: Option<String>,
    pub outcome: Option<String>,
    pub started_at_unix_ms: Option<u64>,
    pub ended_at_unix_ms: Option<u64>,
    pub last_active_unix_ms: Option<u64>,
    #[serde(default)]
    pub recurrence_key: Option<String>,
    #[serde(default)]
    pub recurrence_interval_ms: Option<u64>,
    #[serde(default)]
    pub boundary_label: Option<String>,
    pub previous_record_id: Option<String>,
    pub next_record_id: Option<String>,
    pub causal_record_ids: Vec<String>,
    pub related_record_ids: Vec<String>,
    pub linked_artifact_uris: Vec<String>,
    pub salience: EpisodeSalience,
    pub affective: Option<AffectiveAnnotation>,
}

impl Default for EpisodeContext {
    fn default() -> Self {
        Self {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: String::new(),
            summary: None,
            continuity_state: EpisodeContinuityState::Open,
            actor_ids: Vec::new(),
            goal: None,
            outcome: None,
            started_at_unix_ms: None,
            ended_at_unix_ms: None,
            last_active_unix_ms: None,
            recurrence_key: None,
            recurrence_interval_ms: None,
            boundary_label: None,
            previous_record_id: None,
            next_record_id: None,
            causal_record_ids: Vec::new(),
            related_record_ids: Vec::new(),
            linked_artifact_uris: Vec::new(),
            salience: EpisodeSalience::default(),
            affective: None,
        }
    }
}

impl EpisodeContext {
    pub fn duration_hint_ms(&self) -> Option<u64> {
        match (
            self.started_at_unix_ms,
            self.ended_at_unix_ms,
            self.last_active_unix_ms,
        ) {
            (Some(started), Some(ended), _) if ended >= started => Some(ended - started),
            (Some(started), None, Some(last_active)) if last_active >= started => {
                Some(last_active - started)
            }
            _ => None,
        }
    }

    pub fn validate_for_record(&self, record_id: &str, actor_id: &str) -> Result<()> {
        if self.schema_version != EPISODE_SCHEMA_VERSION {
            return Err(Error::Unsupported(format!(
                "unsupported episode schema version {}; expected {}",
                self.schema_version, EPISODE_SCHEMA_VERSION
            )));
        }
        if self.episode_id.trim().is_empty() {
            return Err(Error::InvalidRequest(
                "episode_id is required when episode context is present".to_string(),
            ));
        }
        if self.previous_record_id.as_deref() == Some(record_id) {
            return Err(Error::InvalidRequest(
                "episode previous_record_id cannot reference the current record".to_string(),
            ));
        }
        if self.next_record_id.as_deref() == Some(record_id) {
            return Err(Error::InvalidRequest(
                "episode next_record_id cannot reference the current record".to_string(),
            ));
        }
        if self
            .causal_record_ids
            .iter()
            .any(|value| value == record_id)
        {
            return Err(Error::InvalidRequest(
                "episode causal_record_ids cannot reference the current record".to_string(),
            ));
        }
        if self
            .related_record_ids
            .iter()
            .any(|value| value == record_id)
        {
            return Err(Error::InvalidRequest(
                "episode related_record_ids cannot reference the current record".to_string(),
            ));
        }
        if let (Some(started_at_unix_ms), Some(ended_at_unix_ms)) =
            (self.started_at_unix_ms, self.ended_at_unix_ms)
            && ended_at_unix_ms < started_at_unix_ms
        {
            return Err(Error::InvalidRequest(
                "episode ended_at_unix_ms cannot be earlier than started_at_unix_ms".to_string(),
            ));
        }
        if let (Some(started_at_unix_ms), Some(last_active_unix_ms)) =
            (self.started_at_unix_ms, self.last_active_unix_ms)
            && last_active_unix_ms < started_at_unix_ms
        {
            return Err(Error::InvalidRequest(
                "episode last_active_unix_ms cannot be earlier than started_at_unix_ms".to_string(),
            ));
        }
        if let (Some(last_active_unix_ms), Some(ended_at_unix_ms)) =
            (self.last_active_unix_ms, self.ended_at_unix_ms)
            && last_active_unix_ms > ended_at_unix_ms
        {
            return Err(Error::InvalidRequest(
                "episode last_active_unix_ms cannot be later than ended_at_unix_ms".to_string(),
            ));
        }
        if self
            .recurrence_key
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(Error::InvalidRequest(
                "episode recurrence_key cannot be empty when provided".to_string(),
            ));
        }
        if self
            .boundary_label
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(Error::InvalidRequest(
                "episode boundary_label cannot be empty when provided".to_string(),
            ));
        }
        if self.recurrence_interval_ms == Some(0) {
            return Err(Error::InvalidRequest(
                "episode recurrence_interval_ms must be greater than zero".to_string(),
            ));
        }
        if self.recurrence_interval_ms.is_some() && self.recurrence_key.is_none() {
            return Err(Error::InvalidRequest(
                "episode recurrence_key is required when recurrence_interval_ms is present"
                    .to_string(),
            ));
        }
        if !self.actor_ids.is_empty() && !self.actor_ids.iter().any(|value| value == actor_id) {
            return Err(Error::InvalidRequest(
                "episode actor_ids must include the owning record actor when provided".to_string(),
            ));
        }
        if let Some(affective) = &self.affective {
            affective.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryRecord {
    pub id: String,
    pub scope: MemoryScope,
    pub kind: MemoryRecordKind,
    pub content: String,
    pub summary: Option<String>,
    pub source_id: Option<String>,
    pub metadata: BTreeMap<String, String>,
    pub quality_state: MemoryQualityState,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
    pub expires_at_unix_ms: Option<u64>,
    pub importance_score: f32,
    pub artifact: Option<ArtifactPointer>,
    #[serde(default)]
    pub episode: Option<EpisodeContext>,
    #[serde(default)]
    pub historical_state: MemoryHistoricalState,
    #[serde(default)]
    pub lineage: Vec<LineageLink>,
}

impl MemoryRecord {
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            return Err(Error::InvalidRequest(
                "memory record id is required".to_string(),
            ));
        }
        if self.scope.tenant_id.trim().is_empty() {
            return Err(Error::InvalidRequest(
                "memory record tenant_id is required".to_string(),
            ));
        }
        if self.scope.namespace.trim().is_empty() {
            return Err(Error::InvalidRequest(
                "memory record namespace is required".to_string(),
            ));
        }
        if self.scope.actor_id.trim().is_empty() {
            return Err(Error::InvalidRequest(
                "memory record actor_id is required".to_string(),
            ));
        }
        if self.content.trim().is_empty() && self.artifact.is_none() {
            return Err(Error::InvalidRequest(
                "memory record content or artifact is required".to_string(),
            ));
        }
        if let Some(episode) = &self.episode {
            episode.validate_for_record(&self.id, &self.scope.actor_id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AffectiveAnnotation, AffectiveAnnotationProvenance, EPISODE_SCHEMA_VERSION, EpisodeContext,
        EpisodeContinuityState, LineageLink, MemoryHistoricalState, MemoryQualityState,
        MemoryRecord, MemoryRecordKind, MemoryScope, MemoryTrustLevel,
    };
    use std::collections::BTreeMap;

    fn scope() -> MemoryScope {
        MemoryScope {
            tenant_id: "default".to_string(),
            namespace: "conversation".to_string(),
            actor_id: "ava".to_string(),
            conversation_id: Some("thread-a".to_string()),
            session_id: Some("session-a".to_string()),
            source: "test".to_string(),
            labels: vec!["shared-fixture".to_string()],
            trust_level: MemoryTrustLevel::Verified,
        }
    }

    #[test]
    fn episode_defaults_and_unresolved_states_are_explicit() {
        let episode = EpisodeContext::default();

        assert_eq!(episode.schema_version, EPISODE_SCHEMA_VERSION);
        assert!(episode.episode_id.is_empty());
        assert_eq!(episode.continuity_state, EpisodeContinuityState::Open);
        assert!(EpisodeContinuityState::Open.is_unresolved());
        assert!(EpisodeContinuityState::Abandoned.is_unresolved());
        assert!(!EpisodeContinuityState::Resolved.is_unresolved());
        assert!(!EpisodeContinuityState::Superseded.is_unresolved());
    }

    #[test]
    fn memory_record_deserializes_missing_additive_fields_with_safe_defaults() {
        let record: MemoryRecord = serde_json::from_value(serde_json::json!({
            "id": "record-1",
            "scope": {
                "tenant_id": "default",
                "namespace": "conversation",
                "actor_id": "ava",
                "conversation_id": "thread-a",
                "session_id": "session-a",
                "source": "test",
                "labels": ["shared-fixture"],
                "trust_level": "Verified"
            },
            "kind": "Fact",
            "content": "Prompt: repair\nAnswer: ok",
            "summary": "ok",
            "source_id": null,
            "metadata": {},
            "quality_state": "Active",
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 1,
            "expires_at_unix_ms": null,
            "importance_score": 0.5,
            "artifact": null
        }))
        .unwrap();

        assert_eq!(record.historical_state, MemoryHistoricalState::Current);
        assert!(record.episode.is_none());
        assert!(record.lineage.is_empty());
    }

    #[test]
    fn episodic_fields_roundtrip_through_json_serialization() {
        let record = MemoryRecord {
            id: "record-episode".to_string(),
            scope: scope(),
            kind: MemoryRecordKind::Episodic,
            content: "Open follow-up for reconnect storm".to_string(),
            summary: Some("Storm follow-up".to_string()),
            source_id: Some("source-1".to_string()),
            metadata: BTreeMap::new(),
            quality_state: MemoryQualityState::Verified,
            created_at_unix_ms: 10,
            updated_at_unix_ms: 20,
            expires_at_unix_ms: None,
            importance_score: 0.9,
            artifact: None,
            episode: Some(EpisodeContext {
                schema_version: EPISODE_SCHEMA_VERSION,
                episode_id: "storm-episode".to_string(),
                summary: Some("Storm remediation episode".to_string()),
                continuity_state: EpisodeContinuityState::Open,
                actor_ids: vec!["ava".to_string(), "ops-bot".to_string()],
                goal: Some("close the reconnect storm follow-up list".to_string()),
                outcome: None,
                started_at_unix_ms: Some(1),
                ended_at_unix_ms: None,
                last_active_unix_ms: Some(20),
                recurrence_key: None,
                recurrence_interval_ms: None,
                boundary_label: None,
                previous_record_id: Some("incident-root".to_string()),
                next_record_id: Some("incident-next".to_string()),
                causal_record_ids: vec!["incident-root".to_string()],
                related_record_ids: vec!["incident-next".to_string()],
                linked_artifact_uris: vec!["file:///tmp/storm.md".to_string()],
                salience: super::EpisodeSalience {
                    reuse_count: 4,
                    novelty_score: 0.3,
                    goal_relevance: 0.95,
                    unresolved_weight: 0.9,
                },
                affective: Some(AffectiveAnnotation {
                    tone: Some("urgent".to_string()),
                    sentiment: Some("concerned".to_string()),
                    urgency: 0.9,
                    confidence: 0.7,
                    tension: 0.6,
                    provenance: AffectiveAnnotationProvenance::Derived,
                }),
            }),
            historical_state: MemoryHistoricalState::Historical,
            lineage: vec![LineageLink {
                record_id: "incident-root".to_string(),
                relation: super::LineageRelationKind::DerivedFrom,
                confidence: 0.8,
            }],
        };

        let encoded = serde_json::to_string(&record).unwrap();
        let decoded: MemoryRecord = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, record);
    }

    #[test]
    fn memory_record_rejects_invalid_episode_association_rules() {
        let mut record = MemoryRecord {
            id: "record-episode".to_string(),
            scope: scope(),
            kind: MemoryRecordKind::Episodic,
            content: "Open follow-up for reconnect storm".to_string(),
            summary: Some("Storm follow-up".to_string()),
            source_id: Some("source-1".to_string()),
            metadata: BTreeMap::new(),
            quality_state: MemoryQualityState::Verified,
            created_at_unix_ms: 10,
            updated_at_unix_ms: 20,
            expires_at_unix_ms: None,
            importance_score: 0.9,
            artifact: None,
            episode: Some(EpisodeContext {
                schema_version: EPISODE_SCHEMA_VERSION,
                episode_id: "storm-episode".to_string(),
                summary: Some("Storm remediation episode".to_string()),
                continuity_state: EpisodeContinuityState::Open,
                actor_ids: vec!["ava".to_string()],
                goal: Some("close the reconnect storm follow-up list".to_string()),
                outcome: None,
                started_at_unix_ms: Some(1),
                ended_at_unix_ms: None,
                last_active_unix_ms: Some(20),
                recurrence_key: None,
                recurrence_interval_ms: None,
                boundary_label: None,
                previous_record_id: Some("record-episode".to_string()),
                next_record_id: None,
                causal_record_ids: vec![],
                related_record_ids: vec![],
                linked_artifact_uris: vec![],
                salience: super::EpisodeSalience::default(),
                affective: None,
            }),
            historical_state: MemoryHistoricalState::Current,
            lineage: vec![],
        };

        let error = record.validate().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("previous_record_id cannot reference the current record")
        );

        record.episode.as_mut().unwrap().previous_record_id = Some("incident-root".to_string());
        record.episode.as_mut().unwrap().actor_ids = vec!["ops-bot".to_string()];
        let error = record.validate().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("actor_ids must include the owning record actor")
        );
    }

    #[test]
    fn episode_duration_and_recurrence_require_coherent_values() {
        let mut episode = EpisodeContext {
            schema_version: EPISODE_SCHEMA_VERSION,
            episode_id: "release-retro".to_string(),
            summary: Some("Recurring release retrospective".to_string()),
            continuity_state: EpisodeContinuityState::Open,
            actor_ids: vec!["ava".to_string()],
            goal: Some("review each release boundary".to_string()),
            outcome: None,
            started_at_unix_ms: Some(10),
            ended_at_unix_ms: Some(40),
            last_active_unix_ms: Some(40),
            recurrence_key: Some("release-retro-weekly".to_string()),
            recurrence_interval_ms: Some(7 * 24 * 60 * 60 * 1000),
            boundary_label: Some("weekly-release-boundary".to_string()),
            previous_record_id: None,
            next_record_id: None,
            causal_record_ids: vec![],
            related_record_ids: vec![],
            linked_artifact_uris: vec![],
            salience: super::EpisodeSalience::default(),
            affective: None,
        };

        assert_eq!(episode.duration_hint_ms(), Some(30));
        episode.validate_for_record("retro-1", "ava").unwrap();

        episode.recurrence_key = None;
        let error = episode.validate_for_record("retro-1", "ava").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("recurrence_key is required when recurrence_interval_ms is present")
        );
    }

    #[test]
    fn derived_affective_annotations_require_bounded_confidence() {
        let annotation = AffectiveAnnotation {
            tone: Some("urgent".to_string()),
            sentiment: Some("concerned".to_string()),
            urgency: 0.8,
            confidence: 1.0,
            tension: 0.6,
            provenance: AffectiveAnnotationProvenance::Derived,
        };

        let error = annotation.validate().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("derived affective confidence must remain below certainty")
        );
    }
}
