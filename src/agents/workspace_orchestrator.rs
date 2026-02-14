//! Workspace Orchestrator for complete workspace generation pipeline.
//!
//! This module provides the main orchestrator that coordinates the full pipeline
//! for generating workspaces with intentionally injected vulnerabilities:
//!
//! 1. **Debate**: Multi-agent debate to decide project parameters
//! 2. **Generate**: Create clean base code (CodeGeneratorAgent)
//! 3. **Inject**: Add vulnerabilities (future: VulnerabilityInjectorAgent)
//! 4. **Clean**: Remove hints (future: CodeCleanerAgent)
//! 5. **Validate**: Check feasibility (future: WorkspaceValidatorAgent)
//! 6. **Review**: Final multi-agent review
//! 7. **Export**: Create .zip and artifacts

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::debate_agents::DebateTopic;
use super::debate_orchestrator::{
    ConsensusResult, DebateContext, DebateEvent, DebateOrchestrator, DebateOrchestratorConfig,
};
use super::error::{AgentError, AgentResult};
use super::workspace_ideator::ProgrammingLanguage;
use crate::llm::LlmProvider;

// ============================================================================
// Pipeline Stages
// ============================================================================

/// Stages in the workspace generation pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkspacePipelineStage {
    /// Multi-agent debate to decide project parameters.
    Debate,
    /// Generating clean base code.
    Generation,
    /// Injecting vulnerabilities into the code.
    Injection,
    /// Cleaning code to remove hints.
    Cleaning,
    /// Validating the workspace is solvable.
    Validation,
    /// Final multi-agent review.
    Review,
    /// Exporting artifacts.
    Export,
}

impl std::fmt::Display for WorkspacePipelineStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Debate => write!(f, "Debate"),
            Self::Generation => write!(f, "Generation"),
            Self::Injection => write!(f, "Injection"),
            Self::Cleaning => write!(f, "Cleaning"),
            Self::Validation => write!(f, "Validation"),
            Self::Review => write!(f, "Review"),
            Self::Export => write!(f, "Export"),
        }
    }
}

// ============================================================================
// Pipeline Events
// ============================================================================

/// Events emitted during workspace generation for TUI updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkspacePipelineEvent {
    /// Pipeline has started.
    PipelineStarted {
        /// Unique workspace ID.
        workspace_id: String,
        /// Target language.
        language: ProgrammingLanguage,
        /// Optional category hint.
        category_hint: Option<String>,
        /// When pipeline started.
        timestamp: DateTime<Utc>,
    },
    /// A stage has started.
    StageStarted {
        /// The stage that started.
        stage: WorkspacePipelineStage,
        /// When stage started.
        timestamp: DateTime<Utc>,
    },
    /// A stage has completed.
    StageCompleted {
        /// The stage that completed.
        stage: WorkspacePipelineStage,
        /// Duration in milliseconds.
        duration_ms: u64,
        /// When stage completed.
        timestamp: DateTime<Utc>,
    },
    /// Debate sub-event occurred.
    DebateProgress {
        /// The underlying debate event.
        event: DebateEvent,
    },
    /// Code generation progress.
    GenerationProgress {
        /// Number of files generated so far.
        files_generated: usize,
        /// Total lines of code so far.
        loc_generated: usize,
        /// When progress was reported.
        timestamp: DateTime<Utc>,
    },
    /// Vulnerability injection progress.
    InjectionProgress {
        /// Type of vulnerability being injected.
        vulnerability_type: String,
        /// File being modified.
        target_file: String,
        /// When progress was reported.
        timestamp: DateTime<Utc>,
    },
    /// Validation result.
    ValidationResult {
        /// Whether validation passed.
        passed: bool,
        /// Issues found (if any).
        issues: Vec<String>,
        /// When validation completed.
        timestamp: DateTime<Utc>,
    },
    /// Review result.
    ReviewResult {
        /// Overall score (0.0 - 1.0).
        score: f64,
        /// Review comments.
        comments: Vec<String>,
        /// When review completed.
        timestamp: DateTime<Utc>,
    },
    /// Pipeline completed successfully.
    PipelineCompleted {
        /// The generated workspace.
        workspace: GeneratedWorkspaceResult,
        /// Total duration in milliseconds.
        total_duration_ms: u64,
        /// When pipeline completed.
        timestamp: DateTime<Utc>,
    },
    /// Pipeline failed.
    PipelineFailed {
        /// Error description.
        error: String,
        /// Stage where failure occurred.
        stage: WorkspacePipelineStage,
        /// When failure occurred.
        timestamp: DateTime<Utc>,
    },
}

impl WorkspacePipelineEvent {
    /// Creates a PipelineStarted event.
    pub fn pipeline_started(
        workspace_id: impl Into<String>,
        language: ProgrammingLanguage,
        category_hint: Option<String>,
    ) -> Self {
        Self::PipelineStarted {
            workspace_id: workspace_id.into(),
            language,
            category_hint,
            timestamp: Utc::now(),
        }
    }

    /// Creates a StageStarted event.
    pub fn stage_started(stage: WorkspacePipelineStage) -> Self {
        Self::StageStarted {
            stage,
            timestamp: Utc::now(),
        }
    }

    /// Creates a StageCompleted event.
    pub fn stage_completed(stage: WorkspacePipelineStage, duration_ms: u64) -> Self {
        Self::StageCompleted {
            stage,
            duration_ms,
            timestamp: Utc::now(),
        }
    }

    /// Creates a DebateProgress event.
    pub fn debate_progress(event: DebateEvent) -> Self {
        Self::DebateProgress { event }
    }

    /// Creates a GenerationProgress event.
    pub fn generation_progress(files_generated: usize, loc_generated: usize) -> Self {
        Self::GenerationProgress {
            files_generated,
            loc_generated,
            timestamp: Utc::now(),
        }
    }

    /// Creates an InjectionProgress event.
    pub fn injection_progress(
        vulnerability_type: impl Into<String>,
        target_file: impl Into<String>,
    ) -> Self {
        Self::InjectionProgress {
            vulnerability_type: vulnerability_type.into(),
            target_file: target_file.into(),
            timestamp: Utc::now(),
        }
    }

    /// Creates a ValidationResult event.
    pub fn validation_result(passed: bool, issues: Vec<String>) -> Self {
        Self::ValidationResult {
            passed,
            issues,
            timestamp: Utc::now(),
        }
    }

    /// Creates a ReviewResult event.
    pub fn review_result(score: f64, comments: Vec<String>) -> Self {
        Self::ReviewResult {
            score,
            comments,
            timestamp: Utc::now(),
        }
    }

    /// Creates a PipelineCompleted event.
    pub fn pipeline_completed(workspace: GeneratedWorkspaceResult, total_duration_ms: u64) -> Self {
        Self::PipelineCompleted {
            workspace,
            total_duration_ms,
            timestamp: Utc::now(),
        }
    }

    /// Creates a PipelineFailed event.
    pub fn pipeline_failed(error: impl Into<String>, stage: WorkspacePipelineStage) -> Self {
        Self::PipelineFailed {
            error: error.into(),
            stage,
            timestamp: Utc::now(),
        }
    }
}

// ============================================================================
// Workspace Result
// ============================================================================

/// The result of a complete workspace generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedWorkspaceResult {
    /// Unique identifier for this workspace.
    pub id: String,
    /// Project name.
    pub project_name: String,
    /// Programming language.
    pub language: ProgrammingLanguage,
    /// Category (e.g., "web-api", "cli-tool").
    pub category: String,
    /// Description of the project.
    pub description: String,
    /// Files in the workspace.
    pub files: Vec<WorkspaceFile>,
    /// Injected vulnerabilities.
    pub vulnerabilities: Vec<InjectedVulnerability>,
    /// Debate results that led to this design.
    pub debate_results: Vec<DebateOutcome>,
    /// Review scores from final review.
    pub review_scores: HashMap<String, f64>,
    /// Build/run instructions.
    pub build_instructions: String,
    /// Test instructions.
    pub test_instructions: String,
    /// Total lines of code.
    pub total_loc: usize,
    /// When this workspace was generated.
    pub created_at: DateTime<Utc>,
}

impl GeneratedWorkspaceResult {
    /// Creates a new workspace result.
    pub fn new(
        project_name: impl Into<String>,
        language: ProgrammingLanguage,
        category: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            project_name: project_name.into(),
            language,
            category: category.into(),
            description: String::new(),
            files: Vec::new(),
            vulnerabilities: Vec::new(),
            debate_results: Vec::new(),
            review_scores: HashMap::new(),
            build_instructions: String::new(),
            test_instructions: String::new(),
            total_loc: 0,
            created_at: Utc::now(),
        }
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Sets the files.
    pub fn with_files(mut self, files: Vec<WorkspaceFile>) -> Self {
        self.total_loc = files.iter().map(|f| f.line_count()).sum();
        self.files = files;
        self
    }

    /// Adds vulnerabilities.
    pub fn with_vulnerabilities(mut self, vulnerabilities: Vec<InjectedVulnerability>) -> Self {
        self.vulnerabilities = vulnerabilities;
        self
    }

    /// Adds debate results.
    pub fn with_debate_results(mut self, results: Vec<DebateOutcome>) -> Self {
        self.debate_results = results;
        self
    }

    /// Adds review scores.
    pub fn with_review_scores(mut self, scores: HashMap<String, f64>) -> Self {
        self.review_scores = scores;
        self
    }

    /// Sets build instructions.
    pub fn with_build_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.build_instructions = instructions.into();
        self
    }

    /// Sets test instructions.
    pub fn with_test_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.test_instructions = instructions.into();
        self
    }
}

/// A file in the generated workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceFile {
    /// Relative path within the workspace.
    pub path: String,
    /// File content.
    pub content: String,
    /// Description of file purpose.
    pub description: String,
}

impl WorkspaceFile {
    /// Creates a new workspace file.
    pub fn new(
        path: impl Into<String>,
        content: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
            description: description.into(),
        }
    }

    /// Returns the number of lines in the file.
    pub fn line_count(&self) -> usize {
        self.content.lines().count()
    }
}

/// A vulnerability that was injected into the workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectedVulnerability {
    /// Type of vulnerability.
    pub vulnerability_type: String,
    /// File where vulnerability was injected.
    pub file_path: String,
    /// Line number(s) where vulnerability exists.
    pub line_numbers: Vec<usize>,
    /// Description of the vulnerability.
    pub description: String,
    /// How to detect/fix the vulnerability.
    pub remediation_hint: String,
    /// Severity score (1-10).
    pub severity: u8,
}

impl InjectedVulnerability {
    /// Creates a new injected vulnerability.
    pub fn new(
        vulnerability_type: impl Into<String>,
        file_path: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            vulnerability_type: vulnerability_type.into(),
            file_path: file_path.into(),
            line_numbers: Vec::new(),
            description: description.into(),
            remediation_hint: String::new(),
            severity: 5,
        }
    }

    /// Sets line numbers.
    pub fn with_line_numbers(mut self, lines: Vec<usize>) -> Self {
        self.line_numbers = lines;
        self
    }

    /// Sets remediation hint.
    pub fn with_remediation(mut self, hint: impl Into<String>) -> Self {
        self.remediation_hint = hint.into();
        self
    }

    /// Sets severity.
    pub fn with_severity(mut self, severity: u8) -> Self {
        self.severity = severity.clamp(1, 10);
        self
    }
}

/// Summary of a debate outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateOutcome {
    /// Topic that was debated.
    pub topic: String,
    /// Whether consensus was reached.
    pub consensus_reached: bool,
    /// The winning position.
    pub decision: String,
    /// Consensus score.
    pub score: f64,
}

impl DebateOutcome {
    /// Creates a debate outcome from a consensus result.
    pub fn from_consensus(result: &ConsensusResult) -> Self {
        Self {
            topic: result.topic.display_name().to_string(),
            consensus_reached: result.consensus_reached,
            decision: result.winning_position.clone().unwrap_or_default(),
            score: result.consensus_score,
        }
    }
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the workspace orchestrator.
#[derive(Debug, Clone)]
pub struct WorkspaceOrchestratorConfig {
    /// Configuration for debates.
    pub debate_config: DebateOrchestratorConfig,
    /// Whether to run the debate phase.
    pub enable_debate: bool,
    /// Whether to inject vulnerabilities.
    pub enable_injection: bool,
    /// Whether to run final review.
    pub enable_review: bool,
    /// Maximum files to generate.
    pub max_files: usize,
    /// Target lines of code.
    pub target_loc: usize,
    /// LLM temperature for generation.
    pub generation_temperature: f64,
    /// Maximum generation tokens.
    pub max_generation_tokens: u32,
}

impl Default for WorkspaceOrchestratorConfig {
    fn default() -> Self {
        Self {
            debate_config: DebateOrchestratorConfig::default(),
            enable_debate: true,
            enable_injection: true,
            enable_review: true,
            max_files: 20,
            target_loc: 1000,
            generation_temperature: 0.5,
            max_generation_tokens: 8000,
        }
    }
}

impl WorkspaceOrchestratorConfig {
    /// Creates a new configuration with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets debate configuration.
    pub fn with_debate_config(mut self, config: DebateOrchestratorConfig) -> Self {
        self.debate_config = config;
        self
    }

    /// Enables or disables the debate phase.
    pub fn with_debate(mut self, enabled: bool) -> Self {
        self.enable_debate = enabled;
        self
    }

    /// Enables or disables vulnerability injection.
    pub fn with_injection(mut self, enabled: bool) -> Self {
        self.enable_injection = enabled;
        self
    }

    /// Enables or disables final review.
    pub fn with_review(mut self, enabled: bool) -> Self {
        self.enable_review = enabled;
        self
    }

    /// Sets maximum files to generate.
    pub fn with_max_files(mut self, max: usize) -> Self {
        self.max_files = max.max(1);
        self
    }

    /// Sets target lines of code.
    pub fn with_target_loc(mut self, loc: usize) -> Self {
        self.target_loc = loc.max(100);
        self
    }

    /// Sets generation temperature.
    pub fn with_generation_temperature(mut self, temp: f64) -> Self {
        self.generation_temperature = temp.clamp(0.0, 2.0);
        self
    }
}

// ============================================================================
// Workspace Orchestrator
// ============================================================================

/// Main orchestrator for the workspace generation pipeline.
///
/// Coordinates:
/// 1. Multi-agent debate to determine project parameters
/// 2. Code generation for clean base workspace
/// 3. Vulnerability injection
/// 4. Code cleaning to remove hints
/// 5. Validation that workspace is solvable
/// 6. Final multi-agent review
/// 7. Export to artifacts
#[allow(dead_code)]
pub struct WorkspaceOrchestrator {
    /// LLM client for all operations.
    llm_client: Arc<dyn LlmProvider>,
    /// Debate orchestrator for multi-agent debates.
    debate_orchestrator: DebateOrchestrator,
    /// Orchestrator configuration.
    config: WorkspaceOrchestratorConfig,
}

impl WorkspaceOrchestrator {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "workspace_orchestrator";

    /// Creates a new workspace orchestrator.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: WorkspaceOrchestratorConfig) -> Self {
        let debate_orchestrator =
            DebateOrchestrator::new(Arc::clone(&llm_client), config.debate_config.clone());

        Self {
            llm_client,
            debate_orchestrator,
            config,
        }
    }

    /// Creates a new orchestrator with default configuration.
    pub fn with_defaults(llm_client: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm_client, WorkspaceOrchestratorConfig::default())
    }

    /// Returns a new builder for configuration.
    pub fn builder() -> WorkspaceOrchestratorBuilder {
        WorkspaceOrchestratorBuilder::new()
    }

    /// Returns the configuration.
    pub fn config(&self) -> &WorkspaceOrchestratorConfig {
        &self.config
    }

    /// Generates a complete workspace.
    ///
    /// # Arguments
    ///
    /// * `language` - Target programming language.
    /// * `category_hint` - Optional category hint (e.g., "web-api", "cli-tool").
    /// * `event_tx` - Channel for emitting events.
    ///
    /// # Returns
    ///
    /// The generated workspace result.
    pub async fn generate_workspace(
        &self,
        language: ProgrammingLanguage,
        category_hint: Option<&str>,
        event_tx: mpsc::Sender<WorkspacePipelineEvent>,
    ) -> AgentResult<GeneratedWorkspaceResult> {
        let start_time = Instant::now();
        let workspace_id = Uuid::new_v4().to_string();

        // Emit pipeline started
        self.send_event(
            &event_tx,
            WorkspacePipelineEvent::pipeline_started(
                &workspace_id,
                language,
                category_hint.map(String::from),
            ),
        )
        .await;

        let mut debate_results = Vec::new();

        // Stage 1: Multi-agent debate (if enabled)
        let project_params = if self.config.enable_debate {
            let stage_start = Instant::now();
            self.send_event(
                &event_tx,
                WorkspacePipelineEvent::stage_started(WorkspacePipelineStage::Debate),
            )
            .await;

            let params = self
                .run_debate_phase(language, category_hint, &event_tx, &mut debate_results)
                .await?;

            self.send_event(
                &event_tx,
                WorkspacePipelineEvent::stage_completed(
                    WorkspacePipelineStage::Debate,
                    stage_start.elapsed().as_millis() as u64,
                ),
            )
            .await;

            params
        } else {
            ProjectParameters::default_for(language, category_hint)
        };

        // Stage 2: Code Generation
        let stage_start = Instant::now();
        self.send_event(
            &event_tx,
            WorkspacePipelineEvent::stage_started(WorkspacePipelineStage::Generation),
        )
        .await;

        let (files, build_instructions, test_instructions) =
            self.generate_code(&project_params, &event_tx).await?;

        self.send_event(
            &event_tx,
            WorkspacePipelineEvent::stage_completed(
                WorkspacePipelineStage::Generation,
                stage_start.elapsed().as_millis() as u64,
            ),
        )
        .await;

        // Stage 3: Vulnerability Injection (if enabled)
        let vulnerabilities = if self.config.enable_injection {
            let stage_start = Instant::now();
            self.send_event(
                &event_tx,
                WorkspacePipelineEvent::stage_started(WorkspacePipelineStage::Injection),
            )
            .await;

            let vulns = self
                .inject_vulnerabilities(&project_params, &files, &event_tx)
                .await?;

            self.send_event(
                &event_tx,
                WorkspacePipelineEvent::stage_completed(
                    WorkspacePipelineStage::Injection,
                    stage_start.elapsed().as_millis() as u64,
                ),
            )
            .await;

            vulns
        } else {
            Vec::new()
        };

        // Stage 4: Cleaning (placeholder for future implementation)
        let stage_start = Instant::now();
        self.send_event(
            &event_tx,
            WorkspacePipelineEvent::stage_started(WorkspacePipelineStage::Cleaning),
        )
        .await;

        // Cleaning would remove comments that hint at vulnerabilities
        // For now, we just pass through

        self.send_event(
            &event_tx,
            WorkspacePipelineEvent::stage_completed(
                WorkspacePipelineStage::Cleaning,
                stage_start.elapsed().as_millis() as u64,
            ),
        )
        .await;

        // Stage 5: Validation
        let stage_start = Instant::now();
        self.send_event(
            &event_tx,
            WorkspacePipelineEvent::stage_started(WorkspacePipelineStage::Validation),
        )
        .await;

        let validation_result = self.validate_workspace(&project_params, &files).await?;
        self.send_event(
            &event_tx,
            WorkspacePipelineEvent::validation_result(
                validation_result.passed,
                validation_result.issues.clone(),
            ),
        )
        .await;

        if !validation_result.passed {
            self.send_event(
                &event_tx,
                WorkspacePipelineEvent::pipeline_failed(
                    format!("Validation failed: {:?}", validation_result.issues),
                    WorkspacePipelineStage::Validation,
                ),
            )
            .await;
            return Err(AgentError::GenerationFailed(format!(
                "Workspace validation failed: {:?}",
                validation_result.issues
            )));
        }

        self.send_event(
            &event_tx,
            WorkspacePipelineEvent::stage_completed(
                WorkspacePipelineStage::Validation,
                stage_start.elapsed().as_millis() as u64,
            ),
        )
        .await;

        // Stage 6: Review (if enabled)
        let review_scores = if self.config.enable_review {
            let stage_start = Instant::now();
            self.send_event(
                &event_tx,
                WorkspacePipelineEvent::stage_started(WorkspacePipelineStage::Review),
            )
            .await;

            let (scores, comments) = self
                .run_review(&project_params, &files, &vulnerabilities)
                .await?;

            let overall_score = scores.values().sum::<f64>() / scores.len().max(1) as f64;
            self.send_event(
                &event_tx,
                WorkspacePipelineEvent::review_result(overall_score, comments),
            )
            .await;

            self.send_event(
                &event_tx,
                WorkspacePipelineEvent::stage_completed(
                    WorkspacePipelineStage::Review,
                    stage_start.elapsed().as_millis() as u64,
                ),
            )
            .await;

            scores
        } else {
            HashMap::new()
        };

        // Stage 7: Export
        let stage_start = Instant::now();
        self.send_event(
            &event_tx,
            WorkspacePipelineEvent::stage_started(WorkspacePipelineStage::Export),
        )
        .await;

        let workspace = GeneratedWorkspaceResult::new(
            &project_params.project_name,
            language,
            &project_params.category,
        )
        .with_description(&project_params.description)
        .with_files(files)
        .with_vulnerabilities(vulnerabilities)
        .with_debate_results(debate_results)
        .with_review_scores(review_scores)
        .with_build_instructions(build_instructions)
        .with_test_instructions(test_instructions);

        self.send_event(
            &event_tx,
            WorkspacePipelineEvent::stage_completed(
                WorkspacePipelineStage::Export,
                stage_start.elapsed().as_millis() as u64,
            ),
        )
        .await;

        // Pipeline complete
        let total_duration_ms = start_time.elapsed().as_millis() as u64;
        self.send_event(
            &event_tx,
            WorkspacePipelineEvent::pipeline_completed(workspace.clone(), total_duration_ms),
        )
        .await;

        Ok(workspace)
    }

    /// Runs the multi-agent debate phase to determine project parameters.
    async fn run_debate_phase(
        &self,
        language: ProgrammingLanguage,
        category_hint: Option<&str>,
        event_tx: &mpsc::Sender<WorkspacePipelineEvent>,
        debate_results: &mut Vec<DebateOutcome>,
    ) -> AgentResult<ProjectParameters> {
        // Create event channel for debate sub-events
        let (debate_tx, mut debate_rx) = mpsc::channel::<DebateEvent>(100);

        // Forward debate events to main event channel
        let event_tx_clone = event_tx.clone();
        let forwarder = tokio::spawn(async move {
            while let Some(event) = debate_rx.recv().await {
                let _ = event_tx_clone
                    .send(WorkspacePipelineEvent::debate_progress(event))
                    .await;
            }
        });

        // Debate 1: Project Type
        let context_str = format!(
            "Target language: {}. Category hint: {}. Generate a realistic project specification.",
            language.display_name(),
            category_hint.unwrap_or("any")
        );

        let project_type_context = DebateContext::new(DebateTopic::ProjectType, &context_str)
            .with_param("language", language.display_name())
            .with_param("category", category_hint.unwrap_or("general"));

        let project_type_result = self
            .debate_orchestrator
            .conduct_debate(project_type_context, debate_tx.clone())
            .await?;

        debate_results.push(DebateOutcome::from_consensus(&project_type_result));

        let project_type = project_type_result
            .winning_position
            .clone()
            .unwrap_or_else(|| "api".to_string());

        // Debate 2: Difficulty level
        let difficulty_context = DebateContext::new(
            DebateTopic::Difficulty,
            format!(
                "Project type: {}. Language: {}. Determine appropriate complexity.",
                project_type,
                language.display_name()
            ),
        )
        .with_param("task_description", &project_type);

        let difficulty_result = self
            .debate_orchestrator
            .conduct_debate(difficulty_context, debate_tx.clone())
            .await?;

        debate_results.push(DebateOutcome::from_consensus(&difficulty_result));

        // Debate 3: Feasibility check
        let feasibility_context = DebateContext::new(
            DebateTopic::Feasibility,
            format!(
                "Project: {} in {}. Complexity: moderate. Check if feasible.",
                project_type,
                language.display_name()
            ),
        )
        .with_param("approach", &project_type);

        let feasibility_result = self
            .debate_orchestrator
            .conduct_debate(feasibility_context, debate_tx)
            .await?;

        debate_results.push(DebateOutcome::from_consensus(&feasibility_result));

        // Stop forwarder
        forwarder.abort();

        // Build project parameters from debate results
        Ok(ProjectParameters {
            project_name: format!(
                "{}-{}-project",
                language.display_name().to_lowercase(),
                category_hint.unwrap_or("sample")
            ),
            language,
            category: category_hint.unwrap_or("general").to_string(),
            project_type,
            description: format!(
                "A {} project demonstrating {} patterns",
                language.display_name(),
                category_hint.unwrap_or("common")
            ),
            complexity: "moderate".to_string(),
        })
    }

    /// Generates the base code for the workspace.
    async fn generate_code(
        &self,
        params: &ProjectParameters,
        event_tx: &mpsc::Sender<WorkspacePipelineEvent>,
    ) -> AgentResult<(Vec<WorkspaceFile>, String, String)> {
        // For now, generate a basic project structure
        // In a full implementation, this would call CodeGeneratorAgent

        let files = self.generate_basic_structure(params)?;

        // Emit progress
        let loc: usize = files.iter().map(|f| f.line_count()).sum();
        self.send_event(
            event_tx,
            WorkspacePipelineEvent::generation_progress(files.len(), loc),
        )
        .await;

        let build_instructions = format!(
            "# Building {}\n\n1. Install dependencies\n2. Run the build command\n3. Execute the application",
            params.project_name
        );

        let test_instructions = format!(
            "# Testing {}\n\n1. Install test dependencies\n2. Run the test command",
            params.project_name
        );

        Ok((files, build_instructions, test_instructions))
    }

    /// Generates a basic project structure.
    fn generate_basic_structure(
        &self,
        params: &ProjectParameters,
    ) -> AgentResult<Vec<WorkspaceFile>> {
        let mut files = Vec::new();

        match params.language {
            ProgrammingLanguage::Python => {
                files.push(WorkspaceFile::new(
                    "main.py",
                    r#"#!/usr/bin/env python3
"""Main entry point for the application."""

import sys
from typing import Optional

def main(args: Optional[list] = None) -> int:
    """Main function that starts the application.
    
    Args:
        args: Command line arguments. Uses sys.argv if None.
        
    Returns:
        Exit code (0 for success, non-zero for errors).
    """
    if args is None:
        args = sys.argv[1:]
    
    print(f"Starting application with args: {args}")
    
    # Application logic would go here
    return 0

if __name__ == "__main__":
    sys.exit(main())
"#,
                    "Main entry point",
                ));

                files.push(WorkspaceFile::new(
                    "requirements.txt",
                    "# Project dependencies\npytest>=7.0.0\nflake8>=6.0.0\n",
                    "Python dependencies",
                ));

                files.push(WorkspaceFile::new(
                    "tests/test_main.py",
                    r#""""Tests for main module."""

import pytest
from main import main

def test_main_returns_zero():
    """Test that main returns 0 on success."""
    result = main([])
    assert result == 0

def test_main_with_args():
    """Test that main handles arguments."""
    result = main(["--help"])
    assert result == 0
"#,
                    "Unit tests",
                ));
            }
            ProgrammingLanguage::Rust => {
                files.push(WorkspaceFile::new(
                    "src/main.rs",
                    r#"//! Main entry point for the application.

use std::env;
use std::process::ExitCode;

/// Main function that starts the application.
fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    
    println!("Starting application with args: {:?}", args);
    
    // Application logic would go here
    
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
        // Basic test placeholder
        assert!(true);
    }
}
"#,
                    "Main entry point",
                ));

                files.push(WorkspaceFile::new(
                    "Cargo.toml",
                    format!(
                        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]

[dev-dependencies]
"#,
                        params.project_name.replace('-', "_")
                    ),
                    "Cargo manifest",
                ));
            }
            ProgrammingLanguage::JavaScript | ProgrammingLanguage::TypeScript => {
                let ext = if params.language == ProgrammingLanguage::TypeScript {
                    "ts"
                } else {
                    "js"
                };

                files.push(WorkspaceFile::new(
                    format!("src/index.{}", ext),
                    r#"/**
 * Main entry point for the application.
 */

/**
 * Main function that starts the application.
 * @param {string[]} args - Command line arguments
 * @returns {number} Exit code
 */
function main(args) {
    console.log(`Starting application with args: ${args}`);
    
    // Application logic would go here
    
    return 0;
}

// Run if called directly
if (require.main === module) {
    const args = process.argv.slice(2);
    process.exit(main(args));
}

module.exports = { main };
"#,
                    "Main entry point",
                ));

                files.push(WorkspaceFile::new(
                    "package.json",
                    format!(
                        r#"{{
  "name": "{}",
  "version": "1.0.0",
  "description": "{}",
  "main": "src/index.{}",
  "scripts": {{
    "start": "node src/index.{}",
    "test": "jest"
  }},
  "devDependencies": {{
    "jest": "^29.0.0"
  }}
}}
"#,
                        params.project_name, params.description, ext, ext
                    ),
                    "Package manifest",
                ));
            }
            _ => {
                // Generate a generic README for other languages
                files.push(WorkspaceFile::new(
                    "README.md",
                    format!(
                        "# {}\n\n{}\n\n## Language: {}\n",
                        params.project_name,
                        params.description,
                        params.language.display_name()
                    ),
                    "Project documentation",
                ));
            }
        }

        // Always add a README
        if !files.iter().any(|f| f.path == "README.md") {
            files.push(WorkspaceFile::new(
                "README.md",
                format!(
                    "# {}\n\n{}\n\n## Getting Started\n\nSee build instructions.\n",
                    params.project_name, params.description
                ),
                "Project documentation",
            ));
        }

        Ok(files)
    }

    /// Injects vulnerabilities into the workspace.
    async fn inject_vulnerabilities(
        &self,
        params: &ProjectParameters,
        _files: &[WorkspaceFile],
        event_tx: &mpsc::Sender<WorkspacePipelineEvent>,
    ) -> AgentResult<Vec<InjectedVulnerability>> {
        // For now, return a placeholder vulnerability
        // Full implementation would use VulnerabilityInjectorAgent

        let vuln = InjectedVulnerability::new(
            "placeholder",
            match params.language {
                ProgrammingLanguage::Python => "main.py",
                ProgrammingLanguage::Rust => "src/main.rs",
                ProgrammingLanguage::JavaScript | ProgrammingLanguage::TypeScript => "src/index.js",
                _ => "main.txt",
            },
            "Placeholder vulnerability for demonstration",
        )
        .with_severity(5)
        .with_remediation("Review and fix any security issues");

        self.send_event(
            event_tx,
            WorkspacePipelineEvent::injection_progress(&vuln.vulnerability_type, &vuln.file_path),
        )
        .await;

        Ok(vec![vuln])
    }

    /// Validates the workspace is correct and solvable.
    async fn validate_workspace(
        &self,
        _params: &ProjectParameters,
        files: &[WorkspaceFile],
    ) -> AgentResult<ValidationResult> {
        let mut issues = Vec::new();

        // Basic validation checks
        if files.is_empty() {
            issues.push("No files generated".to_string());
        }

        let total_loc: usize = files.iter().map(|f| f.line_count()).sum();
        if total_loc < 10 {
            issues.push("Insufficient code generated (less than 10 lines)".to_string());
        }

        // Check for placeholder content
        for file in files {
            if file.content.contains("TODO") || file.content.contains("FIXME") {
                issues.push(format!(
                    "File {} contains TODO/FIXME placeholders",
                    file.path
                ));
            }
        }

        Ok(ValidationResult {
            passed: issues.is_empty(),
            issues,
        })
    }

    /// Runs final multi-agent review.
    async fn run_review(
        &self,
        _params: &ProjectParameters,
        _files: &[WorkspaceFile],
        _vulnerabilities: &[InjectedVulnerability],
    ) -> AgentResult<(HashMap<String, f64>, Vec<String>)> {
        // For now, return placeholder scores
        // Full implementation would run multiple agent reviews

        let mut scores = HashMap::new();
        scores.insert("code_quality".to_string(), 0.8);
        scores.insert("completeness".to_string(), 0.9);
        scores.insert("vulnerability_realism".to_string(), 0.7);
        scores.insert("solvability".to_string(), 0.85);

        let comments = vec![
            "Code structure follows best practices".to_string(),
            "Vulnerability injection points are realistic".to_string(),
        ];

        Ok((scores, comments))
    }

    /// Sends an event through the channel.
    async fn send_event(
        &self,
        event_tx: &mpsc::Sender<WorkspacePipelineEvent>,
        event: WorkspacePipelineEvent,
    ) {
        let _ = event_tx.send(event).await;
    }
}

/// Internal project parameters from debate results.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ProjectParameters {
    project_name: String,
    language: ProgrammingLanguage,
    category: String,
    project_type: String,
    description: String,
    complexity: String,
}

impl ProjectParameters {
    /// Creates default parameters for a language.
    fn default_for(language: ProgrammingLanguage, category_hint: Option<&str>) -> Self {
        Self {
            project_name: format!("{}-sample", language.display_name().to_lowercase()),
            language,
            category: category_hint.unwrap_or("general").to_string(),
            project_type: "cli".to_string(),
            description: format!("A sample {} project", language.display_name()),
            complexity: "simple".to_string(),
        }
    }
}

/// Internal validation result.
struct ValidationResult {
    passed: bool,
    issues: Vec<String>,
}

// ============================================================================
// Builder Pattern
// ============================================================================

/// Builder for creating a WorkspaceOrchestrator with fluent API.
pub struct WorkspaceOrchestratorBuilder {
    llm_client: Option<Arc<dyn LlmProvider>>,
    config: WorkspaceOrchestratorConfig,
}

impl WorkspaceOrchestratorBuilder {
    /// Creates a new builder with default configuration.
    pub fn new() -> Self {
        Self {
            llm_client: None,
            config: WorkspaceOrchestratorConfig::default(),
        }
    }

    /// Sets the LLM client.
    pub fn llm_client(mut self, client: Arc<dyn LlmProvider>) -> Self {
        self.llm_client = Some(client);
        self
    }

    /// Sets the number of debate rounds.
    pub fn debate_rounds(mut self, rounds: u32) -> Self {
        self.config.debate_config.debate_rounds = rounds.max(1);
        self
    }

    /// Sets the consensus threshold.
    pub fn consensus_threshold(mut self, threshold: f64) -> Self {
        self.config.debate_config.consensus_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Enables or disables the debate phase.
    pub fn enable_debate(mut self, enabled: bool) -> Self {
        self.config.enable_debate = enabled;
        self
    }

    /// Enables or disables vulnerability injection.
    pub fn enable_injection(mut self, enabled: bool) -> Self {
        self.config.enable_injection = enabled;
        self
    }

    /// Enables or disables final review.
    pub fn enable_review(mut self, enabled: bool) -> Self {
        self.config.enable_review = enabled;
        self
    }

    /// Sets the maximum files to generate.
    pub fn max_files(mut self, max: usize) -> Self {
        self.config.max_files = max.max(1);
        self
    }

    /// Sets the target lines of code.
    pub fn target_loc(mut self, loc: usize) -> Self {
        self.config.target_loc = loc.max(100);
        self
    }

    /// Builds the WorkspaceOrchestrator.
    pub fn build(self) -> AgentResult<WorkspaceOrchestrator> {
        let llm_client = self
            .llm_client
            .ok_or_else(|| AgentError::ConfigurationError("LLM client is required".to_string()))?;

        Ok(WorkspaceOrchestrator::new(llm_client, self.config))
    }
}

impl Default for WorkspaceOrchestratorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::super::debate_orchestrator::ConsensusMechanism;
    use super::*;
    use crate::llm::{Choice, GenerationResponse, Usage};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// Mock LLM provider for testing.
    struct MockLlmProvider {
        responses: Mutex<Vec<String>>,
        call_count: AtomicUsize,
    }

    impl MockLlmProvider {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: Mutex::new(responses),
                call_count: AtomicUsize::new(0),
            }
        }

        fn single_response(response: String) -> Self {
            Self::new(vec![response])
        }
    }

    #[async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn generate(
            &self,
            _request: crate::llm::GenerationRequest,
        ) -> Result<GenerationResponse, crate::error::LlmError> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            let responses = self.responses.lock().expect("lock not poisoned");
            let content = responses
                .get(idx)
                .cloned()
                .unwrap_or_else(|| responses.last().cloned().unwrap_or_default());

            Ok(GenerationResponse {
                id: format!("mock-{}", idx),
                model: "mock-model".to_string(),
                choices: vec![Choice {
                    index: 0,
                    message: crate::llm::Message::assistant(content),
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

    fn mock_debate_response() -> String {
        r#"{
            "claim": "We should build a REST API",
            "evidence": ["Common use case", "Easy to test"],
            "conclusion": "Build an API project",
            "confidence": 0.85,
            "acknowledged_weaknesses": [],
            "responses_to_others": []
        }"#
        .to_string()
    }

    #[test]
    fn test_config_defaults() {
        let config = WorkspaceOrchestratorConfig::default();
        assert!(config.enable_debate);
        assert!(config.enable_injection);
        assert!(config.enable_review);
        assert_eq!(config.max_files, 20);
        assert_eq!(config.target_loc, 1000);
    }

    #[test]
    fn test_config_builder() {
        let config = WorkspaceOrchestratorConfig::new()
            .with_debate(false)
            .with_injection(true)
            .with_max_files(10)
            .with_target_loc(500);

        assert!(!config.enable_debate);
        assert!(config.enable_injection);
        assert_eq!(config.max_files, 10);
        assert_eq!(config.target_loc, 500);
    }

    #[test]
    fn test_pipeline_stage_display() {
        assert_eq!(format!("{}", WorkspacePipelineStage::Debate), "Debate");
        assert_eq!(
            format!("{}", WorkspacePipelineStage::Generation),
            "Generation"
        );
        assert_eq!(
            format!("{}", WorkspacePipelineStage::Injection),
            "Injection"
        );
        assert_eq!(
            format!("{}", WorkspacePipelineStage::Validation),
            "Validation"
        );
    }

    #[test]
    fn test_workspace_file() {
        let file = WorkspaceFile::new("test.py", "line1\nline2\nline3", "Test file");
        assert_eq!(file.line_count(), 3);
        assert_eq!(file.path, "test.py");
    }

    #[test]
    fn test_injected_vulnerability() {
        let vuln = InjectedVulnerability::new("sql_injection", "app.py", "SQL injection in query")
            .with_severity(9)
            .with_line_numbers(vec![42, 43])
            .with_remediation("Use parameterized queries");

        assert_eq!(vuln.vulnerability_type, "sql_injection");
        assert_eq!(vuln.severity, 9);
        assert_eq!(vuln.line_numbers, vec![42, 43]);
    }

    #[test]
    fn test_workspace_result_builder() {
        let result =
            GeneratedWorkspaceResult::new("test-project", ProgrammingLanguage::Python, "api")
                .with_description("A test project")
                .with_files(vec![WorkspaceFile::new("main.py", "print('hi')", "Main")]);

        assert_eq!(result.project_name, "test-project");
        assert_eq!(result.language, ProgrammingLanguage::Python);
        assert_eq!(result.files.len(), 1);
    }

    #[test]
    fn test_orchestrator_builder_missing_llm() {
        let result = WorkspaceOrchestratorBuilder::new().debate_rounds(3).build();

        assert!(result.is_err());
        match result {
            Err(AgentError::ConfigurationError(msg)) => {
                assert!(msg.contains("LLM client"));
            }
            _ => panic!("Expected ConfigurationError"),
        }
    }

    #[test]
    fn test_project_parameters_default() {
        let params = ProjectParameters::default_for(ProgrammingLanguage::Python, Some("web-api"));
        assert_eq!(params.language, ProgrammingLanguage::Python);
        assert_eq!(params.category, "web-api");
    }

    #[tokio::test]
    async fn test_generate_basic_structure_python() {
        let mock_llm = Arc::new(MockLlmProvider::single_response("".to_string()));
        let orchestrator = WorkspaceOrchestrator::with_defaults(mock_llm);

        let params = ProjectParameters {
            project_name: "test-project".to_string(),
            language: ProgrammingLanguage::Python,
            category: "cli".to_string(),
            project_type: "cli".to_string(),
            description: "Test project".to_string(),
            complexity: "simple".to_string(),
        };

        let files = orchestrator
            .generate_basic_structure(&params)
            .expect("should generate files");

        assert!(!files.is_empty());
        assert!(files.iter().any(|f| f.path == "main.py"));
        assert!(files.iter().any(|f| f.path == "requirements.txt"));
    }

    #[tokio::test]
    async fn test_generate_basic_structure_rust() {
        let mock_llm = Arc::new(MockLlmProvider::single_response("".to_string()));
        let orchestrator = WorkspaceOrchestrator::with_defaults(mock_llm);

        let params = ProjectParameters {
            project_name: "test-project".to_string(),
            language: ProgrammingLanguage::Rust,
            category: "cli".to_string(),
            project_type: "cli".to_string(),
            description: "Test project".to_string(),
            complexity: "simple".to_string(),
        };

        let files = orchestrator
            .generate_basic_structure(&params)
            .expect("should generate files");

        assert!(!files.is_empty());
        assert!(files.iter().any(|f| f.path == "src/main.rs"));
        assert!(files.iter().any(|f| f.path == "Cargo.toml"));
    }

    #[tokio::test]
    async fn test_validate_workspace_empty() {
        let mock_llm = Arc::new(MockLlmProvider::single_response("".to_string()));
        let orchestrator = WorkspaceOrchestrator::with_defaults(mock_llm);

        let params = ProjectParameters::default_for(ProgrammingLanguage::Python, None);
        let files: Vec<WorkspaceFile> = vec![];

        let result = orchestrator
            .validate_workspace(&params, &files)
            .await
            .expect("validation should run");

        assert!(!result.passed);
        assert!(result.issues.iter().any(|i| i.contains("No files")));
    }

    #[tokio::test]
    async fn test_full_pipeline_no_debate() {
        // Create enough responses for the pipeline stages
        let mut responses = Vec::new();
        for _ in 0..20 {
            responses.push(mock_debate_response());
        }

        let mock_llm = Arc::new(MockLlmProvider::new(responses));
        let orchestrator = WorkspaceOrchestratorBuilder::new()
            .llm_client(mock_llm)
            .enable_debate(false) // Skip debate for faster test
            .enable_injection(true)
            .enable_review(false)
            .build()
            .expect("should build");

        let (event_tx, mut event_rx) = mpsc::channel(100);

        let result = orchestrator
            .generate_workspace(ProgrammingLanguage::Python, Some("cli"), event_tx)
            .await
            .expect("pipeline should complete");

        assert!(!result.files.is_empty());
        assert!(result.total_loc > 0);

        // Verify events were emitted
        event_rx.close();
        let mut events = Vec::new();
        while let Some(event) = event_rx.recv().await {
            events.push(event);
        }
        assert!(!events.is_empty());

        // Check for completion event
        let has_completed = events
            .iter()
            .any(|e| matches!(e, WorkspacePipelineEvent::PipelineCompleted { .. }));
        assert!(has_completed, "Should have PipelineCompleted event");
    }

    #[test]
    fn test_event_constructors() {
        let started = WorkspacePipelineEvent::pipeline_started(
            "ws-123",
            ProgrammingLanguage::Python,
            Some("api".to_string()),
        );
        match started {
            WorkspacePipelineEvent::PipelineStarted {
                workspace_id,
                language,
                ..
            } => {
                assert_eq!(workspace_id, "ws-123");
                assert_eq!(language, ProgrammingLanguage::Python);
            }
            _ => panic!("Expected PipelineStarted event"),
        }

        let stage_started = WorkspacePipelineEvent::stage_started(WorkspacePipelineStage::Debate);
        match stage_started {
            WorkspacePipelineEvent::StageStarted { stage, .. } => {
                assert_eq!(stage, WorkspacePipelineStage::Debate);
            }
            _ => panic!("Expected StageStarted event"),
        }
    }

    #[test]
    fn test_debate_outcome_from_consensus() {
        let consensus = ConsensusResult::new(
            DebateTopic::ProjectType,
            true,
            Some("Build REST API".to_string()),
            0.85,
            vec![],
            ConsensusMechanism::SimpleMajority,
            1000,
        );

        let outcome = DebateOutcome::from_consensus(&consensus);
        assert!(outcome.consensus_reached);
        assert_eq!(outcome.decision, "Build REST API");
        assert!((outcome.score - 0.85).abs() < 0.01);
    }
}
