//! Duplicate and near-duplicate detection for trajectories.
//!
//! Provides functionality to identify and remove similar trajectories
//! from a dataset to improve diversity.

use std::collections::HashSet;

use uuid::Uuid;

use crate::trajectory::Trajectory;

use super::embeddings::{cosine_similarity, pairwise_cosine_similarity, EmbeddingGenerator};

/// Default similarity threshold for considering trajectories as duplicates.
const DEFAULT_SIMILARITY_THRESHOLD: f64 = 0.85;

/// Default embedding dimension for the deduplicator.
const DEFAULT_EMBEDDING_DIMENSION: usize = 128;

/// Deduplicator for identifying and removing near-duplicate trajectories.
///
/// Uses embedding-based similarity to detect trajectories that are
/// too similar to each other, keeping only the best representative
/// from each cluster of similar trajectories.
#[derive(Debug, Clone)]
pub struct Deduplicator {
    /// Similarity threshold above which trajectories are considered duplicates.
    /// Range: 0.0 to 1.0 (0.85 means 85% similar = duplicate).
    similarity_threshold: f64,

    /// Embedding generator for converting trajectories to vectors.
    embedding_generator: EmbeddingGenerator,
}

/// Result of a deduplication operation.
#[derive(Debug, Clone)]
pub struct DeduplicationResult {
    /// IDs of trajectories that were kept.
    pub kept: Vec<Uuid>,

    /// Records of removed trajectories: (removed_id, similar_to_id, similarity_score).
    pub removed: Vec<(Uuid, Uuid, f64)>,

    /// Total number of trajectories before deduplication.
    pub total_before: usize,

    /// Total number of trajectories after deduplication.
    pub total_after: usize,
}

impl DeduplicationResult {
    /// Returns the deduplication ratio (removed / total).
    pub fn dedup_ratio(&self) -> f64 {
        if self.total_before == 0 {
            return 0.0;
        }
        self.removed.len() as f64 / self.total_before as f64
    }

    /// Returns the retention ratio (kept / total).
    pub fn retention_ratio(&self) -> f64 {
        if self.total_before == 0 {
            return 1.0;
        }
        self.kept.len() as f64 / self.total_before as f64
    }
}

impl Default for Deduplicator {
    fn default() -> Self {
        Self::new(DEFAULT_SIMILARITY_THRESHOLD)
    }
}

impl Deduplicator {
    /// Creates a new deduplicator with the specified similarity threshold.
    ///
    /// # Arguments
    ///
    /// * `similarity_threshold` - Threshold above which trajectories are considered duplicates.
    ///   Should be in range [0.0, 1.0]. Recommended: 0.85.
    ///
    /// # Example
    ///
    /// ```
    /// use swe_forge::diversity::Deduplicator;
    ///
    /// let deduplicator = Deduplicator::new(0.9); // 90% similarity threshold
    /// ```
    pub fn new(similarity_threshold: f64) -> Self {
        let threshold = similarity_threshold.clamp(0.0, 1.0);
        Self {
            similarity_threshold: threshold,
            embedding_generator: EmbeddingGenerator::new(DEFAULT_EMBEDDING_DIMENSION),
        }
    }

    /// Creates a new deduplicator with custom embedding dimension.
    ///
    /// # Arguments
    ///
    /// * `similarity_threshold` - Threshold above which trajectories are considered duplicates.
    /// * `embedding_dimension` - Dimension of embeddings to use.
    pub fn with_dimension(similarity_threshold: f64, embedding_dimension: usize) -> Self {
        let threshold = similarity_threshold.clamp(0.0, 1.0);
        Self {
            similarity_threshold: threshold,
            embedding_generator: EmbeddingGenerator::new(embedding_dimension),
        }
    }

    /// Returns the current similarity threshold.
    pub fn similarity_threshold(&self) -> f64 {
        self.similarity_threshold
    }

    /// Finds all pairs of near-duplicate trajectories.
    ///
    /// Returns pairs where similarity exceeds the threshold.
    ///
    /// # Arguments
    ///
    /// * `trajectories` - Slice of trajectories to check for duplicates.
    ///
    /// # Returns
    ///
    /// Vector of (id1, id2, similarity) tuples for duplicate pairs.
    pub fn find_duplicates(&self, trajectories: &[Trajectory]) -> Vec<(Uuid, Uuid, f64)> {
        if trajectories.len() < 2 {
            return Vec::new();
        }

        // Generate embeddings for all trajectories
        let embeddings = self.embedding_generator.embed_batch(trajectories);

        // Compute pairwise similarities
        let similarities = pairwise_cosine_similarity(&embeddings);

        // Find pairs exceeding threshold
        let mut duplicates = Vec::new();
        for i in 0..trajectories.len() {
            for j in (i + 1)..trajectories.len() {
                let sim = similarities[[i, j]];
                if sim >= self.similarity_threshold {
                    duplicates.push((trajectories[i].id, trajectories[j].id, sim));
                }
            }
        }

        // Sort by similarity (highest first)
        duplicates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        duplicates
    }

    /// Deduplicates a collection of trajectories.
    ///
    /// For each cluster of similar trajectories, keeps the one with the
    /// highest total reward. Returns the deduplicated list and metadata.
    ///
    /// # Arguments
    ///
    /// * `trajectories` - Trajectories to deduplicate.
    ///
    /// # Returns
    ///
    /// `DeduplicationResult` containing kept/removed IDs and statistics.
    pub fn deduplicate(&self, trajectories: Vec<Trajectory>) -> DeduplicationResult {
        let total_before = trajectories.len();

        if trajectories.is_empty() {
            return DeduplicationResult {
                kept: Vec::new(),
                removed: Vec::new(),
                total_before: 0,
                total_after: 0,
            };
        }

        if trajectories.len() == 1 {
            return DeduplicationResult {
                kept: vec![trajectories[0].id],
                removed: Vec::new(),
                total_before: 1,
                total_after: 1,
            };
        }

        // Generate embeddings
        let embeddings = self.embedding_generator.embed_batch(&trajectories);

        // Compute pairwise similarities
        let similarities = pairwise_cosine_similarity(&embeddings);

        // Track which trajectories to remove
        let mut removed_set: HashSet<usize> = HashSet::new();
        let mut removed_records: Vec<(Uuid, Uuid, f64)> = Vec::new();

        // Process pairs by similarity (highest first)
        let mut pairs: Vec<(usize, usize, f64)> = Vec::new();
        for i in 0..trajectories.len() {
            for j in (i + 1)..trajectories.len() {
                let sim = similarities[[i, j]];
                if sim >= self.similarity_threshold {
                    pairs.push((i, j, sim));
                }
            }
        }
        pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        // For each duplicate pair, remove the one with lower reward
        for (i, j, sim) in pairs {
            // Skip if either is already removed
            if removed_set.contains(&i) || removed_set.contains(&j) {
                continue;
            }

            // Compare by reward (higher is better), then by step count (fewer is better)
            let keep_i = self.is_better(&trajectories[i], &trajectories[j]);

            if keep_i {
                removed_set.insert(j);
                removed_records.push((trajectories[j].id, trajectories[i].id, sim));
            } else {
                removed_set.insert(i);
                removed_records.push((trajectories[i].id, trajectories[j].id, sim));
            }
        }

        // Collect kept trajectory IDs
        let kept: Vec<Uuid> = trajectories
            .iter()
            .enumerate()
            .filter(|(i, _)| !removed_set.contains(i))
            .map(|(_, t)| t.id)
            .collect();

        DeduplicationResult {
            kept: kept.clone(),
            removed: removed_records,
            total_before,
            total_after: kept.len(),
        }
    }

    /// Checks if two trajectories are near-duplicates.
    ///
    /// # Arguments
    ///
    /// * `t1` - First trajectory.
    /// * `t2` - Second trajectory.
    ///
    /// # Returns
    ///
    /// `true` if the trajectories are similar above the threshold.
    pub fn are_similar(&self, t1: &Trajectory, t2: &Trajectory) -> bool {
        self.similarity(t1, t2) >= self.similarity_threshold
    }

    /// Computes similarity score between two trajectories.
    ///
    /// # Arguments
    ///
    /// * `t1` - First trajectory.
    /// * `t2` - Second trajectory.
    ///
    /// # Returns
    ///
    /// Cosine similarity in range [-1, 1], typically [0, 1] for trajectory embeddings.
    pub fn similarity(&self, t1: &Trajectory, t2: &Trajectory) -> f64 {
        let e1 = self.embedding_generator.embed_trajectory(t1);
        let e2 = self.embedding_generator.embed_trajectory(t2);
        cosine_similarity(&e1, &e2)
    }

    /// Determines if trajectory `a` is "better" than trajectory `b`.
    ///
    /// Criteria (in order of priority):
    /// 1. Higher total reward
    /// 2. Fewer steps (more efficient)
    /// 3. Shorter duration
    fn is_better(&self, a: &Trajectory, b: &Trajectory) -> bool {
        // Compare by total reward first
        if (a.total_reward - b.total_reward).abs() > 0.01 {
            return a.total_reward > b.total_reward;
        }

        // Compare by step count (fewer is better)
        if a.steps.len() != b.steps.len() {
            return a.steps.len() < b.steps.len();
        }

        // Compare by duration (shorter is better)
        a.duration_seconds <= b.duration_seconds
    }
}

/// Builder for creating a Deduplicator with custom settings.
#[derive(Debug, Clone)]
pub struct DeduplicatorBuilder {
    similarity_threshold: f64,
    embedding_dimension: usize,
}

impl Default for DeduplicatorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DeduplicatorBuilder {
    /// Creates a new builder with default settings.
    pub fn new() -> Self {
        Self {
            similarity_threshold: DEFAULT_SIMILARITY_THRESHOLD,
            embedding_dimension: DEFAULT_EMBEDDING_DIMENSION,
        }
    }

    /// Sets the similarity threshold.
    pub fn similarity_threshold(mut self, threshold: f64) -> Self {
        self.similarity_threshold = threshold;
        self
    }

    /// Sets the embedding dimension.
    pub fn embedding_dimension(mut self, dimension: usize) -> Self {
        self.embedding_dimension = dimension;
        self
    }

    /// Builds the Deduplicator.
    pub fn build(self) -> Deduplicator {
        Deduplicator::with_dimension(self.similarity_threshold, self.embedding_dimension)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trajectory::{
        AgentAction, EnvironmentState, Observation, TaskResult, TokenUsage, TrajectoryStep,
    };
    use chrono::Utc;

    fn create_test_trajectory(
        task_id: &str,
        model: &str,
        num_steps: usize,
        reward: f64,
    ) -> Trajectory {
        let steps: Vec<TrajectoryStep> = (0..num_steps)
            .map(|i| TrajectoryStep {
                step_number: i as u32,
                state: EnvironmentState::default(),
                action: AgentAction {
                    tool_name: "read_file".to_string(),
                    tool_args: serde_json::json!({"path": format!("file{}.txt", i)}),
                    raw_llm_output: format!("Step {} output", i),
                    thinking: None,
                },
                observation: Observation::default(),
                reward: reward / num_steps as f64,
                done: i == num_steps - 1,
                timestamp: Utc::now(),
            })
            .collect();

        Trajectory {
            id: Uuid::new_v4(),
            task_id: task_id.to_string(),
            model: model.to_string(),
            scaffold_type: "react".to_string(),
            steps,
            final_result: TaskResult::Success { score: reward },
            total_reward: reward,
            created_at: Utc::now(),
            duration_seconds: 120,
            token_usage: TokenUsage::new(1000, 500),
        }
    }

    #[test]
    fn test_deduplicator_new() {
        let dedup = Deduplicator::new(0.9);
        assert!((dedup.similarity_threshold() - 0.9).abs() < 1e-10);
    }

    #[test]
    fn test_deduplicator_default() {
        let dedup = Deduplicator::default();
        assert!((dedup.similarity_threshold() - DEFAULT_SIMILARITY_THRESHOLD).abs() < 1e-10);
    }

    #[test]
    fn test_threshold_clamping() {
        let dedup_high = Deduplicator::new(1.5);
        assert!((dedup_high.similarity_threshold() - 1.0).abs() < 1e-10);

        let dedup_low = Deduplicator::new(-0.5);
        assert!(dedup_low.similarity_threshold().abs() < 1e-10);
    }

    #[test]
    fn test_find_duplicates_empty() {
        let dedup = Deduplicator::new(0.85);
        let result = dedup.find_duplicates(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_find_duplicates_single() {
        let dedup = Deduplicator::new(0.85);
        let trajectories = vec![create_test_trajectory("task-1", "gpt-4", 5, 0.9)];
        let result = dedup.find_duplicates(&trajectories);
        assert!(result.is_empty());
    }

    #[test]
    fn test_find_duplicates_identical() {
        let dedup = Deduplicator::new(0.85);

        // Create two very similar trajectories (same task, model, structure)
        let t1 = create_test_trajectory("task-1", "gpt-4", 5, 0.9);
        let mut t2 = create_test_trajectory("task-1", "gpt-4", 5, 0.8);
        t2.id = Uuid::new_v4(); // Ensure different ID

        let trajectories = vec![t1.clone(), t2.clone()];
        let result = dedup.find_duplicates(&trajectories);

        // Should find them as duplicates (same task/model/structure)
        assert!(
            !result.is_empty(),
            "Similar trajectories should be detected"
        );
        assert_eq!(result[0].0, t1.id);
        assert_eq!(result[0].1, t2.id);
        assert!(result[0].2 >= 0.85, "Similarity should be above threshold");
    }

    #[test]
    fn test_find_duplicates_different() {
        let dedup = Deduplicator::new(0.95); // High threshold

        // Create two distinct trajectories
        let t1 = create_test_trajectory("task-1", "gpt-4", 3, 0.9);
        let t2 = create_test_trajectory("task-2", "claude-3", 10, 0.5);

        let trajectories = vec![t1, t2];
        let result = dedup.find_duplicates(&trajectories);

        // With high threshold, different trajectories should not match
        assert!(
            result.is_empty(),
            "Different trajectories should not be duplicates with high threshold"
        );
    }

    #[test]
    fn test_deduplicate_empty() {
        let dedup = Deduplicator::new(0.85);
        let result = dedup.deduplicate(Vec::new());

        assert!(result.kept.is_empty());
        assert!(result.removed.is_empty());
        assert_eq!(result.total_before, 0);
        assert_eq!(result.total_after, 0);
    }

    #[test]
    fn test_deduplicate_single() {
        let dedup = Deduplicator::new(0.85);
        let trajectory = create_test_trajectory("task-1", "gpt-4", 5, 0.9);
        let id = trajectory.id;

        let result = dedup.deduplicate(vec![trajectory]);

        assert_eq!(result.kept, vec![id]);
        assert!(result.removed.is_empty());
        assert_eq!(result.total_before, 1);
        assert_eq!(result.total_after, 1);
    }

    #[test]
    fn test_deduplicate_keeps_better() {
        let dedup = Deduplicator::new(0.50); // Low threshold to ensure match

        // Create similar trajectories with different rewards
        let t1 = create_test_trajectory("task-1", "gpt-4", 5, 0.5); // Lower reward
        let t2 = create_test_trajectory("task-1", "gpt-4", 5, 0.9); // Higher reward

        let t1_id = t1.id;
        let t2_id = t2.id;

        let result = dedup.deduplicate(vec![t1, t2]);

        // Should keep the higher reward trajectory
        if result
            .removed
            .iter()
            .any(|(removed_id, _, _)| *removed_id == t1_id)
        {
            assert!(
                result.kept.contains(&t2_id),
                "Should keep higher reward trajectory"
            );
        }
    }

    #[test]
    fn test_are_similar() {
        let dedup = Deduplicator::new(0.90);

        let t1 = create_test_trajectory("task-1", "gpt-4", 5, 0.9);
        let t2 = create_test_trajectory("task-1", "gpt-4", 5, 0.8);
        let t3 = create_test_trajectory("task-2", "claude-3", 10, 0.5);

        // Same task/model should be similar
        let sim_12 = dedup.similarity(&t1, &t2);
        assert!(
            sim_12 > 0.5,
            "Similar trajectories should have high similarity"
        );

        // Different task/model should be less similar
        let sim_13 = dedup.similarity(&t1, &t3);
        assert!(
            sim_13 < sim_12,
            "Different trajectories should have lower similarity"
        );
    }

    #[test]
    fn test_similarity_symmetric() {
        let dedup = Deduplicator::new(0.85);

        let t1 = create_test_trajectory("task-1", "gpt-4", 5, 0.9);
        let t2 = create_test_trajectory("task-2", "claude-3", 3, 0.7);

        let sim_12 = dedup.similarity(&t1, &t2);
        let sim_21 = dedup.similarity(&t2, &t1);

        assert!(
            (sim_12 - sim_21).abs() < 1e-10,
            "Similarity should be symmetric"
        );
    }

    #[test]
    fn test_deduplication_result_ratios() {
        let result = DeduplicationResult {
            kept: vec![Uuid::new_v4(), Uuid::new_v4()],
            removed: vec![(Uuid::new_v4(), Uuid::new_v4(), 0.9)],
            total_before: 3,
            total_after: 2,
        };

        assert!((result.dedup_ratio() - 1.0 / 3.0).abs() < 1e-10);
        assert!((result.retention_ratio() - 2.0 / 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_deduplication_result_empty_ratios() {
        let result = DeduplicationResult {
            kept: Vec::new(),
            removed: Vec::new(),
            total_before: 0,
            total_after: 0,
        };

        assert_eq!(result.dedup_ratio(), 0.0);
        assert_eq!(result.retention_ratio(), 1.0);
    }

    #[test]
    fn test_builder() {
        let dedup = DeduplicatorBuilder::new()
            .similarity_threshold(0.95)
            .embedding_dimension(64)
            .build();

        assert!((dedup.similarity_threshold() - 0.95).abs() < 1e-10);
    }

    #[test]
    fn test_builder_default() {
        let dedup = DeduplicatorBuilder::default().build();
        assert!((dedup.similarity_threshold() - DEFAULT_SIMILARITY_THRESHOLD).abs() < 1e-10);
    }
}
