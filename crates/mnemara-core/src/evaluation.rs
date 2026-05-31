use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::Result;
use crate::query::{RecallQuery, RecallResult};
use crate::store::MemoryStore;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JudgedRecallCase {
    pub name: String,
    pub query: RecallQuery,
    pub relevant_record_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct RecallEvaluationAssertions {
    pub expected_record_ids: Vec<String>,
    pub optional_record_ids: Vec<String>,
    pub disallowed_record_ids: Vec<String>,
    pub required_explanation_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecallEvaluationCase {
    pub name: String,
    pub query: RecallQuery,
    pub assertions: RecallEvaluationAssertions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecallEvaluationCaseReport {
    pub name: String,
    pub passed: bool,
    pub ranked_record_ids: Vec<String>,
    pub missing_expected_record_ids: Vec<String>,
    pub present_disallowed_record_ids: Vec<String>,
    pub missing_explanation_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecallEvaluationReport {
    pub cases: usize,
    pub passed_cases: usize,
    pub failed_cases: usize,
    pub pass_rate: f32,
    pub ranking_metrics: RankingMetrics,
    pub case_reports: Vec<RecallEvaluationCaseReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RankingMetrics {
    pub cases: usize,
    pub hit_rate_at_k: f32,
    pub recall_at_k: f32,
    pub mrr: f32,
    pub ndcg_at_k: f32,
}

pub fn evaluate_rankings_at_k(rankings: &[(&[String], &[String])], k: usize) -> RankingMetrics {
    if rankings.is_empty() || k == 0 {
        return RankingMetrics {
            cases: rankings.len(),
            hit_rate_at_k: 0.0,
            recall_at_k: 0.0,
            mrr: 0.0,
            ndcg_at_k: 0.0,
        };
    }

    let mut hits = 0.0f32;
    let mut recall = 0.0f32;
    let mut reciprocal_rank = 0.0f32;
    let mut ndcg = 0.0f32;

    for (ranked_ids, relevant_ids) in rankings {
        let relevant = relevant_ids.iter().cloned().collect::<BTreeSet<_>>();
        if relevant.is_empty() {
            continue;
        }
        let top_k = ranked_ids.iter().take(k).collect::<Vec<_>>();
        let matches = top_k
            .iter()
            .filter(|record_id| relevant.contains(record_id.as_str()))
            .count() as f32;
        if matches > 0.0 {
            hits += 1.0;
        }
        recall += matches / relevant.len() as f32;
        if let Some(rank) = ranked_ids
            .iter()
            .position(|record_id| relevant.contains(record_id.as_str()))
        {
            reciprocal_rank += 1.0 / (rank as f32 + 1.0);
        }

        let dcg = top_k
            .iter()
            .enumerate()
            .filter(|(_, record_id)| relevant.contains(record_id.as_str()))
            .map(|(index, _)| 1.0 / ((index as f32 + 2.0).log2()))
            .sum::<f32>();
        let ideal_hits = relevant.len().min(k);
        let ideal_dcg = (0..ideal_hits)
            .map(|index| 1.0 / ((index as f32 + 2.0).log2()))
            .sum::<f32>();
        if ideal_dcg > 0.0 {
            ndcg += dcg / ideal_dcg;
        }
    }

    let cases = rankings.len() as f32;
    RankingMetrics {
        cases: rankings.len(),
        hit_rate_at_k: hits / cases,
        recall_at_k: recall / cases,
        mrr: reciprocal_rank / cases,
        ndcg_at_k: ndcg / cases,
    }
}

pub async fn run_recall_evaluation<S>(
    store: &S,
    cases: &[RecallEvaluationCase],
    k: usize,
) -> Result<RecallEvaluationReport>
where
    S: MemoryStore + ?Sized,
{
    let mut results = Vec::with_capacity(cases.len());
    for case in cases {
        results.push(store.recall(case.query.clone()).await?);
    }
    Ok(evaluate_recall_results(cases, &results, k))
}

pub fn evaluate_recall_results(
    cases: &[RecallEvaluationCase],
    results: &[RecallResult],
    k: usize,
) -> RecallEvaluationReport {
    let mut ranked = Vec::<Vec<String>>::with_capacity(results.len());
    let mut relevant = Vec::<Vec<String>>::with_capacity(cases.len());
    let mut case_reports = Vec::with_capacity(cases.len());

    for (case, result) in cases.iter().zip(results.iter()) {
        let ranked_record_ids = result
            .hits
            .iter()
            .map(|hit| hit.record.id.clone())
            .collect::<Vec<_>>();
        let ranked_set = ranked_record_ids.iter().cloned().collect::<BTreeSet<_>>();
        let expected_set = case
            .assertions
            .expected_record_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let optional = case
            .assertions
            .optional_record_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut relevant_ids = expected_set
            .union(&optional)
            .cloned()
            .collect::<Vec<String>>();
        relevant_ids.sort();

        let missing_expected_record_ids = expected_set
            .difference(&ranked_set)
            .cloned()
            .collect::<Vec<_>>();
        let present_disallowed_record_ids = case
            .assertions
            .disallowed_record_ids
            .iter()
            .filter(|record_id| ranked_set.contains(*record_id))
            .cloned()
            .collect::<Vec<_>>();

        let explanation_notes = result
            .explanation
            .as_ref()
            .map(|explanation| explanation.policy_notes.as_slice())
            .unwrap_or_default();
        let missing_explanation_notes = case
            .assertions
            .required_explanation_notes
            .iter()
            .filter(|required| {
                !explanation_notes
                    .iter()
                    .any(|note| note.contains(required.as_str()))
            })
            .cloned()
            .collect::<Vec<_>>();

        let passed = missing_expected_record_ids.is_empty()
            && present_disallowed_record_ids.is_empty()
            && missing_explanation_notes.is_empty();

        ranked.push(ranked_record_ids.clone());
        relevant.push(relevant_ids);
        case_reports.push(RecallEvaluationCaseReport {
            name: case.name.clone(),
            passed,
            ranked_record_ids,
            missing_expected_record_ids,
            present_disallowed_record_ids,
            missing_explanation_notes,
        });
    }

    let ranking_pairs = ranked
        .iter()
        .zip(relevant.iter())
        .map(|(ranked_ids, relevant_ids)| (ranked_ids.as_slice(), relevant_ids.as_slice()))
        .collect::<Vec<_>>();
    let passed_cases = case_reports.iter().filter(|report| report.passed).count();
    let cases_len = cases.len();

    RecallEvaluationReport {
        cases: cases_len,
        passed_cases,
        failed_cases: cases_len.saturating_sub(passed_cases),
        pass_rate: if cases_len == 0 {
            0.0
        } else {
            passed_cases as f32 / cases_len as f32
        },
        ranking_metrics: evaluate_rankings_at_k(&ranking_pairs, k),
        case_reports,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RecallEvaluationAssertions, RecallEvaluationCase, evaluate_rankings_at_k,
        evaluate_recall_results,
    };
    use crate::{
        MemoryQualityState, MemoryRecord, MemoryRecordKind, MemoryScope, MemoryTrustLevel,
        RecallExplanation, RecallFilters, RecallHit, RecallQuery, RecallResult,
        RecallScoreBreakdown,
    };
    use std::collections::BTreeMap;

    #[test]
    fn computes_standard_ranking_metrics() {
        let ranked_a = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let relevant_a = vec!["b".to_string(), "d".to_string()];
        let ranked_b = vec!["x".to_string(), "y".to_string(), "z".to_string()];
        let relevant_b = vec!["x".to_string()];

        let metrics =
            evaluate_rankings_at_k(&[(&ranked_a, &relevant_a), (&ranked_b, &relevant_b)], 3);

        assert_eq!(metrics.cases, 2);
        assert!(metrics.hit_rate_at_k > 0.9);
        assert!(metrics.recall_at_k > 0.7);
        assert!(metrics.mrr > 0.6);
        assert!(metrics.ndcg_at_k > 0.6);
    }

    fn scope() -> MemoryScope {
        MemoryScope {
            tenant_id: "tenant-a".to_string(),
            namespace: "ops".to_string(),
            actor_id: "ava".to_string(),
            conversation_id: None,
            session_id: None,
            source: "test".to_string(),
            labels: vec!["eval".to_string()],
            trust_level: MemoryTrustLevel::Verified,
        }
    }

    fn hit(record_id: &str) -> RecallHit {
        RecallHit {
            record: MemoryRecord {
                id: record_id.to_string(),
                scope: scope(),
                kind: MemoryRecordKind::Fact,
                content: format!("record {record_id}"),
                summary: None,
                source_id: None,
                metadata: BTreeMap::new(),
                quality_state: MemoryQualityState::Active,
                created_at_unix_ms: 1,
                updated_at_unix_ms: 1,
                expires_at_unix_ms: None,
                importance_score: 0.5,
                artifact: None,
                episode: None,
                historical_state: Default::default(),
                lineage: Vec::new(),
                conflict: None,
            },
            breakdown: RecallScoreBreakdown {
                lexical: 1.0,
                semantic: 0.0,
                graph: 0.0,
                temporal: 0.0,
                metadata: 0.0,
                episodic: 0.0,
                salience: 0.0,
                curation: 0.0,
                policy: 0.0,
                total: 1.0,
            },
            explanation: None,
        }
    }

    #[test]
    fn evaluates_judged_recall_cases_with_assertions() {
        let query = RecallQuery {
            scope: scope(),
            query_text: "release".to_string(),
            max_items: 3,
            token_budget: None,
            filters: RecallFilters::default(),
            include_explanation: true,
        };
        let cases = vec![RecallEvaluationCase {
            name: "release memory".to_string(),
            query,
            assertions: RecallEvaluationAssertions {
                expected_record_ids: vec!["a".to_string()],
                optional_record_ids: vec!["b".to_string()],
                disallowed_record_ids: vec!["x".to_string()],
                required_explanation_notes: vec!["policy=general".to_string()],
            },
        }];
        let results = vec![RecallResult {
            hits: vec![hit("a"), hit("b")],
            total_candidates_examined: 2,
            explanation: Some(RecallExplanation {
                selected_channels: vec!["lexical".to_string()],
                policy_notes: vec!["recall_policy=general".to_string()],
                trace_id: None,
                planning_trace: None,
                planning_profile: None,
                policy_profile: None,
                scorer_kind: None,
                scoring_profile: None,
            }),
        }];

        let report = evaluate_recall_results(&cases, &results, 3);

        assert_eq!(report.cases, 1);
        assert_eq!(report.passed_cases, 1);
        assert_eq!(report.failed_cases, 0);
        assert!(report.pass_rate > 0.99);
        assert!(report.ranking_metrics.hit_rate_at_k > 0.99);
        assert!(report.case_reports[0].passed);
    }
}
