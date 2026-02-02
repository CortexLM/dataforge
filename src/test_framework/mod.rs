// Test framework module
// Provides pytest generation, reward calculation, and verification utilities

pub mod pytest_generator;
pub mod reward;
pub mod verification;

pub use pytest_generator::PytestGenerator;
pub use reward::{
    CategoryScore, CreditLevel, PartialCredit, Reward, RewardBreakdown, RewardMetadata, TaskConfig,
    TestResults,
};
pub use verification::{OutputVerifier, VerificationResult};

/// Re-export common types for test framework usage
pub struct TestFramework;

impl TestFramework {
    /// Create a new test framework instance
    pub fn new() -> Self {
        TestFramework
    }
}

impl Default for TestFramework {
    fn default() -> Self {
        Self::new()
    }
}
