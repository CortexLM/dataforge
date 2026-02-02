//! Types for the factory multi-agent orchestration system.
//!
//! This module defines the core types used throughout the factory orchestration
//! pipeline for generating challenging synthetic benchmark tasks that target
//! specific LLM weaknesses.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::difficulty::DifficultyLevel;

// ============================================================================
// Pipeline Stages
// ============================================================================

/// Stages in the factory task generation pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FactoryPipelineStage {
    /// Research stage - identifying LLM weaknesses.
    Research,
    /// Creation stage - generating initial task specification.
    Creation,
    /// Amplification stage - adding difficulty traps.
    Amplification,
    /// Validation stage - validating task quality.
    Validation,
    /// Finalization stage - completing task specification.
    Finalization,
}

impl std::fmt::Display for FactoryPipelineStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FactoryPipelineStage::Research => write!(f, "Research"),
            FactoryPipelineStage::Creation => write!(f, "Creation"),
            FactoryPipelineStage::Amplification => write!(f, "Amplification"),
            FactoryPipelineStage::Validation => write!(f, "Validation"),
            FactoryPipelineStage::Finalization => write!(f, "Finalization"),
        }
    }
}

// ============================================================================
// Pipeline Events
// ============================================================================

/// Events emitted during the factory pipeline for TUI progress updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FactoryPipelineEvent {
    /// Pipeline stage has started.
    StageStarted {
        /// The stage that started.
        stage: FactoryPipelineStage,
        /// Timestamp when the stage started.
        timestamp: DateTime<Utc>,
    },

    /// Research stage completed with findings.
    ResearchComplete {
        /// Number of weaknesses identified.
        weaknesses_found: usize,
        /// Number of traps proposed.
        traps_proposed: usize,
        /// Timestamp when research completed.
        timestamp: DateTime<Utc>,
    },

    /// Task creation stage completed.
    CreationComplete {
        /// Title of the created task.
        task_title: String,
        /// Category of the task.
        category: String,
        /// Timestamp when creation completed.
        timestamp: DateTime<Utc>,
    },

    /// Amplification stage completed with traps added.
    AmplificationComplete {
        /// Number of traps added to the task.
        traps_added: usize,
        /// Final difficulty score.
        difficulty_score: f64,
        /// Timestamp when amplification completed.
        timestamp: DateTime<Utc>,
    },

    /// Validation stage completed.
    ValidationComplete {
        /// Whether validation passed.
        passed: bool,
        /// Validation score.
        score: f64,
        /// Timestamp when validation completed.
        timestamp: DateTime<Utc>,
    },

    /// An agent conversation turn occurred.
    AgentConversation {
        /// Name of the agent that spoke.
        agent_name: String,
        /// Summary of the message.
        message_summary: String,
        /// Timestamp of the message.
        timestamp: DateTime<Utc>,
    },

    /// Pipeline completed successfully.
    PipelineComplete {
        /// Total number of tasks generated.
        tasks_generated: usize,
        /// Total duration in milliseconds.
        total_duration_ms: u64,
        /// Timestamp when pipeline completed.
        timestamp: DateTime<Utc>,
    },

    /// Pipeline failed with an error.
    PipelineFailed {
        /// Error description.
        error: String,
        /// Stage where failure occurred.
        stage: FactoryPipelineStage,
        /// Timestamp when failure occurred.
        timestamp: DateTime<Utc>,
    },
}

impl FactoryPipelineEvent {
    /// Creates a StageStarted event.
    pub fn stage_started(stage: FactoryPipelineStage) -> Self {
        Self::StageStarted {
            stage,
            timestamp: Utc::now(),
        }
    }

    /// Creates a ResearchComplete event.
    pub fn research_complete(weaknesses_found: usize, traps_proposed: usize) -> Self {
        Self::ResearchComplete {
            weaknesses_found,
            traps_proposed,
            timestamp: Utc::now(),
        }
    }

    /// Creates a CreationComplete event.
    pub fn creation_complete(task_title: impl Into<String>, category: impl Into<String>) -> Self {
        Self::CreationComplete {
            task_title: task_title.into(),
            category: category.into(),
            timestamp: Utc::now(),
        }
    }

    /// Creates an AmplificationComplete event.
    pub fn amplification_complete(traps_added: usize, difficulty_score: f64) -> Self {
        Self::AmplificationComplete {
            traps_added,
            difficulty_score,
            timestamp: Utc::now(),
        }
    }

    /// Creates a ValidationComplete event.
    pub fn validation_complete(passed: bool, score: f64) -> Self {
        Self::ValidationComplete {
            passed,
            score,
            timestamp: Utc::now(),
        }
    }

    /// Creates an AgentConversation event.
    pub fn agent_conversation(
        agent_name: impl Into<String>,
        message_summary: impl Into<String>,
    ) -> Self {
        Self::AgentConversation {
            agent_name: agent_name.into(),
            message_summary: message_summary.into(),
            timestamp: Utc::now(),
        }
    }

    /// Creates a PipelineComplete event.
    pub fn pipeline_complete(tasks_generated: usize, total_duration_ms: u64) -> Self {
        Self::PipelineComplete {
            tasks_generated,
            total_duration_ms,
            timestamp: Utc::now(),
        }
    }

    /// Creates a PipelineFailed event.
    pub fn pipeline_failed(error: impl Into<String>, stage: FactoryPipelineStage) -> Self {
        Self::PipelineFailed {
            error: error.into(),
            stage,
            timestamp: Utc::now(),
        }
    }
}

// ============================================================================
// LLM Weakness Types
// ============================================================================

/// Known weaknesses in LLMs that can be targeted by benchmark tasks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LlmWeaknessType {
    /// Multi-step reasoning requiring 10+ dependent steps.
    MultiStepReasoning,
    /// Tracking hidden state that changes between operations.
    StateTracking,
    /// Time-sensitive operations and race conditions.
    TemporalAwareness,
    /// Requirements not explicitly stated in the problem.
    ImplicitDependencies,
    /// Structures that appear one way but behave differently.
    DeceptivePatterns,
    /// Boundary conditions and off-by-one errors.
    EdgeCases,
    /// Memory limits, file handles, network timeouts.
    ResourceConstraints,
    /// Concurrent operations with subtle synchronization issues.
    Concurrency,
    /// Tasks requiring broad domain knowledge integration.
    DomainKnowledge,
    /// Complex error propagation and recovery scenarios.
    ErrorHandling,
}

impl std::fmt::Display for LlmWeaknessType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmWeaknessType::MultiStepReasoning => write!(f, "Multi-Step Reasoning"),
            LlmWeaknessType::StateTracking => write!(f, "State Tracking"),
            LlmWeaknessType::TemporalAwareness => write!(f, "Temporal Awareness"),
            LlmWeaknessType::ImplicitDependencies => write!(f, "Implicit Dependencies"),
            LlmWeaknessType::DeceptivePatterns => write!(f, "Deceptive Patterns"),
            LlmWeaknessType::EdgeCases => write!(f, "Edge Cases"),
            LlmWeaknessType::ResourceConstraints => write!(f, "Resource Constraints"),
            LlmWeaknessType::Concurrency => write!(f, "Concurrency"),
            LlmWeaknessType::DomainKnowledge => write!(f, "Domain Knowledge"),
            LlmWeaknessType::ErrorHandling => write!(f, "Error Handling"),
        }
    }
}

/// A specific LLM weakness identified for targeting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmWeakness {
    /// Type of weakness.
    pub weakness_type: LlmWeaknessType,
    /// Description of how this weakness manifests.
    pub description: String,
    /// How to exploit this weakness in a task.
    pub exploitation_strategy: String,
    /// Severity/impact score (0.0 - 1.0).
    pub severity: f64,
}

impl LlmWeakness {
    /// Creates a new LLM weakness.
    pub fn new(
        weakness_type: LlmWeaknessType,
        description: impl Into<String>,
        exploitation_strategy: impl Into<String>,
        severity: f64,
    ) -> Self {
        Self {
            weakness_type,
            description: description.into(),
            exploitation_strategy: exploitation_strategy.into(),
            severity: severity.clamp(0.0, 1.0),
        }
    }
}

// ============================================================================
// Difficulty Traps
// ============================================================================

/// Types of difficulty traps that can be added to tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DifficultyTrapType {
    /// Files or data that corrupt when accessed incorrectly.
    DataCorruption,
    /// Behavior that changes based on hidden state.
    StateDependent,
    /// Race conditions and time-of-check/time-of-use issues.
    Timing,
    /// Symlinks, unicode tricks, misleading file paths.
    DeceptiveStructure,
    /// Memory bombs, file descriptor leaks.
    ResourceExhaustion,
    /// Code or data that modifies itself during execution.
    SelfModifying,
    /// Hidden configuration that affects behavior.
    HiddenConfiguration,
    /// Dependencies between components that aren't obvious.
    CircularDependency,
    /// Subtle permission issues that cause failures.
    PermissionTrap,
    /// Environment-specific behavior differences.
    EnvironmentSensitive,
}

impl std::fmt::Display for DifficultyTrapType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DifficultyTrapType::DataCorruption => write!(f, "Data Corruption"),
            DifficultyTrapType::StateDependent => write!(f, "State Dependent"),
            DifficultyTrapType::Timing => write!(f, "Timing"),
            DifficultyTrapType::DeceptiveStructure => write!(f, "Deceptive Structure"),
            DifficultyTrapType::ResourceExhaustion => write!(f, "Resource Exhaustion"),
            DifficultyTrapType::SelfModifying => write!(f, "Self Modifying"),
            DifficultyTrapType::HiddenConfiguration => write!(f, "Hidden Configuration"),
            DifficultyTrapType::CircularDependency => write!(f, "Circular Dependency"),
            DifficultyTrapType::PermissionTrap => write!(f, "Permission Trap"),
            DifficultyTrapType::EnvironmentSensitive => write!(f, "Environment Sensitive"),
        }
    }
}

/// A specific difficulty trap to add to a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifficultyTrap {
    /// Unique identifier for this trap.
    pub id: String,
    /// Type of trap.
    pub trap_type: DifficultyTrapType,
    /// Description of the trap mechanism.
    pub description: String,
    /// How to implement this trap in the task.
    pub implementation: String,
    /// How a careful solver can detect and avoid the trap.
    pub detection_hint: String,
    /// How much this trap increases difficulty (0.0 - 1.0).
    pub difficulty_increase: f64,
    /// Which LLM weakness this trap targets.
    pub targets_weakness: LlmWeaknessType,
}

impl DifficultyTrap {
    /// Creates a new difficulty trap.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        trap_type: DifficultyTrapType,
        description: impl Into<String>,
        implementation: impl Into<String>,
        detection_hint: impl Into<String>,
        difficulty_increase: f64,
        targets_weakness: LlmWeaknessType,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            trap_type,
            description: description.into(),
            implementation: implementation.into(),
            detection_hint: detection_hint.into(),
            difficulty_increase: difficulty_increase.clamp(0.0, 1.0),
            targets_weakness,
        }
    }

    /// Creates a new trap with a specific ID.
    #[allow(clippy::too_many_arguments)]
    pub fn with_id(
        id: impl Into<String>,
        trap_type: DifficultyTrapType,
        description: impl Into<String>,
        implementation: impl Into<String>,
        detection_hint: impl Into<String>,
        difficulty_increase: f64,
        targets_weakness: LlmWeaknessType,
    ) -> Self {
        Self {
            id: id.into(),
            trap_type,
            description: description.into(),
            implementation: implementation.into(),
            detection_hint: detection_hint.into(),
            difficulty_increase: difficulty_increase.clamp(0.0, 1.0),
            targets_weakness,
        }
    }
}

// ============================================================================
// Research Findings
// ============================================================================

/// Findings from the Research Agent analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchFindings {
    /// Unique identifier for these findings.
    pub id: String,
    /// Category that was analyzed.
    pub category: String,
    /// High-level insights about the category.
    pub category_insights: Vec<String>,
    /// Identified LLM weaknesses.
    pub identified_weaknesses: Vec<LlmWeakness>,
    /// Proposed traps for difficulty amplification.
    pub proposed_traps: Vec<DifficultyTrap>,
    /// Factors that contribute to task difficulty.
    pub difficulty_factors: Vec<DifficultyFactor>,
    /// When this research was conducted.
    pub created_at: DateTime<Utc>,
}

impl ResearchFindings {
    /// Creates new research findings.
    pub fn new(category: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            category: category.into(),
            category_insights: Vec::new(),
            identified_weaknesses: Vec::new(),
            proposed_traps: Vec::new(),
            difficulty_factors: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Adds category insights.
    pub fn with_insights(mut self, insights: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.category_insights = insights.into_iter().map(|i| i.into()).collect();
        self
    }

    /// Adds identified weaknesses.
    pub fn with_weaknesses(mut self, weaknesses: Vec<LlmWeakness>) -> Self {
        self.identified_weaknesses = weaknesses;
        self
    }

    /// Adds proposed traps.
    pub fn with_traps(mut self, traps: Vec<DifficultyTrap>) -> Self {
        self.proposed_traps = traps;
        self
    }

    /// Adds difficulty factors.
    pub fn with_factors(mut self, factors: Vec<DifficultyFactor>) -> Self {
        self.difficulty_factors = factors;
        self
    }
}

/// A factor that contributes to task difficulty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifficultyFactor {
    /// Name of the factor.
    pub name: String,
    /// Description of how this factor increases difficulty.
    pub description: String,
    /// Weight of this factor in overall difficulty (0.0 - 1.0).
    pub weight: f64,
}

impl DifficultyFactor {
    /// Creates a new difficulty factor.
    pub fn new(name: impl Into<String>, description: impl Into<String>, weight: f64) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            weight: weight.clamp(0.0, 1.0),
        }
    }
}

// ============================================================================
// Factory Task Specification
// ============================================================================

/// A task specification generated by the factory pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactoryTaskSpec {
    /// Unique identifier for this task specification.
    pub id: String,
    /// Title of the task.
    pub title: String,
    /// Category of the task.
    pub category: String,
    /// Description of the problem.
    pub description: String,
    /// Difficulty level of the task.
    pub difficulty: DifficultyLevel,
    /// Traps added to increase difficulty.
    pub traps: Vec<DifficultyTrap>,
    /// Weaknesses targeted by this task.
    pub targeted_weaknesses: Vec<LlmWeaknessType>,
    /// Expected failure points where LLMs commonly fail.
    pub expected_failure_points: Vec<String>,
    /// Overall difficulty score (0.0 - 1.0).
    pub difficulty_score: f64,
    /// Skills required to complete the task.
    pub required_skills: Vec<String>,
    /// Solution approach (hidden from test-takers).
    pub solution_approach: String,
    /// When this task was generated.
    pub created_at: DateTime<Utc>,
}

impl FactoryTaskSpec {
    /// Creates a new factory task specification.
    pub fn new(
        title: impl Into<String>,
        category: impl Into<String>,
        description: impl Into<String>,
        difficulty: DifficultyLevel,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title: title.into(),
            category: category.into(),
            description: description.into(),
            difficulty,
            traps: Vec::new(),
            targeted_weaknesses: Vec::new(),
            expected_failure_points: Vec::new(),
            difficulty_score: difficulty.base_points() / 100.0,
            required_skills: Vec::new(),
            solution_approach: String::new(),
            created_at: Utc::now(),
        }
    }

    /// Adds traps to the task.
    pub fn with_traps(mut self, traps: Vec<DifficultyTrap>) -> Self {
        self.traps = traps;
        self
    }

    /// Adds targeted weaknesses.
    pub fn with_targeted_weaknesses(mut self, weaknesses: Vec<LlmWeaknessType>) -> Self {
        self.targeted_weaknesses = weaknesses;
        self
    }

    /// Adds expected failure points.
    pub fn with_expected_failure_points(
        mut self,
        points: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.expected_failure_points = points.into_iter().map(|p| p.into()).collect();
        self
    }

    /// Sets the difficulty score.
    pub fn with_difficulty_score(mut self, score: f64) -> Self {
        self.difficulty_score = score.clamp(0.0, 1.0);
        self
    }

    /// Adds required skills.
    pub fn with_required_skills(
        mut self,
        skills: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.required_skills = skills.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Sets the solution approach.
    pub fn with_solution_approach(mut self, approach: impl Into<String>) -> Self {
        self.solution_approach = approach.into();
        self
    }
}

// ============================================================================
// Agent Conversation
// ============================================================================

/// A conversation turn in multi-agent interactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    /// Name of the agent that sent this message.
    pub agent_name: String,
    /// Role of the message (system, user, assistant).
    pub role: String,
    /// Content of the message.
    pub content: String,
    /// Timestamp of the turn.
    pub timestamp: DateTime<Utc>,
}

impl ConversationTurn {
    /// Creates a new conversation turn.
    pub fn new(
        agent_name: impl Into<String>,
        role: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            agent_name: agent_name.into(),
            role: role.into(),
            content: content.into(),
            timestamp: Utc::now(),
        }
    }

    /// Creates a system message turn.
    pub fn system(agent_name: impl Into<String>, content: impl Into<String>) -> Self {
        Self::new(agent_name, "system", content)
    }

    /// Creates a user message turn.
    pub fn user(agent_name: impl Into<String>, content: impl Into<String>) -> Self {
        Self::new(agent_name, "user", content)
    }

    /// Creates an assistant message turn.
    pub fn assistant(agent_name: impl Into<String>, content: impl Into<String>) -> Self {
        Self::new(agent_name, "assistant", content)
    }
}

/// A multi-turn conversation context between agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConversation {
    /// Unique identifier for this conversation.
    pub id: String,
    /// Participants in the conversation.
    pub participants: Vec<String>,
    /// Conversation turns.
    pub turns: Vec<ConversationTurn>,
    /// Context/topic of the conversation.
    pub context: String,
    /// When this conversation started.
    pub started_at: DateTime<Utc>,
}

impl AgentConversation {
    /// Creates a new agent conversation.
    pub fn new(context: impl Into<String>, participants: Vec<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            participants,
            turns: Vec::new(),
            context: context.into(),
            started_at: Utc::now(),
        }
    }

    /// Adds a turn to the conversation.
    pub fn add_turn(&mut self, turn: ConversationTurn) {
        self.turns.push(turn);
    }

    /// Gets the last turn in the conversation.
    pub fn last_turn(&self) -> Option<&ConversationTurn> {
        self.turns.last()
    }

    /// Gets the number of turns in the conversation.
    pub fn turn_count(&self) -> usize {
        self.turns.len()
    }
}

// ============================================================================
// Amplified Task
// ============================================================================

/// A task that has been amplified with difficulty traps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmplifiedTask {
    /// The original task specification.
    pub original_spec: FactoryTaskSpec,
    /// Traps that were added.
    pub traps_added: Vec<DifficultyTrap>,
    /// Expected failure points where LLMs will struggle.
    pub expected_failure_points: Vec<String>,
    /// Final difficulty score after amplification.
    pub difficulty_score: f64,
    /// Amplification notes from the agent.
    pub amplification_notes: String,
}

impl AmplifiedTask {
    /// Creates a new amplified task.
    pub fn new(original_spec: FactoryTaskSpec) -> Self {
        let base_score = original_spec.difficulty_score;
        Self {
            original_spec,
            traps_added: Vec::new(),
            expected_failure_points: Vec::new(),
            difficulty_score: base_score,
            amplification_notes: String::new(),
        }
    }

    /// Adds traps to the task.
    pub fn with_traps(mut self, traps: Vec<DifficultyTrap>) -> Self {
        // Calculate new difficulty score based on traps
        let trap_increase: f64 = traps.iter().map(|t| t.difficulty_increase).sum();
        self.difficulty_score = (self.original_spec.difficulty_score + trap_increase).min(1.0);
        self.traps_added = traps;
        self
    }

    /// Adds expected failure points.
    pub fn with_expected_failure_points(
        mut self,
        points: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.expected_failure_points = points.into_iter().map(|p| p.into()).collect();
        self
    }

    /// Sets amplification notes.
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.amplification_notes = notes.into();
        self
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_stage_display() {
        assert_eq!(format!("{}", FactoryPipelineStage::Research), "Research");
        assert_eq!(format!("{}", FactoryPipelineStage::Creation), "Creation");
        assert_eq!(
            format!("{}", FactoryPipelineStage::Amplification),
            "Amplification"
        );
        assert_eq!(
            format!("{}", FactoryPipelineStage::Validation),
            "Validation"
        );
        assert_eq!(
            format!("{}", FactoryPipelineStage::Finalization),
            "Finalization"
        );
    }

    #[test]
    fn test_pipeline_event_constructors() {
        let stage_event = FactoryPipelineEvent::stage_started(FactoryPipelineStage::Research);
        match stage_event {
            FactoryPipelineEvent::StageStarted { stage, .. } => {
                assert_eq!(stage, FactoryPipelineStage::Research);
            }
            _ => panic!("Expected StageStarted event"),
        }

        let research_event = FactoryPipelineEvent::research_complete(5, 3);
        match research_event {
            FactoryPipelineEvent::ResearchComplete {
                weaknesses_found,
                traps_proposed,
                ..
            } => {
                assert_eq!(weaknesses_found, 5);
                assert_eq!(traps_proposed, 3);
            }
            _ => panic!("Expected ResearchComplete event"),
        }
    }

    #[test]
    fn test_llm_weakness_creation() {
        let weakness = LlmWeakness::new(
            LlmWeaknessType::MultiStepReasoning,
            "LLMs struggle with 10+ step reasoning chains",
            "Create tasks requiring deep logical dependencies",
            0.8,
        );

        assert_eq!(weakness.weakness_type, LlmWeaknessType::MultiStepReasoning);
        assert!((weakness.severity - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_llm_weakness_severity_clamping() {
        let weakness = LlmWeakness::new(
            LlmWeaknessType::StateTracking,
            "desc",
            "strategy",
            1.5, // Should be clamped to 1.0
        );
        assert!((weakness.severity - 1.0).abs() < 0.01);

        let weakness2 = LlmWeakness::new(
            LlmWeaknessType::StateTracking,
            "desc",
            "strategy",
            -0.5, // Should be clamped to 0.0
        );
        assert!((weakness2.severity - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_difficulty_trap_creation() {
        let trap = DifficultyTrap::new(
            DifficultyTrapType::DataCorruption,
            "File corrupts when read in wrong mode",
            "Open file in text mode when it's binary",
            "Check file magic bytes before reading",
            0.25,
            LlmWeaknessType::ImplicitDependencies,
        );

        assert_eq!(trap.trap_type, DifficultyTrapType::DataCorruption);
        assert!(!trap.id.is_empty());
        assert!((trap.difficulty_increase - 0.25).abs() < 0.01);
    }

    #[test]
    fn test_research_findings_builder() {
        let findings = ResearchFindings::new("debugging")
            .with_insights(["insight1", "insight2"])
            .with_factors(vec![DifficultyFactor::new("factor1", "description", 0.5)]);

        assert_eq!(findings.category, "debugging");
        assert_eq!(findings.category_insights.len(), 2);
        assert_eq!(findings.difficulty_factors.len(), 1);
    }

    #[test]
    fn test_factory_task_spec_builder() {
        let spec = FactoryTaskSpec::new(
            "Debug Memory Leak",
            "debugging",
            "Find the memory leak",
            DifficultyLevel::Hard,
        )
        .with_required_skills(["rust", "profiling"])
        .with_difficulty_score(0.85);

        assert_eq!(spec.title, "Debug Memory Leak");
        assert_eq!(spec.required_skills.len(), 2);
        assert!((spec.difficulty_score - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_conversation_turn_constructors() {
        let system = ConversationTurn::system("orchestrator", "Initialize research");
        assert_eq!(system.role, "system");
        assert_eq!(system.agent_name, "orchestrator");

        let user = ConversationTurn::user("research_agent", "Analysis request");
        assert_eq!(user.role, "user");

        let assistant = ConversationTurn::assistant("research_agent", "Here are findings");
        assert_eq!(assistant.role, "assistant");
    }

    #[test]
    fn test_agent_conversation() {
        let mut conversation = AgentConversation::new(
            "Research Phase",
            vec!["orchestrator".to_string(), "research_agent".to_string()],
        );

        assert_eq!(conversation.turn_count(), 0);
        assert!(conversation.last_turn().is_none());

        conversation.add_turn(ConversationTurn::system("orchestrator", "Start"));
        conversation.add_turn(ConversationTurn::assistant("research_agent", "Done"));

        assert_eq!(conversation.turn_count(), 2);
        assert_eq!(
            conversation.last_turn().map(|t| t.agent_name.as_str()),
            Some("research_agent")
        );
    }

    #[test]
    fn test_amplified_task_score_calculation() {
        let spec = FactoryTaskSpec::new(
            "Test Task",
            "debugging",
            "Description",
            DifficultyLevel::Medium,
        )
        .with_difficulty_score(0.5);

        let trap1 = DifficultyTrap::new(
            DifficultyTrapType::Timing,
            "Race condition",
            "impl",
            "hint",
            0.2,
            LlmWeaknessType::TemporalAwareness,
        );
        let trap2 = DifficultyTrap::new(
            DifficultyTrapType::StateDependent,
            "Hidden state",
            "impl",
            "hint",
            0.15,
            LlmWeaknessType::StateTracking,
        );

        let amplified = AmplifiedTask::new(spec).with_traps(vec![trap1, trap2]);

        // 0.5 + 0.2 + 0.15 = 0.85
        assert!((amplified.difficulty_score - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_difficulty_trap_type_display() {
        assert_eq!(
            format!("{}", DifficultyTrapType::DataCorruption),
            "Data Corruption"
        );
        assert_eq!(
            format!("{}", DifficultyTrapType::StateDependent),
            "State Dependent"
        );
        assert_eq!(format!("{}", DifficultyTrapType::Timing), "Timing");
    }

    #[test]
    fn test_llm_weakness_type_display() {
        assert_eq!(
            format!("{}", LlmWeaknessType::MultiStepReasoning),
            "Multi-Step Reasoning"
        );
        assert_eq!(
            format!("{}", LlmWeaknessType::StateTracking),
            "State Tracking"
        );
    }
}
