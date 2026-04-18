use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::query::RecallQuery;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JudgedRecallCase {
    pub name: String,
    pub query: RecallQuery,
    pub relevant_record_ids: Vec<String>,
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

#[cfg(test)]
mod tests {
    use super::evaluate_rankings_at_k;

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
}
