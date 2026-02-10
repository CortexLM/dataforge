//! Configuration types for synthetic workspace generation.

use serde::{Deserialize, Serialize};

/// Target programming language for workspace generation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguageTarget {
    #[default]
    Python,
    JavaScript,
    TypeScript,
    Rust,
    Go,
    Java,
    Ruby,
    Php,
    Cpp,
    Csharp,
}

impl LanguageTarget {
    /// Returns the display name for the language.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Python => "Python",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Rust => "Rust",
            Self::Go => "Go",
            Self::Java => "Java",
            Self::Ruby => "Ruby",
            Self::Php => "PHP",
            Self::Cpp => "C++",
            Self::Csharp => "C#",
        }
    }

    /// Returns common file extensions for the language.
    pub fn extensions(&self) -> &[&'static str] {
        match self {
            Self::Python => &["py"],
            Self::JavaScript => &["js", "mjs", "cjs"],
            Self::TypeScript => &["ts", "tsx"],
            Self::Rust => &["rs"],
            Self::Go => &["go"],
            Self::Java => &["java"],
            Self::Ruby => &["rb"],
            Self::Php => &["php"],
            Self::Cpp => &["cpp", "cc", "cxx", "hpp", "h"],
            Self::Csharp => &["cs"],
        }
    }

    /// Returns the package manager file for the language.
    pub fn package_file(&self) -> &'static str {
        match self {
            Self::Python => "requirements.txt",
            Self::JavaScript | Self::TypeScript => "package.json",
            Self::Rust => "Cargo.toml",
            Self::Go => "go.mod",
            Self::Java => "pom.xml",
            Self::Ruby => "Gemfile",
            Self::Php => "composer.json",
            Self::Cpp => "CMakeLists.txt",
            Self::Csharp => "*.csproj",
        }
    }

    /// Returns the test command for the language.
    pub fn test_command(&self) -> &'static str {
        match self {
            Self::Python => "pytest",
            Self::JavaScript | Self::TypeScript => "npm test",
            Self::Rust => "cargo test",
            Self::Go => "go test ./...",
            Self::Java => "mvn test",
            Self::Ruby => "bundle exec rspec",
            Self::Php => "phpunit",
            Self::Cpp => "ctest",
            Self::Csharp => "dotnet test",
        }
    }

    /// Returns build artifacts to exclude from exports.
    pub fn artifact_patterns(&self) -> &[&'static str] {
        match self {
            Self::Python => &[
                "__pycache__/**",
                "*.pyc",
                "*.pyo",
                "*.pyd",
                ".pytest_cache/**",
                ".mypy_cache/**",
                "venv/**",
                ".venv/**",
                "*.egg-info/**",
                "dist/**",
                "build/**",
            ],
            Self::JavaScript | Self::TypeScript => &[
                "node_modules/**",
                "dist/**",
                "build/**",
                ".next/**",
                ".nuxt/**",
                "coverage/**",
                "*.log",
            ],
            Self::Rust => &["target/**", "Cargo.lock"],
            Self::Go => &["vendor/**", "bin/**"],
            Self::Java => &["target/**", "*.class", "*.jar", ".gradle/**", "build/**"],
            Self::Ruby => &["vendor/bundle/**", ".bundle/**", "coverage/**"],
            Self::Php => &["vendor/**"],
            Self::Cpp => &["build/**", "cmake-build-*/**", "*.o", "*.obj", "*.exe"],
            Self::Csharp => &["bin/**", "obj/**", "packages/**"],
        }
    }
}

impl std::fmt::Display for LanguageTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Project category for workspace generation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectCategory {
    /// REST API or web service
    #[default]
    WebApi,
    /// Command-line interface tool
    CliTool,
    /// Web application with frontend
    WebApp,
    /// Data processing pipeline
    DataPipeline,
    /// Microservice component
    Microservice,
    /// Library or SDK
    Library,
    /// Backend service
    BackendService,
    /// File processing utility
    FileProcessor,
    /// Authentication service
    AuthService,
    /// Database utility
    DatabaseTool,
}

impl ProjectCategory {
    /// Returns a description of the category.
    pub fn description(&self) -> &'static str {
        match self {
            Self::WebApi => "REST API with CRUD operations and authentication",
            Self::CliTool => "Command-line utility with argument parsing",
            Self::WebApp => "Web application with frontend and backend",
            Self::DataPipeline => "Data processing and transformation pipeline",
            Self::Microservice => "Containerized microservice component",
            Self::Library => "Reusable library or SDK",
            Self::BackendService => "Backend service with business logic",
            Self::FileProcessor => "File upload, download, and processing",
            Self::AuthService => "Authentication and authorization service",
            Self::DatabaseTool => "Database management and query utility",
        }
    }

    /// Returns common vulnerability types for this category.
    pub fn common_vulnerabilities(&self) -> &[&'static str] {
        match self {
            Self::WebApi | Self::WebApp | Self::BackendService => &[
                "sql_injection",
                "xss",
                "csrf",
                "authentication_bypass",
                "insecure_deserialization",
                "ssrf",
            ],
            Self::CliTool | Self::FileProcessor => &[
                "command_injection",
                "path_traversal",
                "arbitrary_file_write",
                "race_condition",
            ],
            Self::DataPipeline => &[
                "code_injection",
                "insecure_deserialization",
                "path_traversal",
                "race_condition",
            ],
            Self::Microservice => &["ssrf", "authentication_bypass", "insecure_api", "injection"],
            Self::Library => &[
                "prototype_pollution",
                "regex_dos",
                "buffer_overflow",
                "integer_overflow",
            ],
            Self::AuthService => &[
                "authentication_bypass",
                "weak_cryptography",
                "session_fixation",
                "timing_attack",
                "brute_force",
            ],
            Self::DatabaseTool => &[
                "sql_injection",
                "privilege_escalation",
                "hardcoded_credentials",
            ],
        }
    }
}

impl std::fmt::Display for ProjectCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::WebApi => "Web API",
            Self::CliTool => "CLI Tool",
            Self::WebApp => "Web App",
            Self::DataPipeline => "Data Pipeline",
            Self::Microservice => "Microservice",
            Self::Library => "Library",
            Self::BackendService => "Backend Service",
            Self::FileProcessor => "File Processor",
            Self::AuthService => "Auth Service",
            Self::DatabaseTool => "Database Tool",
        };
        write!(f, "{}", name)
    }
}

/// Difficulty level for the generated task.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DifficultyLevel {
    /// Easy: 1-2 obvious vulnerabilities
    Easy,
    /// Medium: 3-4 moderate vulnerabilities
    #[default]
    Medium,
    /// Hard: 5-7 subtle, interconnected vulnerabilities
    Hard,
    /// Expert: 8+ complex, multi-layer vulnerabilities
    Expert,
}

impl DifficultyLevel {
    /// Returns the expected vulnerability count range.
    pub fn vulnerability_range(&self) -> (usize, usize) {
        match self {
            Self::Easy => (1, 2),
            Self::Medium => (3, 4),
            Self::Hard => (5, 7),
            Self::Expert => (8, 12),
        }
    }

    /// Returns the expected file count range.
    pub fn file_count_range(&self) -> (usize, usize) {
        match self {
            Self::Easy => (3, 6),
            Self::Medium => (6, 12),
            Self::Hard => (12, 20),
            Self::Expert => (20, 40),
        }
    }

    /// Returns the expected lines of code range.
    pub fn loc_range(&self) -> (usize, usize) {
        match self {
            Self::Easy => (500, 1000),
            Self::Medium => (1500, 3000),
            Self::Hard => (3500, 6000),
            Self::Expert => (6000, 15000),
        }
    }

    /// Returns a description of the difficulty level.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Easy => "Straightforward vulnerabilities with obvious patterns",
            Self::Medium => "Moderate complexity requiring careful analysis",
            Self::Hard => "Subtle issues requiring deep code review",
            Self::Expert => "Complex, multi-layer vulnerabilities requiring expert analysis",
        }
    }
}

impl std::fmt::Display for DifficultyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Easy => "Easy",
            Self::Medium => "Medium",
            Self::Hard => "Hard",
            Self::Expert => "Expert",
        };
        write!(f, "{}", name)
    }
}

/// Configuration for vulnerability injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityConfig {
    /// Minimum number of vulnerabilities to inject.
    pub min_count: usize,
    /// Maximum number of vulnerabilities to inject.
    pub max_count: usize,
    /// Specific vulnerability types to include (empty = any).
    pub required_types: Vec<String>,
    /// Vulnerability types to exclude.
    pub excluded_types: Vec<String>,
    /// Whether to include subtle/advanced vulnerabilities.
    pub include_subtle: bool,
    /// Whether to include chained/multi-step vulnerabilities.
    pub include_chained: bool,
}

impl Default for VulnerabilityConfig {
    fn default() -> Self {
        Self {
            min_count: 3,
            max_count: 7,
            required_types: Vec::new(),
            excluded_types: Vec::new(),
            include_subtle: true,
            include_chained: false,
        }
    }
}

impl VulnerabilityConfig {
    /// Creates a new vulnerability config.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the minimum vulnerability count.
    pub fn with_min_count(mut self, count: usize) -> Self {
        self.min_count = count;
        self
    }

    /// Sets the maximum vulnerability count.
    pub fn with_max_count(mut self, count: usize) -> Self {
        self.max_count = count;
        self
    }

    /// Adds required vulnerability types.
    pub fn with_required_types<I, S>(mut self, types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.required_types
            .extend(types.into_iter().map(Into::into));
        self
    }

    /// Adds excluded vulnerability types.
    pub fn with_excluded_types<I, S>(mut self, types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.excluded_types
            .extend(types.into_iter().map(Into::into));
        self
    }

    /// Sets whether to include subtle vulnerabilities.
    pub fn with_subtle(mut self, include: bool) -> Self {
        self.include_subtle = include;
        self
    }

    /// Sets whether to include chained vulnerabilities.
    pub fn with_chained(mut self, include: bool) -> Self {
        self.include_chained = include;
        self
    }
}

/// Main configuration for synthetic workspace generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntheticWorkspaceConfig {
    /// Target programming language.
    pub language: LanguageTarget,
    /// Project category.
    pub category: ProjectCategory,
    /// Difficulty level.
    pub difficulty: DifficultyLevel,
    /// Vulnerability configuration.
    pub vulnerabilities: VulnerabilityConfig,
    /// Model to use for code generation.
    pub generation_model: String,
    /// Model to use for debate/validation.
    pub debate_model: String,
    /// Temperature for code generation.
    pub generation_temperature: f64,
    /// Temperature for debate/reasoning.
    pub debate_temperature: f64,
    /// Maximum tokens for generation.
    pub max_generation_tokens: u32,
    /// Number of debate rounds.
    pub debate_rounds: u32,
    /// Consensus threshold for debates (0.0-1.0).
    pub consensus_threshold: f64,
    /// Whether to auto-clean generated code.
    pub auto_clean: bool,
    /// Whether to validate feasibility.
    pub validate_feasibility: bool,
    /// Random seed for reproducibility.
    pub seed: Option<u64>,
    /// Output directory for exports.
    pub output_dir: String,
}

impl Default for SyntheticWorkspaceConfig {
    fn default() -> Self {
        Self {
            language: LanguageTarget::default(),
            category: ProjectCategory::default(),
            difficulty: DifficultyLevel::default(),
            vulnerabilities: VulnerabilityConfig::default(),
            generation_model: "moonshotai/kimi-k2.5".to_string(),
            debate_model: "moonshotai/kimi-k2.5".to_string(),
            generation_temperature: 0.4,
            debate_temperature: 0.7,
            max_generation_tokens: 16000,
            debate_rounds: 3,
            consensus_threshold: 0.6,
            auto_clean: true,
            validate_feasibility: true,
            seed: None,
            output_dir: "./generated-workspaces".to_string(),
        }
    }
}

impl SyntheticWorkspaceConfig {
    /// Creates a new config with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the target language.
    pub fn with_language(mut self, language: LanguageTarget) -> Self {
        self.language = language;
        self
    }

    /// Sets the project category.
    pub fn with_category(mut self, category: ProjectCategory) -> Self {
        self.category = category;
        self
    }

    /// Sets the difficulty level.
    pub fn with_difficulty(mut self, difficulty: DifficultyLevel) -> Self {
        self.difficulty = difficulty;
        // Update vulnerability counts based on difficulty
        let (min, max) = difficulty.vulnerability_range();
        self.vulnerabilities.min_count = min;
        self.vulnerabilities.max_count = max;
        self
    }

    /// Sets the vulnerability configuration.
    pub fn with_vulnerabilities(mut self, config: VulnerabilityConfig) -> Self {
        self.vulnerabilities = config;
        self
    }

    /// Sets the minimum vulnerability count.
    pub fn with_min_vulnerabilities(mut self, count: usize) -> Self {
        self.vulnerabilities.min_count = count;
        self
    }

    /// Sets the maximum vulnerability count.
    pub fn with_max_vulnerabilities(mut self, count: usize) -> Self {
        self.vulnerabilities.max_count = count;
        self
    }

    /// Sets the generation model.
    pub fn with_generation_model(mut self, model: impl Into<String>) -> Self {
        self.generation_model = model.into();
        self
    }

    /// Sets the debate model.
    pub fn with_debate_model(mut self, model: impl Into<String>) -> Self {
        self.debate_model = model.into();
        self
    }

    /// Sets the generation temperature.
    pub fn with_generation_temperature(mut self, temp: f64) -> Self {
        self.generation_temperature = temp.clamp(0.0, 2.0);
        self
    }

    /// Sets the debate temperature.
    pub fn with_debate_temperature(mut self, temp: f64) -> Self {
        self.debate_temperature = temp.clamp(0.0, 2.0);
        self
    }

    /// Sets the number of debate rounds.
    pub fn with_debate_rounds(mut self, rounds: u32) -> Self {
        self.debate_rounds = rounds.max(1);
        self
    }

    /// Sets the consensus threshold.
    pub fn with_consensus_threshold(mut self, threshold: f64) -> Self {
        self.consensus_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Sets whether to auto-clean generated code.
    pub fn with_auto_clean(mut self, clean: bool) -> Self {
        self.auto_clean = clean;
        self
    }

    /// Sets the random seed.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Sets the output directory.
    pub fn with_output_dir(mut self, dir: impl Into<String>) -> Self {
        self.output_dir = dir.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_target() {
        assert_eq!(LanguageTarget::Python.display_name(), "Python");
        assert!(!LanguageTarget::Python.extensions().is_empty());
        assert!(!LanguageTarget::Python.artifact_patterns().is_empty());
    }

    #[test]
    fn test_difficulty_level() {
        let easy = DifficultyLevel::Easy;
        let (min, max) = easy.vulnerability_range();
        assert!(min <= max);
        assert!(min >= 1);

        let expert = DifficultyLevel::Expert;
        let (e_min, e_max) = expert.vulnerability_range();
        assert!(e_min > min);
        assert!(e_max > max);
    }

    #[test]
    fn test_config_builder() {
        let config = SyntheticWorkspaceConfig::new()
            .with_language(LanguageTarget::Rust)
            .with_difficulty(DifficultyLevel::Hard)
            .with_min_vulnerabilities(5)
            .with_debate_rounds(5);

        assert_eq!(config.language, LanguageTarget::Rust);
        assert_eq!(config.difficulty, DifficultyLevel::Hard);
        assert_eq!(config.vulnerabilities.min_count, 5);
        assert_eq!(config.debate_rounds, 5);
    }

    #[test]
    fn test_project_category_vulnerabilities() {
        let web_api = ProjectCategory::WebApi;
        let vulns = web_api.common_vulnerabilities();
        assert!(vulns.contains(&"sql_injection"));
        assert!(vulns.contains(&"xss"));

        let cli = ProjectCategory::CliTool;
        let cli_vulns = cli.common_vulnerabilities();
        assert!(cli_vulns.contains(&"command_injection"));
    }
}
