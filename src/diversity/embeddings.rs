//! Embedding generation for trajectories.
//!
//! Provides functionality to convert trajectories, actions, and text
//! into vector representations for similarity comparison.
//!
//! This module uses hash-based embeddings as a simplified approach.
//! In production, you would integrate an actual embedding model.

use ndarray::{Array1, Array2};
use sha2::{Digest, Sha256};

use crate::trajectory::{AgentAction, Trajectory};

/// Default embedding dimension for trajectory vectors.
const DEFAULT_DIMENSION: usize = 128;

/// Generator for trajectory embeddings.
///
/// Uses hash-based feature extraction to create fixed-dimensional
/// vector representations of trajectories, actions, and text.
#[derive(Debug, Clone)]
pub struct EmbeddingGenerator {
    /// Dimension of the generated embeddings.
    dimension: usize,
}

impl Default for EmbeddingGenerator {
    fn default() -> Self {
        Self::new(DEFAULT_DIMENSION)
    }
}

impl EmbeddingGenerator {
    /// Creates a new embedding generator with the specified dimension.
    ///
    /// # Arguments
    ///
    /// * `dimension` - The dimension of the embedding vectors to generate.
    ///
    /// # Example
    ///
    /// ```
    /// use dataforge::diversity::EmbeddingGenerator;
    ///
    /// let generator = EmbeddingGenerator::new(64);
    /// ```
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }

    /// Returns the embedding dimension.
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Generates an embedding for a trajectory.
    ///
    /// The embedding captures:
    /// - Task and model information
    /// - Action sequence features
    /// - Execution metadata (duration, reward)
    ///
    /// # Arguments
    ///
    /// * `trajectory` - The trajectory to embed.
    ///
    /// # Returns
    ///
    /// A fixed-dimensional vector representation of the trajectory.
    pub fn embed_trajectory(&self, trajectory: &Trajectory) -> Array1<f64> {
        let mut embedding = Array1::zeros(self.dimension);

        // Feature 1: Task ID hash (captures task identity)
        let task_hash = self.hash_to_floats(&trajectory.task_id, self.dimension / 4);
        for (i, val) in task_hash.iter().enumerate() {
            embedding[i] = *val;
        }

        // Feature 2: Model hash (captures model-specific patterns)
        let model_hash = self.hash_to_floats(&trajectory.model, self.dimension / 8);
        let offset = self.dimension / 4;
        for (i, val) in model_hash.iter().enumerate() {
            embedding[offset + i] = *val;
        }

        // Feature 3: Scaffold type hash
        let scaffold_hash = self.hash_to_floats(&trajectory.scaffold_type, self.dimension / 8);
        let offset = offset + self.dimension / 8;
        for (i, val) in scaffold_hash.iter().enumerate() {
            embedding[offset + i] = *val;
        }

        // Feature 4: Action sequence embedding
        let actions: Vec<AgentAction> = trajectory.steps.iter().map(|s| s.action.clone()).collect();
        let action_embedding = self.embed_actions(&actions);
        let offset = offset + self.dimension / 8;
        let action_dim = self.dimension / 4;
        for i in 0..action_dim.min(action_embedding.len()) {
            embedding[offset + i] = action_embedding[i];
        }

        // Feature 5: Execution statistics (normalized)
        let stats_offset = offset + action_dim;
        let remaining = self.dimension - stats_offset;
        if remaining > 0 {
            // Normalized step count (assume max 100 steps)
            embedding[stats_offset] = (trajectory.steps.len() as f64 / 100.0).min(1.0);
        }
        if remaining > 1 {
            // Normalized reward (assume -1 to 1 range)
            embedding[stats_offset + 1] = (trajectory.total_reward + 1.0) / 2.0;
        }
        if remaining > 2 {
            // Normalized duration (assume max 3600 seconds)
            embedding[stats_offset + 2] = (trajectory.duration_seconds as f64 / 3600.0).min(1.0);
        }
        if remaining > 3 {
            // Success indicator from final result
            let success_score = match &trajectory.final_result {
                crate::trajectory::TaskResult::Success { score } => *score,
                crate::trajectory::TaskResult::Failure { .. } => 0.0,
                crate::trajectory::TaskResult::Timeout => 0.25,
                crate::trajectory::TaskResult::Error { .. } => 0.1,
            };
            embedding[stats_offset + 3] = success_score;
        }

        // Normalize the embedding to unit length
        self.normalize(&mut embedding);
        embedding
    }

    /// Generates an embedding for an action sequence.
    ///
    /// Captures:
    /// - Tool usage patterns
    /// - Sequence structure
    /// - Action complexity
    ///
    /// # Arguments
    ///
    /// * `actions` - The sequence of actions to embed.
    ///
    /// # Returns
    ///
    /// A fixed-dimensional vector representation of the action sequence.
    pub fn embed_actions(&self, actions: &[AgentAction]) -> Array1<f64> {
        let mut embedding = Array1::zeros(self.dimension);

        if actions.is_empty() {
            return embedding;
        }

        // Feature 1: Tool frequency distribution
        let tool_counts = self.count_tools(actions);
        let tool_dim = self.dimension / 3;
        for (i, (tool, count)) in tool_counts.iter().enumerate() {
            if i >= tool_dim {
                break;
            }
            // Hash tool name to position within tool_dim
            let pos = self.hash_to_index(tool, tool_dim);
            embedding[pos] += *count as f64 / actions.len() as f64;
        }

        // Feature 2: Tool transition patterns (bigrams)
        let offset = tool_dim;
        let bigram_dim = self.dimension / 3;
        for window in actions.windows(2) {
            let bigram = format!("{}→{}", window[0].tool_name, window[1].tool_name);
            let pos = offset + self.hash_to_index(&bigram, bigram_dim);
            embedding[pos] += 1.0 / (actions.len() - 1).max(1) as f64;
        }

        // Feature 3: Action content features
        let offset = offset + bigram_dim;
        let content_dim = self.dimension - offset;
        for (i, action) in actions.iter().enumerate() {
            // Positional encoding
            let position_weight = 1.0 / (i + 1) as f64;

            // Hash arguments to feature position
            let arg_str = action.tool_args.to_string();
            let arg_hash = self.hash_to_floats(&arg_str, content_dim.min(16));
            for (j, val) in arg_hash.iter().enumerate() {
                if offset + j < self.dimension {
                    embedding[offset + j] += val * position_weight;
                }
            }
        }

        self.normalize(&mut embedding);
        embedding
    }

    /// Generates an embedding for text content.
    ///
    /// Uses character n-gram hashing for text representation.
    ///
    /// # Arguments
    ///
    /// * `text` - The text to embed.
    ///
    /// # Returns
    ///
    /// A fixed-dimensional vector representation of the text.
    pub fn embed_text(&self, text: &str) -> Array1<f64> {
        let mut embedding = Array1::zeros(self.dimension);

        if text.is_empty() {
            return embedding;
        }

        let text_lower = text.to_lowercase();

        // Feature 1: Word-level features
        let words: Vec<&str> = text_lower.split_whitespace().collect();
        let word_dim = self.dimension / 2;
        for word in &words {
            let pos = self.hash_to_index(word, word_dim);
            embedding[pos] += 1.0 / words.len() as f64;
        }

        // Feature 2: Character trigram features
        let offset = word_dim;
        let trigram_dim = self.dimension / 4;
        let chars: Vec<char> = text_lower.chars().collect();
        for window in chars.windows(3) {
            let trigram: String = window.iter().collect();
            let pos = offset + self.hash_to_index(&trigram, trigram_dim);
            embedding[pos] += 1.0;
        }

        // Feature 3: Text statistics
        let stats_offset = offset + trigram_dim;
        let remaining = self.dimension - stats_offset;
        if remaining > 0 {
            // Normalized length
            embedding[stats_offset] = (text.len() as f64 / 1000.0).min(1.0);
        }
        if remaining > 1 {
            // Word count normalized
            embedding[stats_offset + 1] = (words.len() as f64 / 200.0).min(1.0);
        }
        if remaining > 2 {
            // Average word length
            let avg_word_len = if words.is_empty() {
                0.0
            } else {
                words.iter().map(|w| w.len()).sum::<usize>() as f64 / words.len() as f64
            };
            embedding[stats_offset + 2] = avg_word_len / 10.0;
        }

        self.normalize(&mut embedding);
        embedding
    }

    /// Batch embeds multiple trajectories.
    ///
    /// # Arguments
    ///
    /// * `trajectories` - The trajectories to embed.
    ///
    /// # Returns
    ///
    /// A 2D array where each row is a trajectory embedding.
    pub fn embed_batch(&self, trajectories: &[Trajectory]) -> Array2<f64> {
        let n = trajectories.len();
        let mut result = Array2::zeros((n, self.dimension));

        for (i, trajectory) in trajectories.iter().enumerate() {
            let embedding = self.embed_trajectory(trajectory);
            result.row_mut(i).assign(&embedding);
        }

        result
    }

    /// Hashes a string to floating-point values in [0, 1].
    fn hash_to_floats(&self, input: &str, count: usize) -> Vec<f64> {
        let mut result = Vec::with_capacity(count);
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        let hash_bytes = hasher.finalize();

        for i in 0..count {
            // Use pairs of bytes to generate floats
            let idx = (i * 2) % 32;
            let val = ((hash_bytes[idx] as u16) << 8 | hash_bytes[(idx + 1) % 32] as u16) as f64
                / 65535.0;
            result.push(val);
        }

        result
    }

    /// Hashes a string to an index in [0, max_index).
    fn hash_to_index(&self, input: &str, max_index: usize) -> usize {
        if max_index == 0 {
            return 0;
        }
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        let hash_bytes = hasher.finalize();
        let hash_val = ((hash_bytes[0] as u32) << 24
            | (hash_bytes[1] as u32) << 16
            | (hash_bytes[2] as u32) << 8
            | hash_bytes[3] as u32) as usize;
        hash_val % max_index
    }

    /// Counts occurrences of each tool in an action sequence.
    fn count_tools(&self, actions: &[AgentAction]) -> Vec<(String, usize)> {
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for action in actions {
            *counts.entry(action.tool_name.clone()).or_insert(0) += 1;
        }
        let mut sorted: Vec<_> = counts.into_iter().collect();
        sorted.sort_by_key(|item| std::cmp::Reverse(item.1));
        sorted
    }

    /// Normalizes a vector to unit length (L2 norm).
    fn normalize(&self, v: &mut Array1<f64>) {
        let norm: f64 = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm > 1e-10 {
            v.mapv_inplace(|x| x / norm);
        }
    }
}

/// Computes cosine similarity between two vectors.
///
/// Cosine similarity measures the angle between vectors,
/// ranging from -1 (opposite) to 1 (identical direction).
///
/// # Arguments
///
/// * `a` - First vector.
/// * `b` - Second vector.
///
/// # Returns
///
/// Cosine similarity value in [-1, 1].
///
/// # Panics
///
/// Panics if vectors have different lengths.
pub fn cosine_similarity(a: &Array1<f64>, b: &Array1<f64>) -> f64 {
    assert_eq!(
        a.len(),
        b.len(),
        "Vectors must have the same length for cosine similarity"
    );

    let dot_product: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

    if norm_a < 1e-10 || norm_b < 1e-10 {
        return 0.0;
    }

    dot_product / (norm_a * norm_b)
}

/// Computes Euclidean distance between two vectors.
///
/// # Arguments
///
/// * `a` - First vector.
/// * `b` - Second vector.
///
/// # Returns
///
/// Euclidean (L2) distance.
///
/// # Panics
///
/// Panics if vectors have different lengths.
pub fn euclidean_distance(a: &Array1<f64>, b: &Array1<f64>) -> f64 {
    assert_eq!(
        a.len(),
        b.len(),
        "Vectors must have the same length for Euclidean distance"
    );

    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// Computes pairwise cosine similarity matrix for a batch of embeddings.
///
/// # Arguments
///
/// * `embeddings` - 2D array where each row is an embedding.
///
/// # Returns
///
/// Symmetric matrix of pairwise similarities.
pub fn pairwise_cosine_similarity(embeddings: &Array2<f64>) -> Array2<f64> {
    let n = embeddings.nrows();
    let mut similarity_matrix = Array2::zeros((n, n));

    for i in 0..n {
        let row_i = embeddings.row(i).to_owned();
        similarity_matrix[[i, i]] = 1.0;

        for j in (i + 1)..n {
            let row_j = embeddings.row(j).to_owned();
            let sim = cosine_similarity(&row_i, &row_j);
            similarity_matrix[[i, j]] = sim;
            similarity_matrix[[j, i]] = sim;
        }
    }

    similarity_matrix
}

/// Computes pairwise Euclidean distance matrix for a batch of embeddings.
///
/// # Arguments
///
/// * `embeddings` - 2D array where each row is an embedding.
///
/// # Returns
///
/// Symmetric matrix of pairwise distances.
pub fn pairwise_euclidean_distance(embeddings: &Array2<f64>) -> Array2<f64> {
    let n = embeddings.nrows();
    let mut distance_matrix = Array2::zeros((n, n));

    for i in 0..n {
        let row_i = embeddings.row(i).to_owned();

        for j in (i + 1)..n {
            let row_j = embeddings.row(j).to_owned();
            let dist = euclidean_distance(&row_i, &row_j);
            distance_matrix[[i, j]] = dist;
            distance_matrix[[j, i]] = dist;
        }
    }

    distance_matrix
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trajectory::{
        AgentAction, EnvironmentState, Observation, TaskResult, TokenUsage, Trajectory,
        TrajectoryStep,
    };
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_trajectory(task_id: &str, model: &str, num_steps: usize) -> Trajectory {
        let steps: Vec<TrajectoryStep> = (0..num_steps)
            .map(|i| TrajectoryStep {
                step_number: i as u32,
                state: EnvironmentState::default(),
                action: AgentAction {
                    tool_name: if i % 2 == 0 {
                        "read_file".to_string()
                    } else {
                        "write_file".to_string()
                    },
                    tool_args: serde_json::json!({"path": format!("file{}.txt", i)}),
                    raw_llm_output: format!("Step {} output", i),
                    thinking: Some(format!("Thinking about step {}", i)),
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

    #[test]
    fn test_embedding_generator_new() {
        let generator = EmbeddingGenerator::new(64);
        assert_eq!(generator.dimension(), 64);
    }

    #[test]
    fn test_embedding_generator_default() {
        let generator = EmbeddingGenerator::default();
        assert_eq!(generator.dimension(), DEFAULT_DIMENSION);
    }

    #[test]
    fn test_embed_trajectory() {
        let generator = EmbeddingGenerator::new(128);
        let trajectory = create_test_trajectory("task-1", "gpt-4", 5);
        let embedding = generator.embed_trajectory(&trajectory);

        assert_eq!(embedding.len(), 128);
        // Check embedding is normalized (L2 norm ≈ 1.0)
        let norm: f64 = embedding.iter().map(|x| x * x).sum::<f64>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-6,
            "Embedding should be unit normalized"
        );
    }

    #[test]
    fn test_embed_trajectory_deterministic() {
        let generator = EmbeddingGenerator::new(128);
        let trajectory = create_test_trajectory("task-1", "gpt-4", 5);

        let embedding1 = generator.embed_trajectory(&trajectory);
        let embedding2 = generator.embed_trajectory(&trajectory);

        for i in 0..embedding1.len() {
            assert!(
                (embedding1[i] - embedding2[i]).abs() < 1e-10,
                "Embeddings should be deterministic"
            );
        }
    }

    #[test]
    fn test_different_trajectories_different_embeddings() {
        let generator = EmbeddingGenerator::new(128);
        let trajectory1 = create_test_trajectory("task-1", "gpt-4", 5);
        let trajectory2 = create_test_trajectory("task-2", "claude-3", 10);

        let embedding1 = generator.embed_trajectory(&trajectory1);
        let embedding2 = generator.embed_trajectory(&trajectory2);

        let similarity = cosine_similarity(&embedding1, &embedding2);
        assert!(
            similarity < 0.99,
            "Different trajectories should have different embeddings"
        );
    }

    #[test]
    fn test_embed_actions() {
        let generator = EmbeddingGenerator::new(64);
        let actions = vec![
            AgentAction {
                tool_name: "read_file".to_string(),
                tool_args: serde_json::json!({"path": "test.txt"}),
                raw_llm_output: "Reading file".to_string(),
                thinking: None,
            },
            AgentAction {
                tool_name: "write_file".to_string(),
                tool_args: serde_json::json!({"path": "test.txt", "content": "hello"}),
                raw_llm_output: "Writing file".to_string(),
                thinking: None,
            },
        ];

        let embedding = generator.embed_actions(&actions);
        assert_eq!(embedding.len(), 64);
    }

    #[test]
    fn test_embed_actions_empty() {
        let generator = EmbeddingGenerator::new(64);
        let embedding = generator.embed_actions(&[]);
        assert_eq!(embedding.len(), 64);
        assert!(embedding.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_embed_text() {
        let generator = EmbeddingGenerator::new(64);
        let embedding = generator.embed_text("Fix the bug in the login function");

        assert_eq!(embedding.len(), 64);
        let norm: f64 = embedding.iter().map(|x| x * x).sum::<f64>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_embed_text_empty() {
        let generator = EmbeddingGenerator::new(64);
        let embedding = generator.embed_text("");
        assert_eq!(embedding.len(), 64);
        assert!(embedding.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_embed_batch() {
        let generator = EmbeddingGenerator::new(128);
        let trajectories = vec![
            create_test_trajectory("task-1", "gpt-4", 5),
            create_test_trajectory("task-2", "claude-3", 3),
            create_test_trajectory("task-3", "gpt-4", 7),
        ];

        let embeddings = generator.embed_batch(&trajectories);
        assert_eq!(embeddings.nrows(), 3);
        assert_eq!(embeddings.ncols(), 128);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        let b = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = Array1::from_vec(vec![1.0, 0.0, 0.0]);
        let b = Array1::from_vec(vec![0.0, 1.0, 0.0]);
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        let b = Array1::from_vec(vec![-1.0, -2.0, -3.0]);
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        let b = Array1::from_vec(vec![0.0, 0.0, 0.0]);
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_euclidean_distance_same() {
        let a = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        let b = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        let dist = euclidean_distance(&a, &b);
        assert!(dist < 1e-10);
    }

    #[test]
    fn test_euclidean_distance_unit_apart() {
        let a = Array1::from_vec(vec![0.0, 0.0, 0.0]);
        let b = Array1::from_vec(vec![1.0, 0.0, 0.0]);
        let dist = euclidean_distance(&a, &b);
        assert!((dist - 1.0).abs() < 1e-10);
    }

    #[test]
    #[should_panic(expected = "Vectors must have the same length")]
    fn test_cosine_similarity_different_lengths() {
        let a = Array1::from_vec(vec![1.0, 2.0]);
        let b = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        cosine_similarity(&a, &b);
    }

    #[test]
    fn test_pairwise_cosine_similarity() {
        let embeddings = Array2::from_shape_vec(
            (3, 4),
            vec![
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.707, 0.707, 0.0, 0.0,
            ],
        )
        .expect("Failed to create array");

        let sim_matrix = pairwise_cosine_similarity(&embeddings);
        assert_eq!(sim_matrix.shape(), &[3, 3]);

        // Diagonal should be 1.0
        for i in 0..3 {
            assert!((sim_matrix[[i, i]] - 1.0).abs() < 1e-10);
        }

        // Matrix should be symmetric
        for i in 0..3 {
            for j in 0..3 {
                assert!((sim_matrix[[i, j]] - sim_matrix[[j, i]]).abs() < 1e-10);
            }
        }
    }

    #[test]
    fn test_pairwise_euclidean_distance() {
        let embeddings = Array2::from_shape_vec((2, 3), vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0])
            .expect("Failed to create array");

        let dist_matrix = pairwise_euclidean_distance(&embeddings);
        assert_eq!(dist_matrix.shape(), &[2, 2]);

        // Diagonal should be 0.0
        for i in 0..2 {
            assert!(dist_matrix[[i, i]] < 1e-10);
        }

        // Distance should be 1.0
        assert!((dist_matrix[[0, 1]] - 1.0).abs() < 1e-10);
    }
}
