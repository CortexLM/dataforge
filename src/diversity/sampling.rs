//! Diverse sampling strategies for trajectory selection.
//!
//! Provides multiple strategies to select a diverse subset of trajectories
//! from a larger pool, ensuring variety in the final dataset.

use std::collections::HashMap;

use ndarray::Array2;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::trajectory::Trajectory;

use super::embeddings::{pairwise_euclidean_distance, EmbeddingGenerator};

/// Default embedding dimension for the sampler.
const DEFAULT_EMBEDDING_DIMENSION: usize = 128;

/// Diverse sampler for selecting varied trajectories from a pool.
///
/// Supports multiple sampling strategies to maximize diversity
/// in the selected subset.
#[derive(Debug, Clone)]
pub struct DiverseSampler {
    /// Embedding generator for distance calculations.
    embedding_generator: EmbeddingGenerator,

    /// Random seed for reproducibility (None = non-deterministic).
    seed: Option<u64>,
}

/// Available sampling strategies for diverse selection.
#[derive(Debug, Clone)]
pub enum SamplingStrategy {
    /// Random uniform sampling.
    Random,

    /// Maximize the minimum distance between any two selected samples.
    /// Greedy algorithm that iteratively picks the point farthest from the current set.
    MaxMinDistance,

    /// Stratified sampling by category.
    /// Ensures proportional representation from each category.
    Stratified {
        /// Categories to stratify by.
        categories: Vec<String>,
    },

    /// Cluster-based sampling using k-medoids.
    /// Groups trajectories into k clusters, then samples from each cluster.
    ClusterBased {
        /// Number of clusters to create.
        k: usize,
    },
}

impl Default for DiverseSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl DiverseSampler {
    /// Creates a new diverse sampler with default settings.
    pub fn new() -> Self {
        Self {
            embedding_generator: EmbeddingGenerator::new(DEFAULT_EMBEDDING_DIMENSION),
            seed: None,
        }
    }

    /// Creates a new sampler with custom embedding dimension.
    pub fn with_dimension(dimension: usize) -> Self {
        Self {
            embedding_generator: EmbeddingGenerator::new(dimension),
            seed: None,
        }
    }

    /// Sets a random seed for reproducibility.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Samples n diverse trajectories from a pool.
    ///
    /// # Arguments
    ///
    /// * `pool` - The pool of trajectories to sample from.
    /// * `n` - Number of trajectories to select.
    /// * `strategy` - The sampling strategy to use.
    ///
    /// # Returns
    ///
    /// Vector of selected trajectories.
    pub fn sample(
        &self,
        pool: &[Trajectory],
        n: usize,
        strategy: SamplingStrategy,
    ) -> Vec<Trajectory> {
        if pool.is_empty() || n == 0 {
            return Vec::new();
        }

        let n = n.min(pool.len());

        let indices = match strategy {
            SamplingStrategy::Random => self.random_sample(pool.len(), n),
            SamplingStrategy::MaxMinDistance => self.maxmin_sample(pool, n),
            SamplingStrategy::Stratified { categories } => {
                self.stratified_sample(pool, n, &categories)
            }
            SamplingStrategy::ClusterBased { k } => self.cluster_sample(pool, n, k),
        };

        indices.into_iter().map(|i| pool[i].clone()).collect()
    }

    /// Random uniform sampling.
    fn random_sample(&self, pool_size: usize, n: usize) -> Vec<usize> {
        let mut rng = self.create_rng();
        let mut indices: Vec<usize> = (0..pool_size).collect();
        indices.shuffle(&mut rng);
        indices.truncate(n);
        indices
    }

    /// Max-min distance greedy sampling.
    ///
    /// Algorithm:
    /// 1. Start with a random seed point
    /// 2. Iteratively add the point with maximum minimum distance to the current set
    /// 3. Repeat until n points are selected
    fn maxmin_sample(&self, pool: &[Trajectory], n: usize) -> Vec<usize> {
        if pool.len() <= n {
            return (0..pool.len()).collect();
        }

        // Compute embeddings and distance matrix
        let embeddings = self.embedding_generator.embed_batch(pool);
        let distances = pairwise_euclidean_distance(&embeddings);

        let mut selected: Vec<usize> = Vec::with_capacity(n);
        let mut min_distances: Vec<f64> = vec![f64::MAX; pool.len()];

        // Start with a random seed point
        let mut rng = self.create_rng();
        let seed_idx = (0..pool.len())
            .collect::<Vec<_>>()
            .choose(&mut rng)
            .copied()
            .unwrap_or(0);
        selected.push(seed_idx);

        // Update min distances from the seed
        for j in 0..pool.len() {
            if j != seed_idx {
                min_distances[j] = distances[[seed_idx, j]];
            }
        }

        // Greedily select remaining points
        while selected.len() < n {
            // Find point with maximum minimum distance
            let mut best_idx = 0;
            let mut best_dist = f64::NEG_INFINITY;

            for (j, &min_dist) in min_distances.iter().enumerate() {
                if !selected.contains(&j) && min_dist > best_dist {
                    best_dist = min_dist;
                    best_idx = j;
                }
            }

            selected.push(best_idx);

            // Update min distances
            for j in 0..pool.len() {
                if !selected.contains(&j) {
                    let dist_to_new = distances[[best_idx, j]];
                    min_distances[j] = min_distances[j].min(dist_to_new);
                }
            }
        }

        selected
    }

    /// Stratified sampling by category.
    ///
    /// Ensures proportional representation from each category.
    fn stratified_sample(
        &self,
        pool: &[Trajectory],
        n: usize,
        categories: &[String],
    ) -> Vec<usize> {
        if pool.len() <= n {
            return (0..pool.len()).collect();
        }

        // Group trajectories by category
        let mut category_indices: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, trajectory) in pool.iter().enumerate() {
            // Extract category from task_id (assumes format "category-xxx" or use model as fallback)
            let category = self.extract_category(trajectory, categories);
            category_indices.entry(category).or_default().push(idx);
        }

        let mut selected: Vec<usize> = Vec::with_capacity(n);
        let mut rng = self.create_rng();

        // Calculate samples per category
        let num_categories = category_indices.len().max(1);
        let base_samples = n / num_categories;
        let extra_samples = n % num_categories;

        // Shuffle categories for fair distribution of extra samples
        let mut cat_keys: Vec<_> = category_indices.keys().cloned().collect();
        cat_keys.shuffle(&mut rng);

        for (i, category) in cat_keys.iter().enumerate() {
            if let Some(indices) = category_indices.get_mut(category) {
                let samples_for_cat = base_samples + if i < extra_samples { 1 } else { 0 };
                let samples_for_cat = samples_for_cat.min(indices.len());

                indices.shuffle(&mut rng);
                selected.extend(indices.iter().take(samples_for_cat));
            }
        }

        // If we still need more (some categories had fewer items), fill randomly
        if selected.len() < n {
            let remaining: Vec<usize> = (0..pool.len()).filter(|i| !selected.contains(i)).collect();
            let mut remaining_shuffled = remaining;
            remaining_shuffled.shuffle(&mut rng);
            let needed = n - selected.len();
            selected.extend(remaining_shuffled.into_iter().take(needed));
        }

        selected.truncate(n);
        selected
    }

    /// Cluster-based sampling using a simplified k-medoids approach.
    ///
    /// Groups trajectories into k clusters, then samples proportionally from each.
    fn cluster_sample(&self, pool: &[Trajectory], n: usize, k: usize) -> Vec<usize> {
        if pool.len() <= n {
            return (0..pool.len()).collect();
        }

        let k = k.min(pool.len()).max(1);

        // Compute embeddings and distance matrix
        let embeddings = self.embedding_generator.embed_batch(pool);
        let distances = pairwise_euclidean_distance(&embeddings);

        // Initialize cluster centers using maxmin for diversity
        let medoid_indices = self.select_initial_medoids(&distances, k);

        // Assign points to nearest medoid
        let assignments = self.assign_to_medoids(&distances, &medoid_indices);

        // Group by cluster
        let mut clusters: HashMap<usize, Vec<usize>> = HashMap::new();
        for (point_idx, &medoid_idx) in assignments.iter().enumerate() {
            clusters.entry(medoid_idx).or_default().push(point_idx);
        }

        // Sample proportionally from each cluster
        let mut selected: Vec<usize> = Vec::with_capacity(n);
        let mut rng = self.create_rng();

        let samples_per_cluster = n / k.max(1);
        let extra = n % k.max(1);

        let mut cluster_keys: Vec<_> = clusters.keys().copied().collect();
        cluster_keys.shuffle(&mut rng);

        for (i, cluster_key) in cluster_keys.iter().enumerate() {
            if let Some(points) = clusters.get_mut(cluster_key) {
                let samples = samples_per_cluster + if i < extra { 1 } else { 0 };
                let samples = samples.min(points.len());

                points.shuffle(&mut rng);
                selected.extend(points.iter().take(samples));
            }
        }

        // Fill remaining if needed
        if selected.len() < n {
            let remaining: Vec<usize> = (0..pool.len()).filter(|i| !selected.contains(i)).collect();
            let mut remaining_shuffled = remaining;
            remaining_shuffled.shuffle(&mut rng);
            let needed = n - selected.len();
            selected.extend(remaining_shuffled.into_iter().take(needed));
        }

        selected.truncate(n);
        selected
    }

    /// Selects k initial medoids using maxmin strategy.
    fn select_initial_medoids(&self, distances: &Array2<f64>, k: usize) -> Vec<usize> {
        let n = distances.nrows();
        let k = k.min(n);

        if k == 0 {
            return Vec::new();
        }

        let mut medoids: Vec<usize> = Vec::with_capacity(k);
        let mut min_distances: Vec<f64> = vec![f64::MAX; n];

        // Start with point 0 as first medoid
        let mut rng = self.create_rng();
        let first = (0..n)
            .collect::<Vec<_>>()
            .choose(&mut rng)
            .copied()
            .unwrap_or(0);
        medoids.push(first);

        // Update distances from first medoid
        for j in 0..n {
            if j != first {
                min_distances[j] = distances[[first, j]];
            }
        }

        // Select remaining medoids
        while medoids.len() < k {
            let mut best_idx = 0;
            let mut best_dist = f64::NEG_INFINITY;

            for (j, &min_dist) in min_distances.iter().enumerate() {
                if !medoids.contains(&j) && min_dist > best_dist {
                    best_dist = min_dist;
                    best_idx = j;
                }
            }

            medoids.push(best_idx);

            // Update min distances
            for j in 0..n {
                if !medoids.contains(&j) {
                    let dist_to_new = distances[[best_idx, j]];
                    min_distances[j] = min_distances[j].min(dist_to_new);
                }
            }
        }

        medoids
    }

    /// Assigns each point to its nearest medoid.
    fn assign_to_medoids(&self, distances: &Array2<f64>, medoids: &[usize]) -> Vec<usize> {
        let n = distances.nrows();
        let mut assignments = vec![0; n];

        for i in 0..n {
            let mut best_medoid = medoids[0];
            let mut best_dist = distances[[i, medoids[0]]];

            for &medoid in medoids.iter().skip(1) {
                let dist = distances[[i, medoid]];
                if dist < best_dist {
                    best_dist = dist;
                    best_medoid = medoid;
                }
            }

            assignments[i] = best_medoid;
        }

        assignments
    }

    /// Extracts category from a trajectory.
    fn extract_category(&self, trajectory: &Trajectory, categories: &[String]) -> String {
        // Check if task_id starts with any known category
        for category in categories {
            if trajectory
                .task_id
                .to_lowercase()
                .starts_with(&category.to_lowercase())
            {
                return category.clone();
            }
        }

        // Fallback: use model as category
        trajectory.model.clone()
    }

    /// Creates a random number generator.
    fn create_rng(&self) -> ChaCha8Rng {
        match self.seed {
            Some(seed) => ChaCha8Rng::seed_from_u64(seed),
            None => ChaCha8Rng::from_rng(&mut rand::rng()),
        }
    }
}

/// Builder for creating a DiverseSampler with custom settings.
#[derive(Debug, Clone, Default)]
pub struct DiverseSamplerBuilder {
    embedding_dimension: Option<usize>,
    seed: Option<u64>,
}

impl DiverseSamplerBuilder {
    /// Creates a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the embedding dimension.
    pub fn embedding_dimension(mut self, dimension: usize) -> Self {
        self.embedding_dimension = Some(dimension);
        self
    }

    /// Sets the random seed.
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Builds the DiverseSampler.
    pub fn build(self) -> DiverseSampler {
        let mut sampler = match self.embedding_dimension {
            Some(dim) => DiverseSampler::with_dimension(dim),
            None => DiverseSampler::new(),
        };

        if let Some(seed) = self.seed {
            sampler = sampler.with_seed(seed);
        }

        sampler
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trajectory::{
        AgentAction, EnvironmentState, Observation, TaskResult, TokenUsage, TrajectoryStep,
    };
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_trajectory(task_id: &str, model: &str, num_steps: usize) -> Trajectory {
        let steps: Vec<TrajectoryStep> = (0..num_steps)
            .map(|i| TrajectoryStep {
                step_number: i as u32,
                state: EnvironmentState::default(),
                action: AgentAction {
                    tool_name: format!("tool_{}", i % 3),
                    tool_args: serde_json::json!({"param": i}),
                    raw_llm_output: format!("Output {}", i),
                    thinking: None,
                },
                observation: Observation::default(),
                reward: 0.1,
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
            final_result: TaskResult::Success { score: 0.9 },
            total_reward: 0.9,
            created_at: Utc::now(),
            duration_seconds: 120,
            token_usage: TokenUsage::new(1000, 500),
        }
    }

    fn create_diverse_pool() -> Vec<Trajectory> {
        vec![
            create_test_trajectory("category-a-task-1", "gpt-4", 5),
            create_test_trajectory("category-a-task-2", "gpt-4", 3),
            create_test_trajectory("category-b-task-1", "claude-3", 7),
            create_test_trajectory("category-b-task-2", "claude-3", 4),
            create_test_trajectory("category-c-task-1", "gpt-4", 6),
            create_test_trajectory("category-c-task-2", "claude-3", 8),
        ]
    }

    #[test]
    fn test_sampler_new() {
        let sampler = DiverseSampler::new();
        assert!(sampler.seed.is_none());
    }

    #[test]
    fn test_sampler_with_seed() {
        let sampler = DiverseSampler::new().with_seed(42);
        assert_eq!(sampler.seed, Some(42));
    }

    #[test]
    fn test_sample_empty_pool() {
        let sampler = DiverseSampler::new().with_seed(42);
        let result = sampler.sample(&[], 5, SamplingStrategy::Random);
        assert!(result.is_empty());
    }

    #[test]
    fn test_sample_zero_n() {
        let sampler = DiverseSampler::new().with_seed(42);
        let pool = create_diverse_pool();
        let result = sampler.sample(&pool, 0, SamplingStrategy::Random);
        assert!(result.is_empty());
    }

    #[test]
    fn test_sample_n_greater_than_pool() {
        let sampler = DiverseSampler::new().with_seed(42);
        let pool = create_diverse_pool();
        let result = sampler.sample(&pool, 100, SamplingStrategy::Random);
        assert_eq!(result.len(), pool.len());
    }

    #[test]
    fn test_random_sample() {
        let sampler = DiverseSampler::new().with_seed(42);
        let pool = create_diverse_pool();

        let result = sampler.sample(&pool, 3, SamplingStrategy::Random);
        assert_eq!(result.len(), 3);

        // Verify all selected are from the pool
        for selected in &result {
            assert!(pool.iter().any(|p| p.id == selected.id));
        }
    }

    #[test]
    fn test_random_sample_reproducible() {
        let pool = create_diverse_pool();

        let sampler1 = DiverseSampler::new().with_seed(42);
        let result1 = sampler1.sample(&pool, 3, SamplingStrategy::Random);

        let sampler2 = DiverseSampler::new().with_seed(42);
        let result2 = sampler2.sample(&pool, 3, SamplingStrategy::Random);

        for (r1, r2) in result1.iter().zip(result2.iter()) {
            assert_eq!(
                r1.id, r2.id,
                "Results should be reproducible with same seed"
            );
        }
    }

    #[test]
    fn test_maxmin_sample() {
        let sampler = DiverseSampler::new().with_seed(42);
        let pool = create_diverse_pool();

        let result = sampler.sample(&pool, 3, SamplingStrategy::MaxMinDistance);
        assert_eq!(result.len(), 3);

        // Verify all selected are unique
        let ids: Vec<Uuid> = result.iter().map(|t| t.id).collect();
        let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique_ids.len(), "All selected should be unique");
    }

    #[test]
    fn test_stratified_sample() {
        let sampler = DiverseSampler::new().with_seed(42);
        let pool = create_diverse_pool();
        let categories = vec![
            "category-a".to_string(),
            "category-b".to_string(),
            "category-c".to_string(),
        ];

        let result = sampler.sample(
            &pool,
            3,
            SamplingStrategy::Stratified {
                categories: categories.clone(),
            },
        );
        assert_eq!(result.len(), 3);

        // With 3 samples from 3 categories, we should ideally get 1 from each
        // (though not guaranteed with uneven pool sizes)
    }

    #[test]
    fn test_cluster_sample() {
        let sampler = DiverseSampler::new().with_seed(42);
        let pool = create_diverse_pool();

        let result = sampler.sample(&pool, 4, SamplingStrategy::ClusterBased { k: 2 });
        assert_eq!(result.len(), 4);

        // Verify uniqueness
        let ids: Vec<Uuid> = result.iter().map(|t| t.id).collect();
        let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique_ids.len());
    }

    #[test]
    fn test_cluster_sample_k_equals_1() {
        let sampler = DiverseSampler::new().with_seed(42);
        let pool = create_diverse_pool();

        // k=1 means all in one cluster, should still work
        let result = sampler.sample(&pool, 3, SamplingStrategy::ClusterBased { k: 1 });
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_cluster_sample_k_greater_than_pool() {
        let sampler = DiverseSampler::new().with_seed(42);
        let pool = create_diverse_pool();

        // k > pool size should be handled gracefully
        let result = sampler.sample(&pool, 3, SamplingStrategy::ClusterBased { k: 100 });
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_builder() {
        let sampler = DiverseSamplerBuilder::new()
            .embedding_dimension(64)
            .seed(123)
            .build();

        assert_eq!(sampler.seed, Some(123));
    }

    #[test]
    fn test_builder_default() {
        let sampler = DiverseSamplerBuilder::default().build();
        assert!(sampler.seed.is_none());
    }

    #[test]
    fn test_sample_full_pool() {
        let sampler = DiverseSampler::new().with_seed(42);
        let pool = create_diverse_pool();
        let pool_len = pool.len();

        // Request exactly pool size
        let result = sampler.sample(&pool, pool_len, SamplingStrategy::MaxMinDistance);
        assert_eq!(result.len(), pool_len);
    }
}
