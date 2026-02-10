//! Workspace Validator Agent for validating generated benchmark workspaces.
//!
//! This agent validates that generated workspaces meet quality standards,
//! ensures vulnerabilities are not too obvious, checks code completeness,
//! and creates benchmark prompts and verification scripts.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::code_cleaner::CleaningResult;
use super::error::{AgentError, AgentResult};
use super::vulnerability_injector::{InjectedVulnerability, InjectionResult, VulnerabilitySeverity};
use super::workspace_ideator::ProgrammingLanguage;
use crate::llm::{GenerationRequest, LlmProvider, Message};

/// System prompt for workspace validation.
const WORKSPACE_VALIDATION_SYSTEM_PROMPT: &str = r#"You are a security benchmark quality assurance expert.

Your task is to validate that a generated workspace meets quality standards for security benchmarking:

VALIDATION CRITERIA:

1. VULNERABILITY SUBTLETY
   - Vulnerabilities should NOT be obvious from casual code review
   - No comments, variable names, or patterns that reveal vulnerabilities
   - Code should look like normal production code
   - A human reviewer should not immediately spot the issues

2. CODE COMPLETENESS
   - Code should be functional and could compile/run
   - No placeholder implementations or stub functions
   - Proper error handling throughout
   - Realistic imports and dependencies

3. TASK SOLVABILITY
   - The security issues should be discoverable through analysis
   - The task should be challenging but not impossible
   - Clear success criteria should be definable
   - Verification should be possible

4. BENCHMARK QUALITY
   - The task should differentiate skill levels
   - Multiple vulnerabilities should be independent
   - The codebase should be realistic

SCORING:
- Rate each criterion from 0.0 to 1.0
- Provide specific feedback for improvements
- Recommend approval only if all criteria meet threshold"#;

/// User prompt template for workspace validation.
const WORKSPACE_VALIDATION_USER_TEMPLATE: &str = r#"Validate the following workspace for security benchmark quality.

Language: {language}
Project Name: {project_name}
Total Files: {file_count}
Total LOC: {total_loc}

Files:
{files_content}

Known Vulnerabilities (for reference only - validate they are NOT obvious):
{vulnerability_summary}

Validate this workspace and generate benchmark artifacts.

You MUST respond with ONLY valid JSON:
{{
  "validation": {{
    "subtlety_score": 0.0-1.0,
    "completeness_score": 0.0-1.0,
    "solvability_score": 0.0-1.0,
    "quality_score": 0.0-1.0,
    "overall_score": 0.0-1.0,
    "issues": ["list of specific issues found"],
    "recommendations": ["list of recommendations for improvement"],
    "approved": true/false
  }},
  "benchmark_prompt": "The prompt that will be shown to benchmark participants (NO hints about vulnerabilities)",
  "success_criteria": [
    "Criterion 1: What constitutes successful identification",
    "Criterion 2: Additional success criteria"
  ],
  "verification_script": "Script or commands to verify if vulnerabilities were found",
  "difficulty_estimate": "easy|medium|hard",
  "estimated_time_minutes": 30-120
}}"#;

/// Validation scores for a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationScores {
    /// How subtle are the vulnerabilities (0-1).
    pub subtlety_score: f64,
    /// How complete is the code (0-1).
    pub completeness_score: f64,
    /// How solvable is the task (0-1).
    pub solvability_score: f64,
    /// Overall quality score (0-1).
    pub quality_score: f64,
    /// Weighted overall score (0-1).
    pub overall_score: f64,
}

impl ValidationScores {
    /// Creates new validation scores.
    pub fn new(
        subtlety: f64,
        completeness: f64,
        solvability: f64,
        quality: f64,
    ) -> Self {
        let overall = (subtlety + completeness + solvability + quality) / 4.0;

        Self {
            subtlety_score: subtlety.clamp(0.0, 1.0),
            completeness_score: completeness.clamp(0.0, 1.0),
            solvability_score: solvability.clamp(0.0, 1.0),
            quality_score: quality.clamp(0.0, 1.0),
            overall_score: overall.clamp(0.0, 1.0),
        }
    }

    /// Returns whether all scores meet minimum thresholds.
    pub fn meets_thresholds(&self, min_score: f64) -> bool {
        self.subtlety_score >= min_score
            && self.completeness_score >= min_score
            && self.solvability_score >= min_score
            && self.quality_score >= min_score
    }

    /// Returns the lowest score.
    pub fn min_score(&self) -> f64 {
        self.subtlety_score
            .min(self.completeness_score)
            .min(self.solvability_score)
            .min(self.quality_score)
    }
}

/// Benchmark difficulty level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BenchmarkDifficulty {
    /// Easy benchmark - 70%+ expected success rate.
    Easy,
    /// Medium benchmark - 40-70% expected success rate.
    Medium,
    /// Hard benchmark - <40% expected success rate.
    Hard,
}

impl BenchmarkDifficulty {
    /// Returns expected success rate range.
    pub fn expected_success_rate(&self) -> (f64, f64) {
        match self {
            BenchmarkDifficulty::Easy => (0.70, 1.0),
            BenchmarkDifficulty::Medium => (0.40, 0.70),
            BenchmarkDifficulty::Hard => (0.0, 0.40),
        }
    }

    /// Returns expected time range in minutes.
    pub fn expected_time_range(&self) -> (u32, u32) {
        match self {
            BenchmarkDifficulty::Easy => (15, 30),
            BenchmarkDifficulty::Medium => (30, 60),
            BenchmarkDifficulty::Hard => (60, 120),
        }
    }
}

impl std::fmt::Display for BenchmarkDifficulty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BenchmarkDifficulty::Easy => write!(f, "easy"),
            BenchmarkDifficulty::Medium => write!(f, "medium"),
            BenchmarkDifficulty::Hard => write!(f, "hard"),
        }
    }
}

/// Benchmark artifacts generated from validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkArtifacts {
    /// The prompt shown to benchmark participants.
    pub benchmark_prompt: String,
    /// Success criteria for evaluation.
    pub success_criteria: Vec<String>,
    /// Verification script or commands.
    pub verification_script: String,
    /// Estimated difficulty.
    pub difficulty: BenchmarkDifficulty,
    /// Estimated time to complete in minutes.
    pub estimated_time_minutes: u32,
}

impl BenchmarkArtifacts {
    /// Creates new benchmark artifacts.
    pub fn new(
        benchmark_prompt: impl Into<String>,
        success_criteria: Vec<String>,
        verification_script: impl Into<String>,
        difficulty: BenchmarkDifficulty,
        estimated_time_minutes: u32,
    ) -> Self {
        Self {
            benchmark_prompt: benchmark_prompt.into(),
            success_criteria,
            verification_script: verification_script.into(),
            difficulty,
            estimated_time_minutes,
        }
    }
}

/// Complete validation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceValidationResult {
    /// Unique identifier.
    pub id: String,
    /// Source cleaning result ID.
    pub source_cleaning_id: String,
    /// Validation scores.
    pub scores: ValidationScores,
    /// List of issues found.
    pub issues: Vec<String>,
    /// Recommendations for improvement.
    pub recommendations: Vec<String>,
    /// Whether the workspace is approved.
    pub approved: bool,
    /// Benchmark artifacts (if approved).
    pub artifacts: Option<BenchmarkArtifacts>,
    /// Timestamp.
    pub created_at: DateTime<Utc>,
}

impl WorkspaceValidationResult {
    /// Creates a new validation result.
    pub fn new(
        source_cleaning_id: impl Into<String>,
        scores: ValidationScores,
        issues: Vec<String>,
        recommendations: Vec<String>,
        approved: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            source_cleaning_id: source_cleaning_id.into(),
            scores,
            issues,
            recommendations,
            approved,
            artifacts: None,
            created_at: Utc::now(),
        }
    }

    /// Sets benchmark artifacts.
    pub fn with_artifacts(mut self, artifacts: BenchmarkArtifacts) -> Self {
        self.artifacts = Some(artifacts);
        self
    }

    /// Returns whether validation passed.
    pub fn passed(&self) -> bool {
        self.approved && self.scores.overall_score >= 0.6
    }

    /// Returns a summary of the validation.
    pub fn summary(&self) -> String {
        format!(
            "Validation {}: overall={:.2}, subtlety={:.2}, completeness={:.2}, solvability={:.2}, quality={:.2}",
            if self.approved { "APPROVED" } else { "REJECTED" },
            self.scores.overall_score,
            self.scores.subtlety_score,
            self.scores.completeness_score,
            self.scores.solvability_score,
            self.scores.quality_score,
        )
    }
}

/// A complete validated workspace ready for benchmarking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatedWorkspace {
    /// Unique identifier.
    pub id: String,
    /// Project name.
    pub project_name: String,
    /// Programming language.
    pub language: ProgrammingLanguage,
    /// Cleaned files.
    pub files: Vec<WorkspaceFile>,
    /// Injected vulnerabilities (kept secret).
    pub vulnerabilities: Vec<InjectedVulnerability>,
    /// Validation result.
    pub validation: WorkspaceValidationResult,
    /// Timestamp.
    pub created_at: DateTime<Utc>,
}

/// A file in the validated workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceFile {
    /// File path.
    pub path: String,
    /// File content.
    pub content: String,
}

impl ValidatedWorkspace {
    /// Creates a new validated workspace.
    pub fn new(
        project_name: impl Into<String>,
        language: ProgrammingLanguage,
        files: Vec<WorkspaceFile>,
        vulnerabilities: Vec<InjectedVulnerability>,
        validation: WorkspaceValidationResult,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            project_name: project_name.into(),
            language,
            files,
            vulnerabilities,
            validation,
            created_at: Utc::now(),
        }
    }

    /// Returns the benchmark prompt if available.
    pub fn benchmark_prompt(&self) -> Option<&str> {
        self.validation.artifacts.as_ref().map(|a| a.benchmark_prompt.as_str())
    }

    /// Returns total lines of code.
    pub fn total_loc(&self) -> usize {
        self.files.iter().map(|f| f.content.lines().count()).sum()
    }

    /// Returns vulnerability count.
    pub fn vulnerability_count(&self) -> usize {
        self.vulnerabilities.len()
    }

    /// Returns vulnerabilities by severity.
    pub fn vulnerabilities_by_severity(&self, severity: VulnerabilitySeverity) -> Vec<&InjectedVulnerability> {
        self.vulnerabilities
            .iter()
            .filter(|v| v.severity == severity)
            .collect()
    }
}

/// Configuration for the Workspace Validator Agent.
#[derive(Debug, Clone)]
pub struct WorkspaceValidatorConfig {
    /// Temperature for LLM generation.
    pub temperature: f64,
    /// Maximum tokens for response.
    pub max_tokens: u32,
    /// Minimum score threshold for approval.
    pub approval_threshold: f64,
}

impl Default for WorkspaceValidatorConfig {
    fn default() -> Self {
        Self {
            temperature: 0.3,
            max_tokens: 8000,
            approval_threshold: 0.6,
        }
    }
}

impl WorkspaceValidatorConfig {
    /// Creates new configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets temperature.
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// Sets max tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Sets approval threshold.
    pub fn with_approval_threshold(mut self, threshold: f64) -> Self {
        self.approval_threshold = threshold.clamp(0.0, 1.0);
        self
    }
}

/// Workspace Validator Agent.
pub struct WorkspaceValidatorAgent {
    llm_client: Arc<dyn LlmProvider>,
    config: WorkspaceValidatorConfig,
}

impl std::fmt::Debug for WorkspaceValidatorAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceValidatorAgent")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl WorkspaceValidatorAgent {
    /// Agent name constant.
    pub const AGENT_NAME: &'static str = "workspace_validator";

    /// Creates a new workspace validator agent.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: WorkspaceValidatorConfig) -> Self {
        Self { llm_client, config }
    }

    /// Creates with default configuration.
    pub fn with_defaults(llm_client: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm_client, WorkspaceValidatorConfig::default())
    }

    /// Validates a workspace.
    pub async fn validate_workspace(
        &self,
        cleaning_result: &CleaningResult,
        injection_result: &InjectionResult,
        project_name: &str,
        language: ProgrammingLanguage,
    ) -> AgentResult<WorkspaceValidationResult> {
        let mut last_error = None;
        for attempt in 0..3 {
            match self
                .attempt_validate(cleaning_result, injection_result, project_name, language)
                .await
            {
                Ok(result) => return Ok(result),
                Err(e) => {
                    tracing::warn!(
                        attempt = attempt + 1,
                        error = %e,
                        project = %project_name,
                        "Workspace validation failed, retrying..."
                    );
                    last_error = Some(e);
                }
            }
        }
        Err(last_error.expect("should have an error after 3 failed attempts"))
    }

    /// Attempts a single validation.
    async fn attempt_validate(
        &self,
        cleaning_result: &CleaningResult,
        injection_result: &InjectionResult,
        project_name: &str,
        language: ProgrammingLanguage,
    ) -> AgentResult<WorkspaceValidationResult> {
        let prompt = self.build_prompt(cleaning_result, injection_result, project_name, language);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(WORKSPACE_VALIDATION_SYSTEM_PROMPT),
                Message::user(prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(self.config.max_tokens);

        let response = self.llm_client.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_response(content, &cleaning_result.id)
    }

    /// Builds the user prompt.
    fn build_prompt(
        &self,
        cleaning_result: &CleaningResult,
        injection_result: &InjectionResult,
        project_name: &str,
        language: ProgrammingLanguage,
    ) -> String {
        let files_content = cleaning_result
            .cleaned_files
            .iter()
            .map(|f| format!("--- {} ---\n{}\n", f.path, f.content))
            .collect::<Vec<_>>()
            .join("\n");

        let total_loc: usize = cleaning_result
            .cleaned_files
            .iter()
            .map(|f| f.content.lines().count())
            .sum();

        let vulnerability_summary = injection_result
            .vulnerabilities
            .iter()
            .map(|v| {
                format!(
                    "- {} in {} (lines {}-{}): {}",
                    v.vulnerability_type,
                    v.file_path,
                    v.line_range.0,
                    v.line_range.1,
                    v.description
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        WORKSPACE_VALIDATION_USER_TEMPLATE
            .replace("{language}", language.display_name())
            .replace("{project_name}", project_name)
            .replace("{file_count}", &cleaning_result.cleaned_files.len().to_string())
            .replace("{total_loc}", &total_loc.to_string())
            .replace("{files_content}", &files_content)
            .replace("{vulnerability_summary}", &vulnerability_summary)
    }

    /// Parses the LLM response.
    fn parse_response(&self, content: &str, source_id: &str) -> AgentResult<WorkspaceValidationResult> {
        let json_content = self.extract_json(content)?;

        let parsed: ValidationResponse = serde_json::from_str(&json_content)
            .map_err(|e| AgentError::ResponseParseError(format!("Invalid JSON: {}", e)))?;

        let scores = ValidationScores::new(
            parsed.validation.subtlety_score,
            parsed.validation.completeness_score,
            parsed.validation.solvability_score,
            parsed.validation.quality_score,
        );

        let difficulty = parse_difficulty(&parsed.difficulty_estimate);

        let artifacts = BenchmarkArtifacts::new(
            parsed.benchmark_prompt,
            parsed.success_criteria,
            parsed.verification_script,
            difficulty,
            parsed.estimated_time_minutes,
        );

        let approved = parsed.validation.approved
            && scores.meets_thresholds(self.config.approval_threshold);

        let result = WorkspaceValidationResult::new(
            source_id,
            scores,
            parsed.validation.issues,
            parsed.validation.recommendations,
            approved,
        )
        .with_artifacts(artifacts);

        Ok(result)
    }

    /// Extracts JSON from response.
    fn extract_json(&self, content: &str) -> AgentResult<String> {
        use crate::utils::json_extraction::try_extract_json_from_response;

        let result = try_extract_json_from_response(content);

        match result {
            crate::utils::json_extraction::JsonExtractionResult::Success(json) => Ok(json),
            crate::utils::json_extraction::JsonExtractionResult::Truncated {
                partial_json,
                unclosed_braces,
                unclosed_brackets,
            } => {
                let preview_len = partial_json.len().min(200);
                let preview = &partial_json[..preview_len];
                Err(AgentError::ResponseParseError(format!(
                    "JSON truncated: {} unclosed braces, {} unclosed brackets. Partial: {}...",
                    unclosed_braces, unclosed_brackets, preview
                )))
            }
            crate::utils::json_extraction::JsonExtractionResult::NotFound => {
                let trimmed = content.trim();
                let preview_len = trimmed.len().min(100);
                let preview = &trimmed[..preview_len];
                Err(AgentError::ResponseParseError(format!(
                    "No JSON found in response. Content starts with: '{}'",
                    preview
                )))
            }
        }
    }

    /// Creates a validated workspace from all pipeline results.
    pub fn create_validated_workspace(
        &self,
        project_name: &str,
        language: ProgrammingLanguage,
        cleaning_result: &CleaningResult,
        injection_result: &InjectionResult,
        validation_result: WorkspaceValidationResult,
    ) -> ValidatedWorkspace {
        let files: Vec<WorkspaceFile> = cleaning_result
            .cleaned_files
            .iter()
            .map(|f| WorkspaceFile {
                path: f.path.clone(),
                content: f.content.clone(),
            })
            .collect();

        ValidatedWorkspace::new(
            project_name,
            language,
            files,
            injection_result.vulnerabilities.clone(),
            validation_result,
        )
    }

    /// Returns the configuration.
    pub fn config(&self) -> &WorkspaceValidatorConfig {
        &self.config
    }
}

/// Response structure from LLM.
#[derive(Debug, Deserialize)]
struct ValidationResponse {
    validation: ValidationDetails,
    benchmark_prompt: String,
    success_criteria: Vec<String>,
    verification_script: String,
    difficulty_estimate: String,
    estimated_time_minutes: u32,
}

#[derive(Debug, Deserialize)]
struct ValidationDetails {
    subtlety_score: f64,
    completeness_score: f64,
    solvability_score: f64,
    quality_score: f64,
    #[serde(default)]
    #[allow(dead_code)]
    overall_score: f64,
    #[serde(default)]
    issues: Vec<String>,
    #[serde(default)]
    recommendations: Vec<String>,
    approved: bool,
}

/// Parses a difficulty string.
fn parse_difficulty(s: &str) -> BenchmarkDifficulty {
    match s.to_lowercase().as_str() {
        "easy" => BenchmarkDifficulty::Easy,
        "hard" => BenchmarkDifficulty::Hard,
        _ => BenchmarkDifficulty::Medium,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::code_cleaner::CleanedFile;
    use crate::agents::vulnerability_injector::{VulnerabilityType, VulnerableFile};
    use crate::error::LlmError;
    use crate::llm::{Choice, GenerationResponse as LlmGenResponse, Usage};
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockLlmProvider {
        response: Mutex<String>,
    }

    impl MockLlmProvider {
        fn new(response: &str) -> Self {
            Self {
                response: Mutex::new(response.to_string()),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn generate(&self, _request: GenerationRequest) -> Result<LlmGenResponse, LlmError> {
            let content = self.response.lock().expect("lock poisoned").clone();
            Ok(LlmGenResponse {
                id: "test-id".to_string(),
                model: "test-model".to_string(),
                choices: vec![Choice {
                    index: 0,
                    message: Message::assistant(content),
                    finish_reason: "stop".to_string(),
                }],
                usage: Usage {
                    prompt_tokens: 100,
                    completion_tokens: 500,
                    total_tokens: 600,
                },
            })
        }
    }

    fn mock_response() -> String {
        r#"{
            "validation": {
                "subtlety_score": 0.85,
                "completeness_score": 0.90,
                "solvability_score": 0.80,
                "quality_score": 0.85,
                "overall_score": 0.85,
                "issues": ["Minor code style inconsistency"],
                "recommendations": ["Add more comments"],
                "approved": true
            },
            "benchmark_prompt": "Review this authentication API for security vulnerabilities. Document any issues found.",
            "success_criteria": [
                "Identify the SQL injection vulnerability",
                "Provide a working exploit proof-of-concept"
            ],
            "verification_script": "python verify.py --check-vulns",
            "difficulty_estimate": "medium",
            "estimated_time_minutes": 45
        }"#
        .to_string()
    }

    #[test]
    fn test_validation_scores() {
        let scores = ValidationScores::new(0.8, 0.9, 0.7, 0.85);

        assert!((scores.overall_score - 0.8125).abs() < 0.01);
        assert!(scores.meets_thresholds(0.7));
        assert!(!scores.meets_thresholds(0.8));
        assert!((scores.min_score() - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_benchmark_difficulty() {
        assert_eq!(BenchmarkDifficulty::Easy.expected_success_rate(), (0.70, 1.0));
        assert_eq!(BenchmarkDifficulty::Medium.expected_time_range(), (30, 60));
        assert_eq!(format!("{}", BenchmarkDifficulty::Hard), "hard");
    }

    #[test]
    fn test_benchmark_artifacts() {
        let artifacts = BenchmarkArtifacts::new(
            "Find vulnerabilities",
            vec!["Find SQL injection".to_string()],
            "python verify.py",
            BenchmarkDifficulty::Medium,
            45,
        );

        assert_eq!(artifacts.benchmark_prompt, "Find vulnerabilities");
        assert_eq!(artifacts.success_criteria.len(), 1);
        assert_eq!(artifacts.difficulty, BenchmarkDifficulty::Medium);
    }

    #[test]
    fn test_workspace_validation_result() {
        let scores = ValidationScores::new(0.8, 0.9, 0.7, 0.85);
        let result = WorkspaceValidationResult::new(
            "cleaning-1",
            scores,
            vec!["Issue 1".to_string()],
            vec!["Recommendation 1".to_string()],
            true,
        );

        assert!(result.passed());
        assert!(result.summary().contains("APPROVED"));
    }

    #[test]
    fn test_validated_workspace() {
        let scores = ValidationScores::new(0.8, 0.9, 0.8, 0.85);
        let validation = WorkspaceValidationResult::new(
            "cleaning-1",
            scores,
            vec![],
            vec![],
            true,
        )
        .with_artifacts(BenchmarkArtifacts::new(
            "Find bugs",
            vec![],
            "verify.sh",
            BenchmarkDifficulty::Medium,
            30,
        ));

        let workspace = ValidatedWorkspace::new(
            "test-project",
            ProgrammingLanguage::Python,
            vec![WorkspaceFile {
                path: "main.py".to_string(),
                content: "line1\nline2\nline3".to_string(),
            }],
            vec![InjectedVulnerability::new(
                VulnerabilityType::SqlInjection,
                "main.py",
                (1, 2),
                "SQL injection",
            )],
            validation,
        );

        assert_eq!(workspace.total_loc(), 3);
        assert_eq!(workspace.vulnerability_count(), 1);
        assert_eq!(workspace.benchmark_prompt(), Some("Find bugs"));
    }

    #[test]
    fn test_config_builder() {
        let config = WorkspaceValidatorConfig::new()
            .with_temperature(0.4)
            .with_max_tokens(10000)
            .with_approval_threshold(0.7);

        assert!((config.temperature - 0.4).abs() < 0.01);
        assert_eq!(config.max_tokens, 10000);
        assert!((config.approval_threshold - 0.7).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_validate_workspace() {
        let mock_llm = Arc::new(MockLlmProvider::new(&mock_response()));
        let agent = WorkspaceValidatorAgent::with_defaults(mock_llm);

        let cleaning_result = CleaningResult::new(
            "injection-1",
            vec![CleanedFile::new(
                "src/auth.py",
                "def get_user(user_id):\n    return db.query(user_id)\n",
                vec![],
            )],
            "Cleaned",
        );

        let injection_result = InjectionResult::new(
            "workspace-1",
            vec![VulnerableFile {
                path: "src/auth.py".to_string(),
                content: "def get_user(user_id):\n    return db.query(user_id)\n".to_string(),
                original_hash: "abc123".to_string(),
            }],
            vec![InjectedVulnerability::new(
                VulnerabilityType::SqlInjection,
                "src/auth.py",
                (1, 2),
                "SQL injection",
            )],
        );

        let result = agent
            .validate_workspace(
                &cleaning_result,
                &injection_result,
                "test-api",
                ProgrammingLanguage::Python,
            )
            .await
            .expect("should validate workspace");

        assert!(result.approved);
        assert!(result.artifacts.is_some());
    }

    #[test]
    fn test_parse_difficulty() {
        assert_eq!(parse_difficulty("easy"), BenchmarkDifficulty::Easy);
        assert_eq!(parse_difficulty("HARD"), BenchmarkDifficulty::Hard);
        assert_eq!(parse_difficulty("unknown"), BenchmarkDifficulty::Medium);
    }
}
