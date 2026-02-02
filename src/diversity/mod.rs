//! Diversity analysis system for trajectory deduplication and sampling.
//!
//! This module provides tools for analyzing and improving the diversity
//! of trajectory datasets through embedding-based similarity analysis,
//! near-duplicate detection, diverse sampling strategies, and diversity metrics.
//!
//! # Overview
//!
//! When training models on trajectory data, diversity is crucial:
//! - **Redundant trajectories** waste compute and may cause overfitting
//! - **Diverse trajectories** teach more generalizable patterns
//! - **Balanced distributions** ensure coverage of different task types
//!
//! This module provides four main components:
//!
//! 1. **Embeddings** - Convert trajectories to vectors for similarity comparison
//! 2. **Deduplication** - Remove near-duplicates based on similarity threshold
//! 3. **Sampling** - Select diverse subsets using various strategies
//! 4. **Metrics** - Track and measure dataset diversity
//!
//! # Usage
//!
//! ## Deduplicating Trajectories
//!
//! ```rust,ignore
//! use synth_bench::diversity::Deduplicator;
//!
//! // Create a deduplicator with 85% similarity threshold
//! let deduplicator = Deduplicator::new(0.85);
//!
//! // Find and remove near-duplicates
//! let result = deduplicator.deduplicate(trajectories);
//! println!("Kept {} of {} trajectories", result.total_after, result.total_before);
//! ```
//!
//! ## Diverse Sampling
//!
//! ```rust,ignore
//! use synth_bench::diversity::{DiverseSampler, SamplingStrategy};
//!
//! let sampler = DiverseSampler::new().with_seed(42);
//!
//! // Sample 100 diverse trajectories using max-min distance
//! let diverse_subset = sampler.sample(
//!     &trajectories,
//!     100,
//!     SamplingStrategy::MaxMinDistance,
//! );
//! ```
//!
//! ## Measuring Diversity
//!
//! ```rust,ignore
//! use synth_bench::diversity::DiversityMetrics;
//!
//! let metrics = DiversityMetrics::calculate(&trajectories);
//! println!("Category entropy: {:.3}", metrics.category_entropy());
//! println!("Overall diversity: {:.3}", metrics.overall_score());
//! ```
//!
//! # Embedding Approach
//!
//! This module uses hash-based embeddings as a simplified approach suitable
//! for comparing trajectories without requiring an external ML model:
//!
//! - Task ID, model, and scaffold type are hashed to capture identity
//! - Tool usage patterns are encoded as bag-of-words features
//! - Action sequences are captured via n-gram hashing
//! - Execution statistics (steps, reward, duration) provide numeric features
//!
//! For production use with higher accuracy, consider integrating an actual
//! embedding model (e.g., sentence transformers).

pub mod dedup;
pub mod embeddings;
pub mod metrics;
pub mod sampling;

// Re-export main types for convenience
pub use dedup::{DeduplicationResult, Deduplicator, DeduplicatorBuilder};
pub use embeddings::{
    cosine_similarity, euclidean_distance, pairwise_cosine_similarity, pairwise_euclidean_distance,
    EmbeddingGenerator,
};
pub use metrics::{gini_coefficient, normalized_entropy, shannon_entropy, DiversityMetrics};
pub use sampling::{DiverseSampler, DiverseSamplerBuilder, SamplingStrategy};
