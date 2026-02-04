//! Difficulty Amplifier Agent - Makes tasks harder with strategic traps.
//!
//! This agent takes a base task and amplifies its difficulty by adding
//! strategic traps that would trip up LLMs while keeping tasks solvable
//! for expert humans.
//!
//! The Difficulty Amplifier:
//! - Takes a base task and amplifies difficulty
//! - Adds data corruption scenarios
//! - Inserts state-dependent behaviors
//! - Creates deceptive file structures
//! - Adds timing-sensitive operations
//! - Ensures tasks remain solvable but genuinely challenging
//!
//! # Example
//!
//! ```ignore
//! use dataforge::agents::difficulty_amplifier::{DifficultyAmplifierAgent, AmplifierConfig};
//! use dataforge::agents::factory_types::{FactoryTaskSpec, DifficultyTrap};
//! use dataforge::llm::LiteLlmClient;
//! use std::sync::Arc;
//!
//! let llm_client = Arc::new(LiteLlmClient::from_env()?);
//! let config = AmplifierConfig::default();
//! let agent = DifficultyAmplifierAgent::new(llm_client, config);
//!
//! let task = FactoryTaskSpec::new("Task Title", "debugging", "description", DifficultyLevel::Medium);
//! let traps = vec![/* traps from research */];
//!
//! let amplified = agent.amplify_task(&task, &traps).await?;
//! println!("Difficulty increased to {:.2}", amplified.difficulty_score);
//! ```

use std::sync::Arc;

use serde::Deserialize;

use crate::llm::{GenerationRequest, LlmProvider, Message};
use crate::prompts::{build_amplifier_prompt, AMPLIFIER_AGENT_SYSTEM};

use super::error::{AgentError, AgentResult};
use super::factory_types::{
    AmplifiedTask, DifficultyTrap, DifficultyTrapType, FactoryTaskSpec, LlmWeaknessType,
};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the Difficulty Amplifier Agent.
#[derive(Debug, Clone)]
pub struct AmplifierConfig {
    /// Temperature for LLM generation (moderate for structured output).
    pub temperature: f64,
    /// Nucleus sampling parameter.
    pub top_p: f64,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
    /// Minimum number of traps to add.
    pub min_traps: usize,
    /// Maximum number of traps to add.
    pub max_traps: usize,
    /// Maximum difficulty score after amplification.
    pub max_difficulty_score: f64,
}

impl Default for AmplifierConfig {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: 0.9,
            max_tokens: 3000,
            min_traps: 2,
            max_traps: 4,
            max_difficulty_score: 0.95,
        }
    }
}

impl AmplifierConfig {
    /// Creates a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the temperature.
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature.clamp(0.3, 1.0);
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

    /// Sets the minimum number of traps.
    pub fn with_min_traps(mut self, min_traps: usize) -> Self {
        self.min_traps = min_traps;
        self
    }

    /// Sets the maximum number of traps.
    pub fn with_max_traps(mut self, max_traps: usize) -> Self {
        self.max_traps = max_traps;
        self
    }

    /// Sets the maximum difficulty score.
    pub fn with_max_difficulty_score(mut self, score: f64) -> Self {
        self.max_difficulty_score = score.clamp(0.5, 1.0);
        self
    }
}

// ============================================================================
// LLM Response Types
// ============================================================================

/// Response structure for parsing LLM amplification output.
#[derive(Debug, Clone, Deserialize)]
struct LlmAmplificationResponse {
    traps_added: Vec<LlmTrapResponse>,
    expected_failure_points: Vec<String>,
    #[allow(dead_code)]
    difficulty_score: f64,
    amplification_notes: String,
}

#[derive(Debug, Clone, Deserialize)]
struct LlmTrapResponse {
    id: String,
    trap_type: String,
    description: String,
    implementation: String,
    detection_hint: String,
    difficulty_increase: f64,
    targets_weakness: String,
}

// ============================================================================
// Difficulty Amplifier Agent
// ============================================================================

/// Difficulty Amplifier Agent that makes benchmark tasks harder with strategic traps.
///
/// This agent takes base tasks and adds challenging elements that would trip up LLMs
/// while ensuring tasks remain solvable by careful human experts.
pub struct DifficultyAmplifierAgent {
    /// LLM client for generation.
    llm_client: Arc<dyn LlmProvider>,
    /// Agent configuration.
    config: AmplifierConfig,
}

impl std::fmt::Debug for DifficultyAmplifierAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DifficultyAmplifierAgent")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl DifficultyAmplifierAgent {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "difficulty_amplifier";

    /// Creates a new Difficulty Amplifier Agent with the given LLM client and configuration.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: AmplifierConfig) -> Self {
        Self { llm_client, config }
    }

    /// Creates a new Difficulty Amplifier Agent with default configuration.
    pub fn with_defaults(llm_client: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm_client, AmplifierConfig::default())
    }

    /// Amplifies the difficulty of a task by adding traps.
    ///
    /// # Arguments
    ///
    /// * `task` - The base task specification to amplify
    /// * `traps` - Available traps from research to choose from
    ///
    /// # Returns
    ///
    /// An `AmplifiedTask` with added traps and increased difficulty.
    pub async fn amplify_task(
        &self,
        task: &FactoryTaskSpec,
        traps: &[DifficultyTrap],
    ) -> AgentResult<AmplifiedTask> {
        // Format the research traps for the prompt
        let traps_summary = self.format_traps_for_prompt(traps);
        let weaknesses_summary = self.format_weaknesses_for_prompt(&task.targeted_weaknesses);

        let prompt = build_amplifier_prompt(
            &task.title,
            &task.category,
            &task.description,
            task.difficulty_score,
            &task.required_skills,
            &weaknesses_summary,
            &traps_summary,
        );

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(AMPLIFIER_AGENT_SYSTEM),
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

        self.parse_amplification_response(content, task)
    }

    /// Adds a data corruption trap to a task.
    ///
    /// # Arguments
    ///
    /// * `task` - The task specification to modify
    ///
    /// # Returns
    ///
    /// A `DifficultyTrap` representing the data corruption scenario.
    pub async fn add_data_corruption_trap(
        &self,
        task: &FactoryTaskSpec,
    ) -> AgentResult<DifficultyTrap> {
        let prompt = format!(
            r#"Design a data corruption trap for this task:

Task: {}
Category: {}
Description: {}

Create a data corruption scenario that:
1. Causes silent data corruption when files are accessed incorrectly
2. Is detectable by checking file properties before access
3. Has a clear workaround for careful solvers

Return as JSON:
{{
    "id": "unique-id",
    "trap_type": "data_corruption",
    "description": "what happens",
    "implementation": "how to set it up",
    "detection_hint": "how to detect/avoid",
    "difficulty_increase": 0.0-1.0,
    "targets_weakness": "which weakness"
}}

Output ONLY the JSON object."#,
            task.title, task.category, task.description
        );

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(AMPLIFIER_AGENT_SYSTEM),
                Message::user(prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(1000);

        let response = self.llm_client.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_single_trap_response(content)
    }

    /// Adds a state-dependent trap to a task.
    ///
    /// # Arguments
    ///
    /// * `task` - The task specification to modify
    ///
    /// # Returns
    ///
    /// A `DifficultyTrap` representing the state-dependent behavior.
    pub async fn add_state_trap(&self, task: &FactoryTaskSpec) -> AgentResult<DifficultyTrap> {
        let prompt = format!(
            r#"Design a state-dependent trap for this task:

Task: {}
Category: {}
Description: {}

Create a hidden state scenario that:
1. Causes behavior to change based on non-obvious system state
2. Can be discovered by checking environment/configuration
3. Has predictable behavior once the state is understood

Return as JSON:
{{
    "id": "unique-id",
    "trap_type": "state_dependent",
    "description": "what happens",
    "implementation": "how to set it up",
    "detection_hint": "how to detect/avoid",
    "difficulty_increase": 0.0-1.0,
    "targets_weakness": "which weakness"
}}

Output ONLY the JSON object."#,
            task.title, task.category, task.description
        );

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(AMPLIFIER_AGENT_SYSTEM),
                Message::user(prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(1000);

        let response = self.llm_client.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_single_trap_response(content)
    }

    /// Adds a timing trap to a task.
    ///
    /// # Arguments
    ///
    /// * `task` - The task specification to modify
    ///
    /// # Returns
    ///
    /// A `DifficultyTrap` representing the timing-sensitive operation.
    pub async fn add_timing_trap(&self, task: &FactoryTaskSpec) -> AgentResult<DifficultyTrap> {
        let prompt = format!(
            r#"Design a timing trap for this task:

Task: {}
Category: {}
Description: {}

Create a timing-sensitive scenario that:
1. Causes race conditions or order-dependent behavior
2. Can be addressed with proper synchronization
3. Has clear symptoms when triggered

Return as JSON:
{{
    "id": "unique-id",
    "trap_type": "timing",
    "description": "what happens",
    "implementation": "how to set it up",
    "detection_hint": "how to detect/avoid",
    "difficulty_increase": 0.0-1.0,
    "targets_weakness": "which weakness"
}}

Output ONLY the JSON object."#,
            task.title, task.category, task.description
        );

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(AMPLIFIER_AGENT_SYSTEM),
                Message::user(prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(1000);

        let response = self.llm_client.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_single_trap_response(content)
    }

    /// Adds a deceptive structure trap to a task.
    ///
    /// # Arguments
    ///
    /// * `task` - The task specification to modify
    ///
    /// # Returns
    ///
    /// A `DifficultyTrap` representing the deceptive file/directory structure.
    pub async fn add_deceptive_structure(
        &self,
        task: &FactoryTaskSpec,
    ) -> AgentResult<DifficultyTrap> {
        let prompt = format!(
            r#"Design a deceptive structure trap for this task:

Task: {}
Category: {}
Description: {}

Create a deceptive file/directory scenario that:
1. Uses symlinks, unicode, or naming tricks to mislead
2. Can be detected by careful file inspection
3. Has legitimate files mixed with decoys

Return as JSON:
{{
    "id": "unique-id",
    "trap_type": "deceptive_structure",
    "description": "what happens",
    "implementation": "how to set it up",
    "detection_hint": "how to detect/avoid",
    "difficulty_increase": 0.0-1.0,
    "targets_weakness": "which weakness"
}}

Output ONLY the JSON object."#,
            task.title, task.category, task.description
        );

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(AMPLIFIER_AGENT_SYSTEM),
                Message::user(prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(1000);

        let response = self.llm_client.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty LLM response".to_string()))?;

        self.parse_single_trap_response(content)
    }

    /// Formats traps for inclusion in the prompt.
    fn format_traps_for_prompt(&self, traps: &[DifficultyTrap]) -> String {
        if traps.is_empty() {
            return "No specific traps provided - propose your own based on category".to_string();
        }

        traps
            .iter()
            .map(|t| {
                format!(
                    "- {} ({}): {} [+{:.2} difficulty]",
                    t.trap_type, t.targets_weakness, t.description, t.difficulty_increase
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Formats weaknesses for inclusion in the prompt.
    fn format_weaknesses_for_prompt(&self, weaknesses: &[LlmWeaknessType]) -> String {
        if weaknesses.is_empty() {
            return "No specific weaknesses targeted".to_string();
        }

        weaknesses
            .iter()
            .map(|w| format!("- {}", w))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Parses the complete amplification response from the LLM.
    fn parse_amplification_response(
        &self,
        content: &str,
        task: &FactoryTaskSpec,
    ) -> AgentResult<AmplifiedTask> {
        let json_content = extract_json_from_response(content);

        let llm_response: LlmAmplificationResponse =
            serde_json::from_str(&json_content).map_err(|e| {
                AgentError::ResponseParseError(format!(
                    "Failed to parse amplification response: {}. Content: {}",
                    e,
                    &json_content[..json_content.len().min(500)]
                ))
            })?;

        let traps: Vec<DifficultyTrap> = llm_response
            .traps_added
            .into_iter()
            .map(|t| {
                DifficultyTrap::with_id(
                    t.id,
                    parse_trap_type(&t.trap_type),
                    t.description,
                    t.implementation,
                    t.detection_hint,
                    t.difficulty_increase,
                    parse_weakness_type(&t.targets_weakness),
                )
            })
            .collect();

        // Calculate final difficulty score
        let trap_increase: f64 = traps.iter().map(|t| t.difficulty_increase).sum();
        let final_score =
            (task.difficulty_score + trap_increase).min(self.config.max_difficulty_score);

        Ok(AmplifiedTask {
            original_spec: task.clone(),
            traps_added: traps,
            expected_failure_points: llm_response.expected_failure_points,
            difficulty_score: final_score,
            amplification_notes: llm_response.amplification_notes,
        })
    }

    /// Parses a single trap response from the LLM.
    fn parse_single_trap_response(&self, content: &str) -> AgentResult<DifficultyTrap> {
        let json_content = extract_json_from_response(content);

        let llm_trap: LlmTrapResponse = serde_json::from_str(&json_content).map_err(|e| {
            AgentError::ResponseParseError(format!("Failed to parse trap response: {}", e))
        })?;

        Ok(DifficultyTrap::with_id(
            llm_trap.id,
            parse_trap_type(&llm_trap.trap_type),
            llm_trap.description,
            llm_trap.implementation,
            llm_trap.detection_hint,
            llm_trap.difficulty_increase,
            parse_weakness_type(&llm_trap.targets_weakness),
        ))
    }

    /// Returns the agent configuration.
    pub fn config(&self) -> &AmplifierConfig {
        &self.config
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Extracts JSON content from an LLM response that might be wrapped in markdown.
fn extract_json_from_response(content: &str) -> String {
    let trimmed = content.trim();

    // Try to find JSON block in markdown code fence
    if let Some(start) = trimmed.find("```json") {
        let after_fence = &trimmed[start + 7..];
        if let Some(end) = after_fence.find("```") {
            return after_fence[..end].trim().to_string();
        }
    }

    // Try generic code fence
    if let Some(start) = trimmed.find("```") {
        let after_fence = &trimmed[start + 3..];
        let content_start = after_fence.find('\n').map(|i| i + 1).unwrap_or(0);
        let after_newline = &after_fence[content_start..];
        if let Some(end) = after_newline.find("```") {
            return after_newline[..end].trim().to_string();
        }
    }

    // Try to find raw JSON object
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                return trimmed[start..=end].to_string();
            }
        }
    }

    trimmed.to_string()
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::difficulty::DifficultyLevel;
    use crate::llm::{Choice, GenerationResponse, Usage};
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

    fn mock_amplification_response() -> String {
        r#"{
            "traps_added": [
                {
                    "id": "trap-1",
                    "trap_type": "timing",
                    "description": "Log rotation during analysis",
                    "implementation": "Schedule logrotate to run mid-task",
                    "detection_hint": "Check for lsof or fuser on log files",
                    "difficulty_increase": 0.2,
                    "targets_weakness": "temporal_awareness"
                },
                {
                    "id": "trap-2",
                    "trap_type": "deceptive_structure",
                    "description": "Symlinks to wrong log files",
                    "implementation": "Create latest.log -> old_backup.log",
                    "detection_hint": "Use ls -la to check link targets",
                    "difficulty_increase": 0.15,
                    "targets_weakness": "deceptive_patterns"
                }
            ],
            "expected_failure_points": [
                "Reading logs after rotation without reconnecting",
                "Following symlinks without verification"
            ],
            "difficulty_score": 0.85,
            "amplification_notes": "These traps target common LLM assumptions about file system state"
        }"#
        .to_string()
    }

    fn mock_single_trap_response() -> String {
        r#"{
            "id": "single-trap-1",
            "trap_type": "data_corruption",
            "description": "Binary file with text extension",
            "implementation": "Create config.txt that is actually gzipped",
            "detection_hint": "Check file magic bytes before reading",
            "difficulty_increase": 0.2,
            "targets_weakness": "implicit_dependencies"
        }"#
        .to_string()
    }

    fn create_test_task() -> FactoryTaskSpec {
        FactoryTaskSpec::new(
            "Debug Memory Leak",
            "debugging",
            "Find and fix a memory leak in the application",
            DifficultyLevel::Medium,
        )
        .with_difficulty_score(0.5)
        .with_required_skills(["rust", "profiling"])
        .with_targeted_weaknesses(vec![LlmWeaknessType::StateTracking])
    }

    #[test]
    fn test_config_defaults() {
        let config = AmplifierConfig::default();
        assert!((config.temperature - 0.7).abs() < 0.01);
        assert!((config.top_p - 0.9).abs() < 0.01);
        assert_eq!(config.max_tokens, 3000);
        assert_eq!(config.min_traps, 2);
        assert_eq!(config.max_traps, 4);
    }

    #[test]
    fn test_config_builder() {
        let config = AmplifierConfig::new()
            .with_temperature(0.8)
            .with_top_p(0.85)
            .with_max_tokens(2000)
            .with_min_traps(1)
            .with_max_traps(5)
            .with_max_difficulty_score(0.9);

        assert!((config.temperature - 0.8).abs() < 0.01);
        assert!((config.top_p - 0.85).abs() < 0.01);
        assert_eq!(config.max_tokens, 2000);
        assert_eq!(config.min_traps, 1);
        assert_eq!(config.max_traps, 5);
        assert!((config.max_difficulty_score - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_config_temperature_clamping() {
        let config = AmplifierConfig::new().with_temperature(2.0);
        assert!((config.temperature - 1.0).abs() < 0.01);

        let config = AmplifierConfig::new().with_temperature(0.1);
        assert!((config.temperature - 0.3).abs() < 0.01);
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
            parse_trap_type("unknown"),
            DifficultyTrapType::StateDependent
        );
    }

    #[test]
    fn test_parse_weakness_type() {
        assert_eq!(
            parse_weakness_type("multi_step_reasoning"),
            LlmWeaknessType::MultiStepReasoning
        );
        assert_eq!(
            parse_weakness_type("temporal_awareness"),
            LlmWeaknessType::TemporalAwareness
        );
        assert_eq!(
            parse_weakness_type("unknown"),
            LlmWeaknessType::MultiStepReasoning
        );
    }

    #[test]
    fn test_extract_json_from_response() {
        let raw = r#"{"key": "value"}"#;
        assert_eq!(extract_json_from_response(raw), raw);

        let markdown = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json_from_response(markdown), r#"{"key": "value"}"#);
    }

    #[tokio::test]
    async fn test_amplifier_agent_creation() {
        let mock_llm = Arc::new(MockLlmProvider::new("{}"));
        let agent = DifficultyAmplifierAgent::with_defaults(mock_llm);

        assert_eq!(DifficultyAmplifierAgent::AGENT_NAME, "difficulty_amplifier");
        assert!((agent.config().temperature - 0.7).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_amplify_task() {
        let mock_llm = Arc::new(MockLlmProvider::new(mock_amplification_response()));
        let agent = DifficultyAmplifierAgent::with_defaults(mock_llm);

        let task = create_test_task();
        let traps = vec![DifficultyTrap::new(
            DifficultyTrapType::Timing,
            "test trap",
            "impl",
            "hint",
            0.2,
            LlmWeaknessType::TemporalAwareness,
        )];

        let amplified = agent
            .amplify_task(&task, &traps)
            .await
            .expect("should amplify task");

        assert_eq!(amplified.traps_added.len(), 2);
        assert_eq!(amplified.expected_failure_points.len(), 2);
        // Score should be clamped by config.max_difficulty_score
        assert!(amplified.difficulty_score <= 0.95);
    }

    #[tokio::test]
    async fn test_add_data_corruption_trap() {
        let mock_llm = Arc::new(MockLlmProvider::new(mock_single_trap_response()));
        let agent = DifficultyAmplifierAgent::with_defaults(mock_llm);

        let task = create_test_task();
        let trap = agent
            .add_data_corruption_trap(&task)
            .await
            .expect("should create trap");

        assert_eq!(trap.trap_type, DifficultyTrapType::DataCorruption);
        assert_eq!(trap.id, "single-trap-1");
    }

    #[tokio::test]
    async fn test_add_state_trap() {
        let mock_response = r#"{
            "id": "state-trap-1",
            "trap_type": "state_dependent",
            "description": "Environment variable changes behavior",
            "implementation": "Set DEBUG=1 to alter output format",
            "detection_hint": "Check env vars before running",
            "difficulty_increase": 0.18,
            "targets_weakness": "state_tracking"
        }"#;

        let mock_llm = Arc::new(MockLlmProvider::new(mock_response));
        let agent = DifficultyAmplifierAgent::with_defaults(mock_llm);

        let task = create_test_task();
        let trap = agent
            .add_state_trap(&task)
            .await
            .expect("should create trap");

        assert_eq!(trap.trap_type, DifficultyTrapType::StateDependent);
    }

    #[tokio::test]
    async fn test_add_timing_trap() {
        let mock_response = r#"{
            "id": "timing-trap-1",
            "trap_type": "timing",
            "description": "Race condition in file access",
            "implementation": "Multiple processes write to same file",
            "detection_hint": "Use file locking",
            "difficulty_increase": 0.22,
            "targets_weakness": "temporal_awareness"
        }"#;

        let mock_llm = Arc::new(MockLlmProvider::new(mock_response));
        let agent = DifficultyAmplifierAgent::with_defaults(mock_llm);

        let task = create_test_task();
        let trap = agent
            .add_timing_trap(&task)
            .await
            .expect("should create trap");

        assert_eq!(trap.trap_type, DifficultyTrapType::Timing);
    }

    #[tokio::test]
    async fn test_add_deceptive_structure() {
        let mock_response = r#"{
            "id": "deceptive-trap-1",
            "trap_type": "deceptive_structure",
            "description": "Hidden directory with important files",
            "implementation": "Put config in .hidden/real_config",
            "detection_hint": "Use ls -la to see hidden files",
            "difficulty_increase": 0.15,
            "targets_weakness": "deceptive_patterns"
        }"#;

        let mock_llm = Arc::new(MockLlmProvider::new(mock_response));
        let agent = DifficultyAmplifierAgent::with_defaults(mock_llm);

        let task = create_test_task();
        let trap = agent
            .add_deceptive_structure(&task)
            .await
            .expect("should create trap");

        assert_eq!(trap.trap_type, DifficultyTrapType::DeceptiveStructure);
    }

    #[test]
    fn test_format_traps_for_prompt() {
        let mock_llm = Arc::new(MockLlmProvider::new("{}"));
        let agent = DifficultyAmplifierAgent::with_defaults(mock_llm);

        let traps = vec![DifficultyTrap::new(
            DifficultyTrapType::Timing,
            "Race condition",
            "implementation",
            "hint",
            0.2,
            LlmWeaknessType::TemporalAwareness,
        )];

        let formatted = agent.format_traps_for_prompt(&traps);
        assert!(formatted.contains("Timing"));
        assert!(formatted.contains("Race condition"));
        assert!(formatted.contains("+0.20"));
    }

    #[test]
    fn test_format_traps_empty() {
        let mock_llm = Arc::new(MockLlmProvider::new("{}"));
        let agent = DifficultyAmplifierAgent::with_defaults(mock_llm);

        let formatted = agent.format_traps_for_prompt(&[]);
        assert!(formatted.contains("No specific traps provided"));
    }

    #[test]
    fn test_format_weaknesses_for_prompt() {
        let mock_llm = Arc::new(MockLlmProvider::new("{}"));
        let agent = DifficultyAmplifierAgent::with_defaults(mock_llm);

        let weaknesses = vec![
            LlmWeaknessType::StateTracking,
            LlmWeaknessType::TemporalAwareness,
        ];

        let formatted = agent.format_weaknesses_for_prompt(&weaknesses);
        assert!(formatted.contains("State Tracking"));
        assert!(formatted.contains("Temporal Awareness"));
    }

    #[tokio::test]
    async fn test_amplified_task_difficulty_score_capped() {
        // Create a response that would push score over the cap
        let response = r#"{
            "traps_added": [
                {
                    "id": "trap-1",
                    "trap_type": "timing",
                    "description": "trap 1",
                    "implementation": "impl",
                    "detection_hint": "hint",
                    "difficulty_increase": 0.3,
                    "targets_weakness": "temporal_awareness"
                },
                {
                    "id": "trap-2",
                    "trap_type": "state_dependent",
                    "description": "trap 2",
                    "implementation": "impl",
                    "detection_hint": "hint",
                    "difficulty_increase": 0.3,
                    "targets_weakness": "state_tracking"
                }
            ],
            "expected_failure_points": ["point 1"],
            "difficulty_score": 1.0,
            "amplification_notes": "test"
        }"#;

        let mock_llm = Arc::new(MockLlmProvider::new(response));
        let config = AmplifierConfig::default().with_max_difficulty_score(0.9);
        let agent = DifficultyAmplifierAgent::new(mock_llm, config);

        let task = create_test_task(); // difficulty_score = 0.5
        let amplified = agent
            .amplify_task(&task, &[])
            .await
            .expect("should amplify task");

        // 0.5 + 0.3 + 0.3 = 1.1, should be capped at 0.9
        assert!(amplified.difficulty_score <= 0.9);
    }
}
