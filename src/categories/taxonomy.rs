//! Category taxonomy system for dataforge.
//!
//! Defines the 12 main benchmark categories and their subcategories.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The main benchmark categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    // Core categories
    Debugging,
    Security,
    SystemAdministration,
    SoftwareEngineering,
    FileOperations,
    DataScience,
    Networking,
    Containers,

    // Advanced categories for synthetic benchmarks
    AlgorithmDesign,
    ReverseEngineering,
    PerformanceOptimization,
    IntegrationTasks,
}

impl Category {
    /// Returns all available categories.
    pub fn all() -> Vec<Category> {
        vec![
            Category::Debugging,
            Category::Security,
            Category::SystemAdministration,
            Category::SoftwareEngineering,
            Category::FileOperations,
            Category::DataScience,
            Category::Networking,
            Category::Containers,
            Category::AlgorithmDesign,
            Category::ReverseEngineering,
            Category::PerformanceOptimization,
            Category::IntegrationTasks,
        ]
    }

    /// Returns all advanced synthetic benchmark categories.
    pub fn all_advanced() -> Vec<Category> {
        vec![
            Category::AlgorithmDesign,
            Category::ReverseEngineering,
            Category::PerformanceOptimization,
            Category::IntegrationTasks,
        ]
    }

    /// Returns true if this category is an advanced synthetic benchmark category.
    pub fn is_advanced(&self) -> bool {
        matches!(
            self,
            Category::AlgorithmDesign
                | Category::ReverseEngineering
                | Category::PerformanceOptimization
                | Category::IntegrationTasks
        )
    }

    /// Returns the target distribution percentage for this category.
    /// Distributions are designed to provide balanced coverage while
    /// emphasizing core system administration and debugging tasks.
    pub fn target_distribution(&self) -> f64 {
        match self {
            // Core categories
            Category::Debugging => 0.12,
            Category::Security => 0.12,
            Category::SystemAdministration => 0.10,
            Category::SoftwareEngineering => 0.10,
            Category::FileOperations => 0.08,
            Category::DataScience => 0.08,
            Category::Networking => 0.08,
            Category::Containers => 0.05,
            // Advanced categories
            Category::AlgorithmDesign => 0.08,
            Category::ReverseEngineering => 0.06,
            Category::PerformanceOptimization => 0.07,
            Category::IntegrationTasks => 0.06,
        }
    }

    /// Returns a base difficulty weight for this category (0.0-1.0).
    /// Higher values indicate categories that typically produce harder tasks.
    pub fn difficulty_weight(&self) -> f64 {
        match self {
            Category::FileOperations => 0.3,
            Category::Containers => 0.4,
            Category::Networking => 0.5,
            Category::SystemAdministration => 0.5,
            Category::DataScience => 0.5,
            Category::Debugging => 0.6,
            Category::SoftwareEngineering => 0.6,
            Category::Security => 0.7,
            Category::AlgorithmDesign => 0.7,
            Category::PerformanceOptimization => 0.7,
            Category::IntegrationTasks => 0.8,
            Category::ReverseEngineering => 0.8,
        }
    }

    /// Returns the subcategories for this category.
    pub fn subcategories(&self) -> Vec<&'static str> {
        match self {
            Category::Debugging => vec![
                "log-analysis",
                "error-fixing",
                "crash-investigation",
                "performance-debugging",
                "integration-debugging",
            ],
            Category::Security => vec![
                "vulnerability-detection",
                "hardening",
                "ctf-challenges",
                "incident-response",
                "audit",
            ],
            Category::SystemAdministration => vec![
                "service-configuration",
                "user-management",
                "storage-management",
                "process-management",
                "package-management",
            ],
            Category::SoftwareEngineering => vec![
                "build-systems",
                "version-control",
                "refactoring",
                "dependency-management",
                "testing",
            ],
            Category::FileOperations => vec![
                "text-processing",
                "search-replace",
                "archival",
                "file-organization",
            ],
            Category::DataScience => vec![
                "data-wrangling",
                "analysis",
                "visualization",
                "ml-workflows",
            ],
            Category::Networking => vec![
                "dns-configuration",
                "firewall",
                "proxy-setup",
                "diagnostics",
                "vpn-tunneling",
            ],
            Category::Containers => vec!["docker-operations", "compose", "kubernetes"],
            Category::AlgorithmDesign => vec![
                "graph-algorithms",
                "dynamic-programming",
                "optimization",
                "constraint-satisfaction",
                "search-algorithms",
            ],
            Category::ReverseEngineering => vec![
                "binary-analysis",
                "protocol-decoding",
                "malware-analysis",
                "firmware-extraction",
                "obfuscation-analysis",
            ],
            Category::PerformanceOptimization => vec![
                "profiling",
                "memory-optimization",
                "cpu-optimization",
                "io-optimization",
                "concurrency-optimization",
            ],
            Category::IntegrationTasks => vec![
                "api-orchestration",
                "event-driven",
                "saga-patterns",
                "service-mesh",
                "data-synchronization",
            ],
        }
    }

    /// Returns the human-readable display name for this category.
    pub fn display_name(&self) -> &'static str {
        match self {
            Category::Debugging => "Debugging",
            Category::Security => "Security",
            Category::SystemAdministration => "System Administration",
            Category::SoftwareEngineering => "Software Engineering",
            Category::FileOperations => "File Operations",
            Category::DataScience => "Data Science",
            Category::Networking => "Networking",
            Category::Containers => "Containers",
            Category::AlgorithmDesign => "Algorithm Design",
            Category::ReverseEngineering => "Reverse Engineering",
            Category::PerformanceOptimization => "Performance Optimization",
            Category::IntegrationTasks => "Integration Tasks",
        }
    }
}

/// Metadata associated with a categorized task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryMetadata {
    /// The primary category of the task.
    pub category: Category,
    /// The subcategory within the primary category.
    pub subcategory: String,
    /// Tags for additional classification and filtering.
    pub tags: Vec<String>,
    /// Additional categories this task spans (for cross-category tasks).
    pub cross_categories: Vec<Category>,
}

impl CategoryMetadata {
    /// Creates a new CategoryMetadata instance.
    pub fn new(category: Category, subcategory: impl Into<String>) -> Self {
        Self {
            category,
            subcategory: subcategory.into(),
            tags: Vec::new(),
            cross_categories: Vec::new(),
        }
    }

    /// Adds a tag to the metadata.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Adds multiple tags to the metadata.
    pub fn with_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tags.extend(tags.into_iter().map(|t| t.into()));
        self
    }

    /// Adds a cross-category reference.
    pub fn with_cross_category(mut self, category: Category) -> Self {
        if !self.cross_categories.contains(&category) && category != self.category {
            self.cross_categories.push(category);
        }
        self
    }
}

/// Registry for tracking tasks by category.
#[derive(Debug, Clone, Default)]
pub struct CategoryRegistry {
    tasks_by_category: HashMap<Category, Vec<String>>,
}

impl CategoryRegistry {
    /// Creates a new empty registry.
    pub fn new() -> Self {
        Self {
            tasks_by_category: HashMap::new(),
        }
    }

    /// Registers a task with its category metadata.
    pub fn register(&mut self, task_id: &str, metadata: &CategoryMetadata) {
        // Register under primary category
        self.tasks_by_category
            .entry(metadata.category)
            .or_default()
            .push(task_id.to_string());

        // Also register under cross-categories
        for cross_cat in &metadata.cross_categories {
            self.tasks_by_category
                .entry(*cross_cat)
                .or_default()
                .push(task_id.to_string());
        }
    }

    /// Returns all task IDs for a given category.
    pub fn get_by_category(&self, category: Category) -> Vec<&str> {
        self.tasks_by_category
            .get(&category)
            .map(|tasks| tasks.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Returns the current distribution of tasks across categories.
    pub fn get_distribution(&self) -> HashMap<Category, usize> {
        let mut distribution = HashMap::new();
        for category in Category::all() {
            let count = self
                .tasks_by_category
                .get(&category)
                .map(|v| v.len())
                .unwrap_or(0);
            distribution.insert(category, count);
        }
        distribution
    }

    /// Checks if the current distribution is balanced and returns warnings for imbalanced categories.
    ///
    /// A category is considered imbalanced if its actual distribution differs from
    /// the target by more than 5 percentage points (absolute).
    pub fn check_distribution_balance(&self) -> Vec<String> {
        let distribution = self.get_distribution();
        let total: usize = distribution.values().sum();

        if total == 0 {
            return vec!["No tasks registered in the registry".to_string()];
        }

        let tolerance = 0.05; // 5% tolerance
        let mut warnings = Vec::new();

        for category in Category::all() {
            let count = distribution.get(&category).copied().unwrap_or(0);
            let actual_pct = count as f64 / total as f64;
            let target_pct = category.target_distribution();
            let diff = (actual_pct - target_pct).abs();

            if diff > tolerance {
                let direction = if actual_pct > target_pct {
                    "over-represented"
                } else {
                    "under-represented"
                };
                warnings.push(format!(
                    "{} is {}: {:.1}% actual vs {:.1}% target (diff: {:.1}%)",
                    category.display_name(),
                    direction,
                    actual_pct * 100.0,
                    target_pct * 100.0,
                    diff * 100.0
                ));
            }
        }

        warnings
    }

    /// Returns the total number of registered tasks.
    pub fn total_tasks(&self) -> usize {
        self.tasks_by_category.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_category_all() {
        let all = Category::all();
        assert_eq!(
            all.len(),
            12,
            "Expected 12 categories (8 core + 4 advanced)"
        );
    }

    #[test]
    fn test_category_all_advanced() {
        let advanced = Category::all_advanced();
        assert_eq!(advanced.len(), 4, "Expected 4 advanced categories");
        assert!(advanced.contains(&Category::AlgorithmDesign));
        assert!(advanced.contains(&Category::ReverseEngineering));
        assert!(advanced.contains(&Category::PerformanceOptimization));
        assert!(advanced.contains(&Category::IntegrationTasks));
    }

    #[test]
    fn test_is_advanced() {
        // Advanced categories should return true
        assert!(Category::AlgorithmDesign.is_advanced());
        assert!(Category::ReverseEngineering.is_advanced());
        assert!(Category::PerformanceOptimization.is_advanced());
        assert!(Category::IntegrationTasks.is_advanced());

        // Core categories should return false
        assert!(!Category::Debugging.is_advanced());
        assert!(!Category::Security.is_advanced());
        assert!(!Category::SystemAdministration.is_advanced());
        assert!(!Category::SoftwareEngineering.is_advanced());
        assert!(!Category::FileOperations.is_advanced());
        assert!(!Category::DataScience.is_advanced());
        assert!(!Category::Networking.is_advanced());
        assert!(!Category::Containers.is_advanced());
    }

    #[test]
    fn test_target_distribution_sums_to_one() {
        let total: f64 = Category::all()
            .iter()
            .map(|c| c.target_distribution())
            .sum();
        assert!(
            (total - 1.0).abs() < 0.001,
            "Distribution should sum to 1.0, got {}",
            total
        );
    }

    #[test]
    fn test_difficulty_weight_in_valid_range() {
        for category in Category::all() {
            let weight = category.difficulty_weight();
            assert!(
                (0.0..=1.0).contains(&weight),
                "{:?} difficulty weight {} should be between 0.0 and 1.0",
                category,
                weight
            );
        }
    }

    #[test]
    fn test_difficulty_weight_ordering() {
        // FileOperations should be easiest
        assert!(
            Category::FileOperations.difficulty_weight() < Category::Security.difficulty_weight()
        );
        // ReverseEngineering should be hardest
        assert!(
            Category::ReverseEngineering.difficulty_weight()
                >= Category::Debugging.difficulty_weight()
        );
        // Advanced categories should generally be harder
        assert!(Category::ReverseEngineering.difficulty_weight() >= 0.7);
        assert!(Category::IntegrationTasks.difficulty_weight() >= 0.7);
    }

    #[test]
    fn test_subcategories_not_empty() {
        for category in Category::all() {
            let subs = category.subcategories();
            assert!(!subs.is_empty(), "{:?} should have subcategories", category);
        }
    }

    #[test]
    fn test_advanced_subcategories() {
        // Verify advanced categories have expected subcategories
        let algo_subs = Category::AlgorithmDesign.subcategories();
        assert!(algo_subs.contains(&"graph-algorithms"));
        assert!(algo_subs.contains(&"dynamic-programming"));

        let rev_subs = Category::ReverseEngineering.subcategories();
        assert!(rev_subs.contains(&"binary-analysis"));
        assert!(rev_subs.contains(&"protocol-decoding"));

        let perf_subs = Category::PerformanceOptimization.subcategories();
        assert!(perf_subs.contains(&"profiling"));
        assert!(perf_subs.contains(&"memory-optimization"));

        let int_subs = Category::IntegrationTasks.subcategories();
        assert!(int_subs.contains(&"api-orchestration"));
        assert!(int_subs.contains(&"saga-patterns"));
    }

    #[test]
    fn test_display_names() {
        assert_eq!(Category::AlgorithmDesign.display_name(), "Algorithm Design");
        assert_eq!(
            Category::ReverseEngineering.display_name(),
            "Reverse Engineering"
        );
        assert_eq!(
            Category::PerformanceOptimization.display_name(),
            "Performance Optimization"
        );
        assert_eq!(
            Category::IntegrationTasks.display_name(),
            "Integration Tasks"
        );
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut registry = CategoryRegistry::new();
        let metadata = CategoryMetadata::new(Category::Debugging, "log-analysis");

        registry.register("task-1", &metadata);

        let tasks = registry.get_by_category(Category::Debugging);
        assert_eq!(tasks, vec!["task-1"]);
    }

    #[test]
    fn test_registry_cross_category() {
        let mut registry = CategoryRegistry::new();
        let metadata = CategoryMetadata::new(Category::Debugging, "integration-debugging")
            .with_cross_category(Category::Networking);

        registry.register("task-1", &metadata);

        let debug_tasks = registry.get_by_category(Category::Debugging);
        let network_tasks = registry.get_by_category(Category::Networking);

        assert_eq!(debug_tasks, vec!["task-1"]);
        assert_eq!(network_tasks, vec!["task-1"]);
    }

    #[test]
    fn test_registry_advanced_categories() {
        let mut registry = CategoryRegistry::new();
        let metadata = CategoryMetadata::new(Category::AlgorithmDesign, "graph-algorithms");

        registry.register("task-algo-1", &metadata);

        let tasks = registry.get_by_category(Category::AlgorithmDesign);
        assert_eq!(tasks, vec!["task-algo-1"]);

        // Verify distribution includes all 12 categories
        let distribution = registry.get_distribution();
        assert_eq!(distribution.len(), 12);
    }
}
