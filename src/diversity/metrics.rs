//! Diversity metrics for trajectory datasets.
//!
//! Provides metrics to measure and track the diversity of a trajectory
//! collection, including category distribution, model usage, action variety,
//! and embedding-based coverage.

use std::collections::HashMap;

use crate::trajectory::Trajectory;

use super::embeddings::{pairwise_euclidean_distance, EmbeddingGenerator};

/// Default embedding dimension for metrics calculations.
const DEFAULT_EMBEDDING_DIMENSION: usize = 128;

/// Diversity metrics for a set of trajectories.
///
/// Contains various measurements of dataset diversity including
/// distributions, variety scores, and coverage metrics.
#[derive(Debug, Clone)]
pub struct DiversityMetrics {
    /// Distribution of trajectories across task categories.
    pub category_distribution: HashMap<String, usize>,

    /// Distribution of trajectories across models.
    pub model_distribution: HashMap<String, usize>,

    /// Action variety score (0.0 to 1.0).
    /// Higher values indicate more diverse tool usage patterns.
    pub action_variety: f64,

    /// Average pairwise distance between trajectory embeddings.
    /// Higher values indicate more diverse trajectories.
    pub average_pairwise_distance: f64,

    /// Coverage score (0.0 to 1.0).
    /// Measures how well the trajectories cover the embedding space.
    pub coverage_score: f64,

    /// Total number of trajectories analyzed.
    pub trajectory_count: usize,

    /// Number of unique tools used across all trajectories.
    pub unique_tools_count: usize,

    /// Distribution of scaffold types.
    pub scaffold_distribution: HashMap<String, usize>,
}

impl DiversityMetrics {
    /// Calculates diversity metrics for a set of trajectories.
    ///
    /// # Arguments
    ///
    /// * `trajectories` - The trajectories to analyze.
    ///
    /// # Returns
    ///
    /// `DiversityMetrics` containing all calculated metrics.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use dataforge::diversity::DiversityMetrics;
    ///
    /// let metrics = DiversityMetrics::calculate(&trajectories);
    /// println!("Overall diversity: {:.2}", metrics.overall_score());
    /// ```
    pub fn calculate(trajectories: &[Trajectory]) -> Self {
        if trajectories.is_empty() {
            return Self::empty();
        }

        // Calculate distributions
        let category_distribution = Self::calculate_category_distribution(trajectories);
        let model_distribution = Self::calculate_model_distribution(trajectories);
        let scaffold_distribution = Self::calculate_scaffold_distribution(trajectories);

        // Calculate action variety
        let (action_variety, unique_tools_count) = Self::calculate_action_variety(trajectories);

        // Calculate embedding-based metrics
        let generator = EmbeddingGenerator::new(DEFAULT_EMBEDDING_DIMENSION);
        let embeddings = generator.embed_batch(trajectories);
        let distances = pairwise_euclidean_distance(&embeddings);

        let average_pairwise_distance = Self::calculate_average_distance(&distances);
        let coverage_score = Self::calculate_coverage_score(&distances);

        Self {
            category_distribution,
            model_distribution,
            action_variety,
            average_pairwise_distance,
            coverage_score,
            trajectory_count: trajectories.len(),
            unique_tools_count,
            scaffold_distribution,
        }
    }

    /// Returns empty metrics.
    fn empty() -> Self {
        Self {
            category_distribution: HashMap::new(),
            model_distribution: HashMap::new(),
            action_variety: 0.0,
            average_pairwise_distance: 0.0,
            coverage_score: 0.0,
            trajectory_count: 0,
            unique_tools_count: 0,
            scaffold_distribution: HashMap::new(),
        }
    }

    /// Calculates category distribution from task IDs.
    fn calculate_category_distribution(trajectories: &[Trajectory]) -> HashMap<String, usize> {
        let mut distribution: HashMap<String, usize> = HashMap::new();

        for trajectory in trajectories {
            // Extract category from task_id (assumes format like "category-xxx")
            let category = trajectory
                .task_id
                .split('-')
                .next()
                .unwrap_or(&trajectory.task_id)
                .to_string();

            *distribution.entry(category).or_insert(0) += 1;
        }

        distribution
    }

    /// Calculates model distribution.
    fn calculate_model_distribution(trajectories: &[Trajectory]) -> HashMap<String, usize> {
        let mut distribution: HashMap<String, usize> = HashMap::new();

        for trajectory in trajectories {
            *distribution.entry(trajectory.model.clone()).or_insert(0) += 1;
        }

        distribution
    }

    /// Calculates scaffold type distribution.
    fn calculate_scaffold_distribution(trajectories: &[Trajectory]) -> HashMap<String, usize> {
        let mut distribution: HashMap<String, usize> = HashMap::new();

        for trajectory in trajectories {
            *distribution
                .entry(trajectory.scaffold_type.clone())
                .or_insert(0) += 1;
        }

        distribution
    }

    /// Calculates action variety and counts unique tools.
    fn calculate_action_variety(trajectories: &[Trajectory]) -> (f64, usize) {
        let mut tool_usage: HashMap<String, usize> = HashMap::new();
        let mut total_actions = 0;

        for trajectory in trajectories {
            for step in &trajectory.steps {
                *tool_usage.entry(step.action.tool_name.clone()).or_insert(0) += 1;
                total_actions += 1;
            }
        }

        let unique_tools = tool_usage.len();

        if total_actions == 0 || unique_tools == 0 {
            return (0.0, 0);
        }

        // Calculate normalized entropy as variety measure
        let entropy = shannon_entropy(&tool_usage);
        let max_entropy = (unique_tools as f64).ln();

        let variety = if max_entropy > 0.0 {
            entropy / max_entropy
        } else {
            0.0
        };

        (variety.clamp(0.0, 1.0), unique_tools)
    }

    /// Calculates average pairwise distance from distance matrix.
    fn calculate_average_distance(distances: &ndarray::Array2<f64>) -> f64 {
        let n = distances.nrows();
        if n < 2 {
            return 0.0;
        }

        let mut sum = 0.0;
        let mut count = 0;

        for i in 0..n {
            for j in (i + 1)..n {
                sum += distances[[i, j]];
                count += 1;
            }
        }

        if count > 0 {
            sum / count as f64
        } else {
            0.0
        }
    }

    /// Calculates coverage score based on distance distribution.
    ///
    /// A higher coverage score indicates that trajectories are spread
    /// out across the embedding space rather than clustered together.
    fn calculate_coverage_score(distances: &ndarray::Array2<f64>) -> f64 {
        let n = distances.nrows();
        if n < 2 {
            return 0.0;
        }

        // Calculate min distance to any other point for each trajectory
        let mut min_distances: Vec<f64> = Vec::with_capacity(n);

        for i in 0..n {
            let mut min_dist = f64::MAX;
            for j in 0..n {
                if i != j && distances[[i, j]] < min_dist {
                    min_dist = distances[[i, j]];
                }
            }
            if min_dist < f64::MAX {
                min_distances.push(min_dist);
            }
        }

        if min_distances.is_empty() {
            return 0.0;
        }

        // Coverage score is the average of minimum distances normalized
        // Higher average min distance = better spread = higher coverage
        let avg_min_distance: f64 = min_distances.iter().sum::<f64>() / min_distances.len() as f64;

        // Normalize to [0, 1] using sigmoid-like function
        // This assumes typical embedding distances are in [0, 2] range for normalized embeddings
        let normalized = (avg_min_distance * 2.0).tanh();

        normalized.clamp(0.0, 1.0)
    }

    /// Calculates Shannon entropy for the category distribution.
    ///
    /// Higher entropy indicates more uniform distribution across categories.
    pub fn category_entropy(&self) -> f64 {
        shannon_entropy(&self.category_distribution)
    }

    /// Calculates Shannon entropy for the model distribution.
    ///
    /// Higher entropy indicates more uniform distribution across models.
    pub fn model_entropy(&self) -> f64 {
        shannon_entropy(&self.model_distribution)
    }

    /// Calculates Shannon entropy for the scaffold distribution.
    pub fn scaffold_entropy(&self) -> f64 {
        shannon_entropy(&self.scaffold_distribution)
    }

    /// Calculates the overall diversity score (0.0 to 1.0).
    ///
    /// Combines multiple diversity measures into a single score.
    /// Higher values indicate a more diverse dataset.
    pub fn overall_score(&self) -> f64 {
        if self.trajectory_count == 0 {
            return 0.0;
        }

        // Normalize entropies by their maximum possible values
        let max_category_entropy = if self.category_distribution.is_empty() {
            1.0
        } else {
            (self.category_distribution.len() as f64).ln().max(1.0)
        };

        let max_model_entropy = if self.model_distribution.is_empty() {
            1.0
        } else {
            (self.model_distribution.len() as f64).ln().max(1.0)
        };

        let normalized_category_entropy = self.category_entropy() / max_category_entropy;
        let normalized_model_entropy = self.model_entropy() / max_model_entropy;

        // Weighted combination of all diversity metrics
        let weights = [
            (normalized_category_entropy, 0.25), // Category diversity
            (normalized_model_entropy, 0.15),    // Model diversity
            (self.action_variety, 0.25),         // Action diversity
            (self.coverage_score, 0.35),         // Embedding space coverage
        ];

        let score: f64 = weights.iter().map(|(val, weight)| val * weight).sum();

        score.clamp(0.0, 1.0)
    }

    /// Returns a summary of the metrics as a formatted string.
    pub fn summary(&self) -> String {
        format!(
            "Diversity Metrics Summary:\n\
             - Trajectories: {}\n\
             - Categories: {} (entropy: {:.3})\n\
             - Models: {} (entropy: {:.3})\n\
             - Scaffolds: {}\n\
             - Unique Tools: {}\n\
             - Action Variety: {:.3}\n\
             - Avg Pairwise Distance: {:.3}\n\
             - Coverage Score: {:.3}\n\
             - Overall Score: {:.3}",
            self.trajectory_count,
            self.category_distribution.len(),
            self.category_entropy(),
            self.model_distribution.len(),
            self.model_entropy(),
            self.scaffold_distribution.len(),
            self.unique_tools_count,
            self.action_variety,
            self.average_pairwise_distance,
            self.coverage_score,
            self.overall_score()
        )
    }
}

/// Calculates Shannon entropy for a distribution.
///
/// Shannon entropy measures the uncertainty or "surprise" of a probability distribution.
/// Higher entropy means more uniform distribution (more diverse).
///
/// # Arguments
///
/// * `distribution` - Map from categories to counts.
///
/// # Returns
///
/// Shannon entropy in nats (natural logarithm base).
///
/// # Example
///
/// ```rust,ignore
/// use dataforge::diversity::shannon_entropy;
/// use std::collections::HashMap;
///
/// let mut dist = HashMap::new();
/// dist.insert("a".to_string(), 10);
/// dist.insert("b".to_string(), 10);
///
/// let entropy = shannon_entropy(&dist);
/// // For uniform distribution, entropy equals ln(n)
/// ```
pub fn shannon_entropy(distribution: &HashMap<String, usize>) -> f64 {
    if distribution.is_empty() {
        return 0.0;
    }

    let total: usize = distribution.values().sum();
    if total == 0 {
        return 0.0;
    }

    let total_f = total as f64;

    distribution
        .values()
        .filter(|&&count| count > 0)
        .map(|&count| {
            let p = count as f64 / total_f;
            -p * p.ln()
        })
        .sum()
}

/// Calculates normalized entropy (0.0 to 1.0).
///
/// Normalized by the maximum possible entropy for the given number of categories.
///
/// # Arguments
///
/// * `distribution` - Map from categories to counts.
///
/// # Returns
///
/// Normalized entropy in range [0, 1].
pub fn normalized_entropy(distribution: &HashMap<String, usize>) -> f64 {
    let entropy = shannon_entropy(distribution);
    let max_entropy = (distribution.len() as f64).ln();

    if max_entropy > 0.0 {
        (entropy / max_entropy).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Calculates Gini coefficient for distribution inequality.
///
/// A Gini coefficient of 0 indicates perfect equality (all categories have same count),
/// while 1 indicates maximum inequality (all items in one category).
///
/// # Arguments
///
/// * `distribution` - Map from categories to counts.
///
/// # Returns
///
/// Gini coefficient in range [0, 1].
pub fn gini_coefficient(distribution: &HashMap<String, usize>) -> f64 {
    if distribution.is_empty() {
        return 0.0;
    }

    let mut values: Vec<f64> = distribution.values().map(|&v| v as f64).collect();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n = values.len() as f64;
    let total: f64 = values.iter().sum();

    if total == 0.0 {
        return 0.0;
    }

    let mut gini_sum = 0.0;

    for (i, &value) in values.iter().enumerate() {
        gini_sum += (2.0 * (i as f64 + 1.0) - n - 1.0) * value;
    }

    gini_sum / (n * total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trajectory::{
        AgentAction, EnvironmentState, Observation, TaskResult, TokenUsage, TrajectoryStep,
    };
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_trajectory(
        task_id: &str,
        model: &str,
        scaffold: &str,
        tools: &[&str],
    ) -> Trajectory {
        let steps: Vec<TrajectoryStep> = tools
            .iter()
            .enumerate()
            .map(|(i, tool)| TrajectoryStep {
                step_number: i as u32,
                state: EnvironmentState::default(),
                action: AgentAction {
                    tool_name: tool.to_string(),
                    tool_args: serde_json::json!({"param": i}),
                    raw_llm_output: format!("Output {}", i),
                    thinking: None,
                },
                observation: Observation::default(),
                reward: 0.1,
                done: i == tools.len() - 1,
                timestamp: Utc::now(),
            })
            .collect();

        Trajectory {
            id: Uuid::new_v4(),
            task_id: task_id.to_string(),
            model: model.to_string(),
            scaffold_type: scaffold.to_string(),
            steps,
            final_result: TaskResult::Success { score: 0.9 },
            total_reward: 0.9,
            created_at: Utc::now(),
            duration_seconds: 120,
            token_usage: TokenUsage::new(1000, 500),
        }
    }

    fn create_diverse_dataset() -> Vec<Trajectory> {
        vec![
            create_test_trajectory(
                "bugfix-1",
                "gpt-4",
                "react",
                &["read_file", "edit_file", "run_test"],
            ),
            create_test_trajectory(
                "bugfix-2",
                "claude-3",
                "reflexion",
                &["read_file", "edit_file"],
            ),
            create_test_trajectory(
                "feature-1",
                "gpt-4",
                "react",
                &["search", "read_file", "create_file", "edit_file"],
            ),
            create_test_trajectory(
                "feature-2",
                "claude-3",
                "basic",
                &["read_file", "create_file"],
            ),
            create_test_trajectory(
                "refactor-1",
                "gpt-4",
                "reflexion",
                &["search", "read_file", "edit_file", "run_test"],
            ),
        ]
    }

    #[test]
    fn test_calculate_empty() {
        let metrics = DiversityMetrics::calculate(&[]);
        assert_eq!(metrics.trajectory_count, 0);
        assert_eq!(metrics.overall_score(), 0.0);
    }

    #[test]
    fn test_calculate_single() {
        let trajectories = vec![create_test_trajectory(
            "bugfix-1",
            "gpt-4",
            "react",
            &["read_file"],
        )];

        let metrics = DiversityMetrics::calculate(&trajectories);
        assert_eq!(metrics.trajectory_count, 1);
        assert_eq!(metrics.category_distribution.len(), 1);
        assert_eq!(metrics.model_distribution.len(), 1);
    }

    #[test]
    fn test_calculate_diverse() {
        let trajectories = create_diverse_dataset();
        let metrics = DiversityMetrics::calculate(&trajectories);

        assert_eq!(metrics.trajectory_count, 5);
        assert_eq!(metrics.category_distribution.len(), 3); // bugfix, feature, refactor
        assert_eq!(metrics.model_distribution.len(), 2); // gpt-4, claude-3
        assert_eq!(metrics.scaffold_distribution.len(), 3); // react, reflexion, basic

        // Check overall score is reasonable (between 0 and 1)
        let score = metrics.overall_score();
        assert!((0.0..=1.0).contains(&score));
    }

    #[test]
    fn test_category_distribution() {
        let trajectories = create_diverse_dataset();
        let metrics = DiversityMetrics::calculate(&trajectories);

        assert_eq!(metrics.category_distribution.get("bugfix"), Some(&2));
        assert_eq!(metrics.category_distribution.get("feature"), Some(&2));
        assert_eq!(metrics.category_distribution.get("refactor"), Some(&1));
    }

    #[test]
    fn test_model_distribution() {
        let trajectories = create_diverse_dataset();
        let metrics = DiversityMetrics::calculate(&trajectories);

        assert_eq!(metrics.model_distribution.get("gpt-4"), Some(&3));
        assert_eq!(metrics.model_distribution.get("claude-3"), Some(&2));
    }

    #[test]
    fn test_action_variety() {
        let trajectories = create_diverse_dataset();
        let metrics = DiversityMetrics::calculate(&trajectories);

        // Should have multiple unique tools
        assert!(metrics.unique_tools_count > 1);
        // Action variety should be positive
        assert!(metrics.action_variety > 0.0);
        assert!(metrics.action_variety <= 1.0);
    }

    #[test]
    fn test_shannon_entropy_empty() {
        let distribution: HashMap<String, usize> = HashMap::new();
        assert_eq!(shannon_entropy(&distribution), 0.0);
    }

    #[test]
    fn test_shannon_entropy_single() {
        let mut distribution = HashMap::new();
        distribution.insert("a".to_string(), 10);
        assert_eq!(shannon_entropy(&distribution), 0.0); // No uncertainty
    }

    #[test]
    fn test_shannon_entropy_uniform() {
        let mut distribution = HashMap::new();
        distribution.insert("a".to_string(), 10);
        distribution.insert("b".to_string(), 10);

        let entropy = shannon_entropy(&distribution);
        let expected = 2.0_f64.ln(); // ln(2) for uniform binary distribution

        assert!(
            (entropy - expected).abs() < 1e-10,
            "Expected {}, got {}",
            expected,
            entropy
        );
    }

    #[test]
    fn test_shannon_entropy_skewed() {
        let mut uniform = HashMap::new();
        uniform.insert("a".to_string(), 50);
        uniform.insert("b".to_string(), 50);

        let mut skewed = HashMap::new();
        skewed.insert("a".to_string(), 90);
        skewed.insert("b".to_string(), 10);

        let uniform_entropy = shannon_entropy(&uniform);
        let skewed_entropy = shannon_entropy(&skewed);

        assert!(
            uniform_entropy > skewed_entropy,
            "Uniform distribution should have higher entropy"
        );
    }

    #[test]
    fn test_normalized_entropy() {
        let mut distribution = HashMap::new();
        distribution.insert("a".to_string(), 10);
        distribution.insert("b".to_string(), 10);

        let normalized = normalized_entropy(&distribution);
        assert!(
            (normalized - 1.0).abs() < 1e-10,
            "Uniform distribution should have normalized entropy of 1.0"
        );
    }

    #[test]
    fn test_gini_coefficient_equal() {
        let mut distribution = HashMap::new();
        distribution.insert("a".to_string(), 10);
        distribution.insert("b".to_string(), 10);
        distribution.insert("c".to_string(), 10);

        let gini = gini_coefficient(&distribution);
        assert!(
            gini.abs() < 1e-10,
            "Equal distribution should have Gini ≈ 0"
        );
    }

    #[test]
    fn test_gini_coefficient_unequal() {
        // With 4 categories, highly unequal: one has 97, others have 1 each
        let mut distribution = HashMap::new();
        distribution.insert("a".to_string(), 97);
        distribution.insert("b".to_string(), 1);
        distribution.insert("c".to_string(), 1);
        distribution.insert("d".to_string(), 1);

        let gini = gini_coefficient(&distribution);
        // Gini should be high for this highly unequal distribution
        assert!(
            gini > 0.4,
            "Highly unequal distribution should have Gini > 0.4, got {}",
            gini
        );
    }

    #[test]
    fn test_gini_coefficient_empty() {
        let distribution: HashMap<String, usize> = HashMap::new();
        assert_eq!(gini_coefficient(&distribution), 0.0);
    }

    #[test]
    fn test_overall_score_bounds() {
        let trajectories = create_diverse_dataset();
        let metrics = DiversityMetrics::calculate(&trajectories);
        let score = metrics.overall_score();

        assert!(score >= 0.0, "Score should be >= 0");
        assert!(score <= 1.0, "Score should be <= 1");
    }

    #[test]
    fn test_summary() {
        let trajectories = create_diverse_dataset();
        let metrics = DiversityMetrics::calculate(&trajectories);
        let summary = metrics.summary();

        assert!(summary.contains("Trajectories: 5"));
        assert!(summary.contains("Categories:"));
        assert!(summary.contains("Models:"));
        assert!(summary.contains("Overall Score:"));
    }

    #[test]
    fn test_low_diversity_dataset() {
        // Create homogeneous dataset
        let trajectories = vec![
            create_test_trajectory("bugfix-1", "gpt-4", "react", &["read_file"]),
            create_test_trajectory("bugfix-2", "gpt-4", "react", &["read_file"]),
            create_test_trajectory("bugfix-3", "gpt-4", "react", &["read_file"]),
        ];

        let metrics = DiversityMetrics::calculate(&trajectories);

        // All same category, model, scaffold, tool
        assert_eq!(metrics.category_distribution.len(), 1);
        assert_eq!(metrics.model_distribution.len(), 1);
        assert_eq!(metrics.unique_tools_count, 1);

        // Entropy should be 0 for single-category distributions
        assert_eq!(metrics.category_entropy(), 0.0);
        assert_eq!(metrics.model_entropy(), 0.0);
    }
}
