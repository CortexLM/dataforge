//! Research Agent - Identifies what makes tasks difficult for LLMs.
//!
//! This agent analyzes LLM weaknesses and proposes trap mechanisms that would
//! trip up AI systems while keeping tasks solvable for expert humans.
//!
//! The Research Agent:
//! - Analyzes LLM weaknesses (multi-step reasoning, state tracking, temporal awareness)
//! - Identifies domain-specific challenges for each task category
//! - Proposes trap mechanisms that would trip up LLMs
//! - Uses high temperature for creative exploration of difficulty mechanisms
//!
//! # Example
//!
//! ```ignore
//! use dataforge::agents::research_agent::{ResearchAgent, ResearchConfig};
//! use dataforge::llm::LiteLlmClient;
//! use std::sync::Arc;
//!
//! let llm_client = Arc::new(LiteLlmClient::from_env()?);
//! let config = ResearchConfig::default();
//! let agent = ResearchAgent::new(llm_client, config);
//!
//! let findings = agent.research_category("debugging").await?;
//! println!("Found {} weaknesses", findings.identified_weaknesses.len());
//! ```

use std::sync::Arc;

use serde::Deserialize;

use crate::llm::{GenerationRequest, LlmProvider, Message};
use crate::prompts::{build_research_prompt, RESEARCH_AGENT_SYSTEM};
use crate::utils::json_extraction::{try_extract_json_from_response, JsonExtractionError};

use super::error::{AgentError, AgentResult};
use super::factory_types::{
    DifficultyFactor, DifficultyTrap, DifficultyTrapType, LlmWeakness, LlmWeaknessType,
    ResearchFindings,
};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the Research Agent.
#[derive(Debug, Clone)]
pub struct ResearchConfig {
    /// Temperature for LLM generation (0.9-1.2 for high creativity).
    pub temperature: f64,
    /// Nucleus sampling parameter.
    pub top_p: f64,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
    /// Whether to include detailed exploitation strategies.
    pub include_exploitation_details: bool,
}

impl Default for ResearchConfig {
    fn default() -> Self {
        Self {
            temperature: 1.0,
            top_p: 0.95,
            max_tokens: 4000,
            include_exploitation_details: true,
        }
    }
}

impl ResearchConfig {
    /// Creates a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the temperature (clamped to 0.7-1.2 for creative exploration).
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature.clamp(0.7, 1.2);
        self
    }

    /// Sets the top_p parameter.
    pub fn with_top_p(mut self, top_p: f64) -> Self {
        self.top_p = top_p.clamp(0.0, 1.0);
        self
    }

    /// Sets the maximum tokens for responses.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Sets whether to include exploitation details.
    pub fn with_exploitation_details(mut self, include: bool) -> Self {
        self.include_exploitation_details = include;
        self
    }
}

// ============================================================================
// LLM Response Types
// ============================================================================

/// Response structure for parsing LLM research output.
#[derive(Debug, Clone, Deserialize)]
struct LlmResearchResponse {
    category_insights: Vec<String>,
    identified_weaknesses: Vec<LlmWeaknessResponse>,
    proposed_traps: Vec<LlmTrapResponse>,
    difficulty_factors: Vec<LlmFactorResponse>,
}

#[derive(Debug, Clone, Deserialize)]
struct LlmWeaknessResponse {
    weakness_type: String,
    description: String,
    exploitation_strategy: String,
    severity: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct LlmTrapResponse {
    trap_type: String,
    description: String,
    implementation: String,
    detection_hint: String,
    difficulty_increase: f64,
    targets_weakness: String,
}

#[derive(Debug, Clone, Deserialize)]
struct LlmFactorResponse {
    name: String,
    description: String,
    weight: f64,
}

// ============================================================================
// Research Agent
// ============================================================================

/// Research Agent that identifies LLM weaknesses and proposes difficulty mechanisms.
///
/// This agent uses high-temperature LLM calls to creatively explore what makes
/// tasks genuinely difficult for AI systems while remaining solvable by humans.
pub struct ResearchAgent {
    /// LLM client for generation.
    llm_client: Arc<dyn LlmProvider>,
    /// Agent configuration.
    config: ResearchConfig,
}

impl std::fmt::Debug for ResearchAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResearchAgent")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl ResearchAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "research_agent";

    /// Creates a new Research Agent with the given LLM client and configuration.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: ResearchConfig) -> Self {
        Self { llm_client, config }
    }

    /// Creates a new Research Agent with default configuration.
    pub fn with_defaults(llm_client: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm_client, ResearchConfig::default())
    }

    /// Researches a category to identify LLM weaknesses and propose traps.
    ///
    /// # Arguments
    ///
    /// * `category` - The task category to research (e.g., "debugging", "security")
    ///
    /// # Returns
    ///
    /// `ResearchFindings` containing identified weaknesses and proposed traps.
    pub async fn research_category(&self, category: &str) -> AgentResult<ResearchFindings> {
        let prompt = build_research_prompt(category);

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(RESEARCH_AGENT_SYSTEM),
                Message::user(prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(self.config.max_tokens)
        .with_top_p(self.config.top_p);

        let response = self.llm_client.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_research_response(content, category)
    }

    /// Identifies specific weaknesses for a task type.
    ///
    /// # Arguments
    ///
    /// * `task_type` - The type of task to analyze (e.g., "log_analysis", "memory_debugging")
    ///
    /// # Returns
    ///
    /// A vector of `LlmWeakness` instances relevant to the task type.
    pub async fn identify_weaknesses(&self, task_type: &str) -> AgentResult<Vec<LlmWeakness>> {
        let prompt = format!(
            r#"Identify the specific LLM weaknesses that would be exposed by tasks of type "{}".

For each weakness, provide:
1. The weakness type (multi_step_reasoning, state_tracking, etc.)
2. How it manifests in this task type
3. How to exploit it in benchmark design
4. Severity rating (0.0-1.0)

Return as JSON array:
[
    {{
        "weakness_type": "type",
        "description": "how it manifests",
        "exploitation_strategy": "how to exploit",
        "severity": 0.0-1.0
    }}
]

Output ONLY the JSON array."#,
            task_type
        );

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(RESEARCH_AGENT_SYSTEM),
                Message::user(prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(self.config.max_tokens / 2);

        let response = self.llm_client.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_weaknesses_response(content)
    }

    /// Proposes specific traps based on research findings.
    ///
    /// # Arguments
    ///
    /// * `findings` - Research findings to base trap proposals on
    ///
    /// # Returns
    ///
    /// A vector of `DifficultyTrap` instances customized for the findings.
    pub async fn propose_traps(
        &self,
        findings: &ResearchFindings,
    ) -> AgentResult<Vec<DifficultyTrap>> {
        let weaknesses_summary: String = findings
            .identified_weaknesses
            .iter()
            .map(|w| format!("- {}: {}", w.weakness_type, w.description))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"Based on these identified weaknesses for the "{}" category:

{}

Propose 3-5 specific difficulty traps that would exploit these weaknesses.

For each trap, provide:
1. Trap type (data_corruption, state_dependent, timing, etc.)
2. Concrete description of what the trap does
3. Specific implementation details
4. How a careful solver can detect and avoid it
5. How much it increases difficulty (0.0-1.0)
6. Which weakness it targets

Return as JSON array:
[
    {{
        "trap_type": "type",
        "description": "what the trap does",
        "implementation": "how to implement it",
        "detection_hint": "how to detect/avoid",
        "difficulty_increase": 0.0-1.0,
        "targets_weakness": "weakness type"
    }}
]

Output ONLY the JSON array."#,
            findings.category, weaknesses_summary
        );

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(RESEARCH_AGENT_SYSTEM),
                Message::user(prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(self.config.max_tokens / 2);

        let response = self.llm_client.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_traps_response(content)
    }

    /// Parses the complete research response from the LLM.
    fn parse_research_response(
        &self,
        content: &str,
        category: &str,
    ) -> AgentResult<ResearchFindings> {
        let result = try_extract_json_from_response(content);
        let json_content = result.into_result_with_context(content).map_err(|e| {
            match &e {
                JsonExtractionError::Truncated { partial_preview, unclosed_braces, unclosed_brackets } => {
                    AgentError::ResponseParseError(format!(
                        "JSON appears truncated in research response: {} unclosed braces, {} unclosed brackets. Partial: {}...",
                        unclosed_braces, unclosed_brackets, partial_preview
                    ))
                }
                JsonExtractionError::NotFound { content_preview } => {
                    AgentError::ResponseParseError(format!(
                        "Could not extract JSON from research response. Content starts with: '{}'",
                        content_preview
                    ))
                }
            }
        })?;

        let llm_response: LlmResearchResponse =
            serde_json::from_str(&json_content).map_err(|e| {
                AgentError::ResponseParseError(format!(
                    "Failed to parse research response: {}. Content: {}",
                    e,
                    json_content.chars().take(500).collect::<String>()
                ))
            })?;

        let weaknesses = llm_response
            .identified_weaknesses
            .into_iter()
            .map(|w| {
                LlmWeakness::new(
                    parse_weakness_type(&w.weakness_type),
                    w.description,
                    w.exploitation_strategy,
                    w.severity,
                )
            })
            .collect();

        let traps = llm_response
            .proposed_traps
            .into_iter()
            .map(|t| {
                DifficultyTrap::new(
                    parse_trap_type(&t.trap_type),
                    t.description,
                    t.implementation,
                    t.detection_hint,
                    t.difficulty_increase,
                    parse_weakness_type(&t.targets_weakness),
                )
            })
            .collect();

        let factors = llm_response
            .difficulty_factors
            .into_iter()
            .map(|f| DifficultyFactor::new(f.name, f.description, f.weight))
            .collect();

        Ok(ResearchFindings::new(category)
            .with_insights(llm_response.category_insights)
            .with_weaknesses(weaknesses)
            .with_traps(traps)
            .with_factors(factors))
    }

    /// Parses a weaknesses-only response from the LLM.
    fn parse_weaknesses_response(&self, content: &str) -> AgentResult<Vec<LlmWeakness>> {
        let result = try_extract_json_from_response(content);
        let json_content = result.into_result_with_context(content).map_err(|e| {
            match &e {
                JsonExtractionError::Truncated { partial_preview, unclosed_braces, unclosed_brackets } => {
                    AgentError::ResponseParseError(format!(
                        "JSON appears truncated in weaknesses response: {} unclosed braces, {} unclosed brackets. Partial: {}...",
                        unclosed_braces, unclosed_brackets, partial_preview
                    ))
                }
                JsonExtractionError::NotFound { content_preview } => {
                    AgentError::ResponseParseError(format!(
                        "Could not extract JSON from weaknesses response. Content starts with: '{}'",
                        content_preview
                    ))
                }
            }
        })?;

        let llm_weaknesses: Vec<LlmWeaknessResponse> = serde_json::from_str(&json_content)
            .map_err(|e| {
                AgentError::ResponseParseError(format!(
                    "Failed to parse weaknesses response: {}",
                    e
                ))
            })?;

        Ok(llm_weaknesses
            .into_iter()
            .map(|w| {
                LlmWeakness::new(
                    parse_weakness_type(&w.weakness_type),
                    w.description,
                    w.exploitation_strategy,
                    w.severity,
                )
            })
            .collect())
    }

    /// Parses a traps-only response from the LLM.
    fn parse_traps_response(&self, content: &str) -> AgentResult<Vec<DifficultyTrap>> {
        let result = try_extract_json_from_response(content);
        let json_content = result.into_result_with_context(content).map_err(|e| {
            match &e {
                JsonExtractionError::Truncated { partial_preview, unclosed_braces, unclosed_brackets } => {
                    AgentError::ResponseParseError(format!(
                        "JSON appears truncated in traps response: {} unclosed braces, {} unclosed brackets. Partial: {}...",
                        unclosed_braces, unclosed_brackets, partial_preview
                    ))
                }
                JsonExtractionError::NotFound { content_preview } => {
                    AgentError::ResponseParseError(format!(
                        "Could not extract JSON from traps response. Content starts with: '{}'",
                        content_preview
                    ))
                }
            }
        })?;

        let llm_traps: Vec<LlmTrapResponse> = serde_json::from_str(&json_content).map_err(|e| {
            AgentError::ResponseParseError(format!("Failed to parse traps response: {}", e))
        })?;

        Ok(llm_traps
            .into_iter()
            .map(|t| {
                DifficultyTrap::new(
                    parse_trap_type(&t.trap_type),
                    t.description,
                    t.implementation,
                    t.detection_hint,
                    t.difficulty_increase,
                    parse_weakness_type(&t.targets_weakness),
                )
            })
            .collect())
    }

    /// Returns the agent configuration.
    pub fn config(&self) -> &ResearchConfig {
        &self.config
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parses a weakness type string into the enum.
fn parse_weakness_type(s: &str) -> LlmWeaknessType {
    match s.to_lowercase().replace('-', "_").as_str() {
        "multi_step_reasoning" | "multistep_reasoning" | "multistep" => {
            LlmWeaknessType::MultiStepReasoning
        }
        "state_tracking" | "statetracking" | "state" => LlmWeaknessType::StateTracking,
        "temporal_awareness" | "temporalawareness" | "temporal" | "timing" => {
            LlmWeaknessType::TemporalAwareness
        }
        "implicit_dependencies" | "implicitdependencies" | "implicit" => {
            LlmWeaknessType::ImplicitDependencies
        }
        "deceptive_patterns" | "deceptivepatterns" | "deceptive" => {
            LlmWeaknessType::DeceptivePatterns
        }
        "edge_cases" | "edgecases" | "edge" => LlmWeaknessType::EdgeCases,
        "resource_constraints" | "resourceconstraints" | "resource" => {
            LlmWeaknessType::ResourceConstraints
        }
        "concurrency" | "concurrent" => LlmWeaknessType::Concurrency,
        "domain_knowledge" | "domainknowledge" | "domain" => LlmWeaknessType::DomainKnowledge,
        "error_handling" | "errorhandling" | "error" => LlmWeaknessType::ErrorHandling,
        _ => LlmWeaknessType::MultiStepReasoning, // Default fallback
    }
}

/// Parses a trap type string into the enum.
fn parse_trap_type(s: &str) -> DifficultyTrapType {
    match s.to_lowercase().replace('-', "_").as_str() {
        "data_corruption" | "datacorruption" => DifficultyTrapType::DataCorruption,
        "state_dependent" | "statedependent" | "state" => DifficultyTrapType::StateDependent,
        "timing" | "race_condition" | "racecondition" => DifficultyTrapType::Timing,
        "deceptive_structure" | "deceptivestructure" | "deceptive" => {
            DifficultyTrapType::DeceptiveStructure
        }
        "resource_exhaustion" | "resourceexhaustion" | "resource" => {
            DifficultyTrapType::ResourceExhaustion
        }
        "self_modifying" | "selfmodifying" | "self" => DifficultyTrapType::SelfModifying,
        "hidden_configuration" | "hiddenconfiguration" | "hidden_config" => {
            DifficultyTrapType::HiddenConfiguration
        }
        "circular_dependency" | "circulardependency" | "circular" => {
            DifficultyTrapType::CircularDependency
        }
        "permission_trap" | "permissiontrap" | "permission" => DifficultyTrapType::PermissionTrap,
        "environment_sensitive" | "environmentsensitive" | "environment" => {
            DifficultyTrapType::EnvironmentSensitive
        }
        _ => DifficultyTrapType::StateDependent, // Default fallback
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{Choice, GenerationResponse, Usage};
    use crate::utils::json_extraction::extract_json_from_response;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// Mock LLM provider for testing.
    struct MockLlmProvider {
        response: Mutex<String>,
    }

    impl MockLlmProvider {
        fn new(response: impl Into<String>) -> Self {
            Self {
                response: Mutex::new(response.into()),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn generate(
            &self,
            _request: GenerationRequest,
        ) -> Result<GenerationResponse, crate::error::LlmError> {
            let content = self.response.lock().expect("lock not poisoned").clone();
            Ok(GenerationResponse {
                id: "mock-id".to_string(),
                model: "mock-model".to_string(),
                choices: vec![Choice {
                    index: 0,
                    message: Message::assistant(content),
                    finish_reason: "stop".to_string(),
                }],
                usage: Usage {
                    prompt_tokens: 100,
                    completion_tokens: 200,
                    total_tokens: 300,
                },
            })
        }
    }

    fn mock_research_response() -> String {
        r#"{
            "category_insights": [
                "Log analysis requires pattern recognition across large datasets",
                "Error correlation is challenging for LLMs"
            ],
            "identified_weaknesses": [
                {
                    "weakness_type": "multi_step_reasoning",
                    "description": "LLMs struggle to trace error propagation through multiple log files",
                    "exploitation_strategy": "Create tasks with cascading errors across services",
                    "severity": 0.85
                },
                {
                    "weakness_type": "state_tracking",
                    "description": "LLMs lose track of system state changes over time",
                    "exploitation_strategy": "Include log entries that modify global state",
                    "severity": 0.75
                }
            ],
            "proposed_traps": [
                {
                    "trap_type": "timing",
                    "description": "Logs with out-of-order timestamps",
                    "implementation": "Shuffle log entries so timestamps don't reflect actual event order",
                    "detection_hint": "Check for sequence numbers or correlation IDs",
                    "difficulty_increase": 0.2,
                    "targets_weakness": "temporal_awareness"
                },
                {
                    "trap_type": "deceptive_structure",
                    "description": "Multiple log files with similar names",
                    "implementation": "Create app.log, app.log.1, app_log etc.",
                    "detection_hint": "List all files and check modification times",
                    "difficulty_increase": 0.15,
                    "targets_weakness": "deceptive_patterns"
                }
            ],
            "difficulty_factors": [
                {
                    "name": "Log Volume",
                    "description": "Large number of log entries increases search time",
                    "weight": 0.3
                },
                {
                    "name": "Service Count",
                    "description": "More services means more correlation required",
                    "weight": 0.4
                }
            ]
        }"#
        .to_string()
    }

    fn mock_weaknesses_response() -> String {
        r#"[
            {
                "weakness_type": "edge_cases",
                "description": "Boundary condition handling in memory analysis",
                "exploitation_strategy": "Include allocations at page boundaries",
                "severity": 0.7
            }
        ]"#
        .to_string()
    }

    fn mock_traps_response() -> String {
        r#"[
            {
                "trap_type": "resource_exhaustion",
                "description": "Memory-mapped file that grows when read",
                "implementation": "Use named pipe that produces infinite output",
                "detection_hint": "Check file type before reading",
                "difficulty_increase": 0.25,
                "targets_weakness": "resource_constraints"
            }
        ]"#
        .to_string()
    }

    #[test]
    fn test_config_defaults() {
        let config = ResearchConfig::default();
        assert!((config.temperature - 1.0).abs() < 0.01);
        assert!((config.top_p - 0.95).abs() < 0.01);
        assert_eq!(config.max_tokens, 4000);
        assert!(config.include_exploitation_details);
    }

    #[test]
    fn test_config_builder() {
        let config = ResearchConfig::new()
            .with_temperature(0.9)
            .with_top_p(0.8)
            .with_max_tokens(3000)
            .with_exploitation_details(false);

        assert!((config.temperature - 0.9).abs() < 0.01);
        assert!((config.top_p - 0.8).abs() < 0.01);
        assert_eq!(config.max_tokens, 3000);
        assert!(!config.include_exploitation_details);
    }

    #[test]
    fn test_config_temperature_clamping() {
        let config = ResearchConfig::new().with_temperature(2.0);
        assert!((config.temperature - 1.2).abs() < 0.01);

        let config = ResearchConfig::new().with_temperature(0.3);
        assert!((config.temperature - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_parse_weakness_type() {
        assert_eq!(
            parse_weakness_type("multi_step_reasoning"),
            LlmWeaknessType::MultiStepReasoning
        );
        assert_eq!(
            parse_weakness_type("state_tracking"),
            LlmWeaknessType::StateTracking
        );
        assert_eq!(
            parse_weakness_type("temporal"),
            LlmWeaknessType::TemporalAwareness
        );
        assert_eq!(
            parse_weakness_type("concurrency"),
            LlmWeaknessType::Concurrency
        );
        assert_eq!(
            parse_weakness_type("unknown_type"),
            LlmWeaknessType::MultiStepReasoning
        );
    }

    #[test]
    fn test_parse_trap_type() {
        assert_eq!(
            parse_trap_type("data_corruption"),
            DifficultyTrapType::DataCorruption
        );
        assert_eq!(parse_trap_type("timing"), DifficultyTrapType::Timing);
        assert_eq!(
            parse_trap_type("deceptive_structure"),
            DifficultyTrapType::DeceptiveStructure
        );
        assert_eq!(
            parse_trap_type("permission_trap"),
            DifficultyTrapType::PermissionTrap
        );
        assert_eq!(
            parse_trap_type("unknown_trap"),
            DifficultyTrapType::StateDependent
        );
    }

    #[test]
    fn test_extract_json_from_response_raw() {
        let raw = r#"{"key": "value"}"#;
        assert_eq!(extract_json_from_response(raw), raw);
    }

    #[test]
    fn test_extract_json_from_response_markdown() {
        let markdown = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json_from_response(markdown), r#"{"key": "value"}"#);
    }

    #[test]
    fn test_extract_json_from_response_array() {
        let array = "[1, 2, 3]";
        assert_eq!(extract_json_from_response(array), array);
    }

    #[tokio::test]
    async fn test_research_agent_creation() {
        let mock_llm = Arc::new(MockLlmProvider::new("{}"));
        let agent = ResearchAgent::with_defaults(mock_llm);

        assert_eq!(ResearchAgent::AGENT_NAME, "research_agent");
        assert!((agent.config().temperature - 1.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_research_category() {
        let mock_llm = Arc::new(MockLlmProvider::new(mock_research_response()));
        let agent = ResearchAgent::with_defaults(mock_llm);

        let findings = agent
            .research_category("debugging")
            .await
            .expect("should parse research response");

        assert_eq!(findings.category, "debugging");
        assert_eq!(findings.category_insights.len(), 2);
        assert_eq!(findings.identified_weaknesses.len(), 2);
        assert_eq!(findings.proposed_traps.len(), 2);
        assert_eq!(findings.difficulty_factors.len(), 2);
    }

    #[tokio::test]
    async fn test_identify_weaknesses() {
        let mock_llm = Arc::new(MockLlmProvider::new(mock_weaknesses_response()));
        let agent = ResearchAgent::with_defaults(mock_llm);

        let weaknesses = agent
            .identify_weaknesses("memory_debugging")
            .await
            .expect("should parse weaknesses response");

        assert_eq!(weaknesses.len(), 1);
        assert_eq!(weaknesses[0].weakness_type, LlmWeaknessType::EdgeCases);
    }

    #[tokio::test]
    async fn test_propose_traps() {
        let mock_llm = Arc::new(MockLlmProvider::new(mock_traps_response()));
        let agent = ResearchAgent::with_defaults(mock_llm);

        let findings = ResearchFindings::new("debugging").with_weaknesses(vec![LlmWeakness::new(
            LlmWeaknessType::ResourceConstraints,
            "Memory handling issues",
            "Exploit via large allocations",
            0.8,
        )]);

        let traps = agent
            .propose_traps(&findings)
            .await
            .expect("should parse traps response");

        assert_eq!(traps.len(), 1);
        assert_eq!(traps[0].trap_type, DifficultyTrapType::ResourceExhaustion);
    }

    #[tokio::test]
    async fn test_research_findings_content() {
        let mock_llm = Arc::new(MockLlmProvider::new(mock_research_response()));
        let agent = ResearchAgent::with_defaults(mock_llm);

        let findings = agent
            .research_category("debugging")
            .await
            .expect("should parse");

        // Check weakness details
        let weakness = &findings.identified_weaknesses[0];
        assert_eq!(weakness.weakness_type, LlmWeaknessType::MultiStepReasoning);
        assert!((weakness.severity - 0.85).abs() < 0.01);

        // Check trap details
        let trap = &findings.proposed_traps[0];
        assert_eq!(trap.trap_type, DifficultyTrapType::Timing);
        assert!((trap.difficulty_increase - 0.2).abs() < 0.01);

        // Check factor details
        let factor = &findings.difficulty_factors[0];
        assert_eq!(factor.name, "Log Volume");
        assert!((factor.weight - 0.3).abs() < 0.01);
    }
}
