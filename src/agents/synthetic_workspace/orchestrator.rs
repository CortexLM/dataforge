//! Main orchestrator for synthetic workspace generation.
//!
//! This module provides the `SyntheticWorkspaceOrchestrator` that coordinates
//! the entire pipeline for generating workspaces with injected vulnerabilities.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use tar::Builder as TarBuilder;
use tempfile;
use tokio::fs;
use tokio::sync::mpsc;
use tracing::{info, instrument};
use uuid::Uuid;

use crate::error::GeneratorError;
use crate::llm::{GenerationRequest, LlmProvider, Message};
use crate::workspace::cleaner::WorkspaceCleaner;

use super::config::{DifficultyLevel, LanguageTarget, ProjectCategory, SyntheticWorkspaceConfig};
use super::debate::{DebateAgent, DebateOrchestrator, DebateSession, DebateTopic};
use super::templates::WorkspaceTemplate;
use super::types::{
    FileContent, FileType, GeneratedFile, InjectedVulnerability, ProjectSpec, ProjectStructure,
};

// ============================================================================
// Events
// ============================================================================

/// Stages in the generation pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationStage {
    /// Planning and design phase.
    Planning,
    /// Multi-agent debate phase.
    Debate,
    /// Code generation phase.
    CodeGeneration,
    /// Vulnerability injection phase.
    VulnerabilityInjection,
    /// Code cleaning phase.
    Cleaning,
    /// Validation phase.
    Validation,
    /// Export phase.
    Export,
}

impl std::fmt::Display for GenerationStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Planning => write!(f, "Planning"),
            Self::Debate => write!(f, "Debate"),
            Self::CodeGeneration => write!(f, "Code Generation"),
            Self::VulnerabilityInjection => write!(f, "Vulnerability Injection"),
            Self::Cleaning => write!(f, "Cleaning"),
            Self::Validation => write!(f, "Validation"),
            Self::Export => write!(f, "Export"),
        }
    }
}

/// Events emitted during workspace generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GenerationEvent {
    /// Generation started.
    Started {
        workspace_id: String,
        language: LanguageTarget,
        category: ProjectCategory,
        difficulty: DifficultyLevel,
        timestamp: DateTime<Utc>,
    },
    /// Stage started.
    StageStarted {
        stage: GenerationStage,
        timestamp: DateTime<Utc>,
    },
    /// Stage completed.
    StageCompleted {
        stage: GenerationStage,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
    /// Debate round completed.
    DebateRound {
        topic: String,
        round: u32,
        agreement_score: f64,
        emerging_consensus: Option<String>,
        timestamp: DateTime<Utc>,
    },
    /// Debate completed.
    DebateCompleted {
        topic: String,
        consensus_reached: bool,
        decision: Option<String>,
        timestamp: DateTime<Utc>,
    },
    /// File generated.
    FileGenerated {
        path: String,
        file_type: FileType,
        lines: usize,
        timestamp: DateTime<Utc>,
    },
    /// Vulnerability injected.
    VulnerabilityInjected {
        vulnerability_type: String,
        file: String,
        severity: u8,
        timestamp: DateTime<Utc>,
    },
    /// Cleaning completed.
    CleaningCompleted {
        files_cleaned: usize,
        patterns_removed: usize,
        timestamp: DateTime<Utc>,
    },
    /// Generation completed.
    Completed {
        workspace_id: String,
        file_count: usize,
        vulnerability_count: usize,
        total_loc: usize,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
    /// Generation failed.
    Failed {
        stage: GenerationStage,
        error: String,
        timestamp: DateTime<Utc>,
    },
}

impl GenerationEvent {
    pub fn started(
        workspace_id: &str,
        language: LanguageTarget,
        category: ProjectCategory,
        difficulty: DifficultyLevel,
    ) -> Self {
        Self::Started {
            workspace_id: workspace_id.to_string(),
            language,
            category,
            difficulty,
            timestamp: Utc::now(),
        }
    }

    pub fn stage_started(stage: GenerationStage) -> Self {
        Self::StageStarted {
            stage,
            timestamp: Utc::now(),
        }
    }

    pub fn stage_completed(stage: GenerationStage, duration_ms: u64) -> Self {
        Self::StageCompleted {
            stage,
            duration_ms,
            timestamp: Utc::now(),
        }
    }

    pub fn file_generated(path: &str, file_type: FileType, lines: usize) -> Self {
        Self::FileGenerated {
            path: path.to_string(),
            file_type,
            lines,
            timestamp: Utc::now(),
        }
    }

    pub fn vulnerability_injected(vulnerability_type: &str, file: &str, severity: u8) -> Self {
        Self::VulnerabilityInjected {
            vulnerability_type: vulnerability_type.to_string(),
            file: file.to_string(),
            severity,
            timestamp: Utc::now(),
        }
    }

    pub fn completed(
        workspace_id: &str,
        file_count: usize,
        vulnerability_count: usize,
        total_loc: usize,
        duration_ms: u64,
    ) -> Self {
        Self::Completed {
            workspace_id: workspace_id.to_string(),
            file_count,
            vulnerability_count,
            total_loc,
            duration_ms,
            timestamp: Utc::now(),
        }
    }

    pub fn failed(stage: GenerationStage, error: &str) -> Self {
        Self::Failed {
            stage,
            error: error.to_string(),
            timestamp: Utc::now(),
        }
    }
}

// ============================================================================
// Generated Workspace
// ============================================================================

/// A complete generated workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntheticWorkspace {
    /// Unique identifier.
    pub id: String,
    /// Project specification.
    pub spec: ProjectSpec,
    /// Generated files.
    pub files: Vec<GeneratedFile>,
    /// Injected vulnerabilities (hidden from task).
    pub vulnerabilities: Vec<InjectedVulnerability>,
    /// Debate sessions that led to decisions.
    pub debates: Vec<DebateSession>,
    /// Task prompt for the benchmark.
    pub task_prompt: String,
    /// Build/run instructions.
    pub build_instructions: String,
    /// Test instructions.
    pub test_instructions: String,
    /// Canary token for anti-hardcoding.
    pub canary_token: String,
    /// Total lines of code.
    pub total_loc: usize,
    /// When this workspace was generated.
    pub created_at: DateTime<Utc>,
}

impl SyntheticWorkspace {
    /// Creates a new synthetic workspace.
    pub fn new(spec: ProjectSpec) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            spec,
            files: Vec::new(),
            vulnerabilities: Vec::new(),
            debates: Vec::new(),
            task_prompt: String::new(),
            build_instructions: String::new(),
            test_instructions: String::new(),
            canary_token: Uuid::new_v4().to_string(),
            total_loc: 0,
            created_at: Utc::now(),
        }
    }

    /// Adds a file to the workspace.
    pub fn add_file(&mut self, file: GeneratedFile) {
        self.total_loc += file.line_count();
        self.files.push(file);
    }

    /// Adds a vulnerability record.
    pub fn add_vulnerability(&mut self, vuln: InjectedVulnerability) {
        self.vulnerabilities.push(vuln);
    }

    /// Adds a debate session.
    pub fn add_debate(&mut self, debate: DebateSession) {
        self.debates.push(debate);
    }

    /// Gets a file by path.
    pub fn get_file(&self, path: &str) -> Option<&GeneratedFile> {
        self.files.iter().find(|f| f.path.to_string_lossy() == path)
    }

    /// Gets a mutable file by path.
    pub fn get_file_mut(&mut self, path: &str) -> Option<&mut GeneratedFile> {
        self.files
            .iter_mut()
            .find(|f| f.path.to_string_lossy() == path)
    }

    /// Exports the workspace to a directory.
    pub async fn export_to_directory(&self, output_dir: &Path) -> Result<PathBuf, GeneratorError> {
        let workspace_dir = output_dir.join(&self.id);
        fs::create_dir_all(&workspace_dir).await?;

        // Write all files
        for file in &self.files {
            // Validate that file path doesn't escape workspace via ../
            let normalized_path = file
                .path
                .components()
                .filter(|c| !matches!(c, std::path::Component::ParentDir))
                .collect::<PathBuf>();

            // Additional check: ensure the final path is within workspace_dir
            let file_path = workspace_dir.join(&normalized_path);
            let canonical_workspace = workspace_dir
                .canonicalize()
                .unwrap_or(workspace_dir.clone());

            // Security: Ensure file_path starts with workspace_dir
            if !file_path.starts_with(&canonical_workspace)
                && !file_path.starts_with(&workspace_dir)
            {
                return Err(GeneratorError::InvalidParameter(format!(
                    "Path traversal attempt detected in file: {}",
                    file.path.display()
                )));
            }

            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).await?;
            }

            match &file.content {
                FileContent::Text(content) => {
                    fs::write(&file_path, content).await?;
                }
                FileContent::Binary(data) => {
                    fs::write(&file_path, data).await?;
                }
            }
        }

        // Write prompt.md
        let prompt_path = workspace_dir.join("prompt.md");
        fs::write(&prompt_path, &self.task_prompt).await?;

        // Write workspace.yaml
        let workspace_yaml = serde_yaml::to_string(&self).map_err(|e| {
            GeneratorError::InvalidParameter(format!("YAML serialization failed: {}", e))
        })?;
        fs::write(workspace_dir.join("workspace.yaml"), workspace_yaml).await?;

        // Write hidden solution info
        let solution_dir = workspace_dir.join(".solution");
        fs::create_dir_all(&solution_dir).await?;

        let vulns_yaml = serde_yaml::to_string(&self.vulnerabilities).map_err(|e| {
            GeneratorError::InvalidParameter(format!("YAML serialization failed: {}", e))
        })?;
        fs::write(solution_dir.join("vulnerabilities.yaml"), vulns_yaml).await?;

        // Write canary
        fs::write(workspace_dir.join(".canary"), &self.canary_token).await?;

        Ok(workspace_dir)
    }

    /// Exports the workspace to a tar.gz archive.
    pub async fn export_to_zip(&self, output_path: &Path) -> Result<PathBuf, GeneratorError> {
        // Use secure temp directory creation instead of predictable path
        let temp_dir = tempfile::Builder::new()
            .prefix("workspace-export-")
            .tempdir()
            .map_err(GeneratorError::Io)?;

        self.export_to_directory(temp_dir.path()).await?;

        let workspace_dir = temp_dir.path().join(&self.id);

        // Create tar.gz
        let file = std::fs::File::create(output_path).map_err(GeneratorError::Io)?;
        let enc = GzEncoder::new(file, Compression::default());
        let mut tar = TarBuilder::new(enc);

        // Get exclude patterns based on language
        let exclude_patterns = self.spec.language.artifact_patterns();

        // Add files to archive
        self.add_directory_to_tar(&mut tar, &workspace_dir, &workspace_dir, exclude_patterns)?;

        tar.finish().map_err(GeneratorError::Io)?;

        // temp_dir will be automatically cleaned up when dropped

        Ok(output_path.to_path_buf())
    }

    /// Recursively adds a directory to a tar archive.
    fn add_directory_to_tar<W: std::io::Write>(
        &self,
        tar: &mut TarBuilder<W>,
        dir: &Path,
        base: &Path,
        exclude_patterns: &[&str],
    ) -> Result<(), GeneratorError> {
        for entry in std::fs::read_dir(dir).map_err(GeneratorError::Io)? {
            let entry = entry.map_err(GeneratorError::Io)?;
            let path = entry.path();
            let relative = path.strip_prefix(base).unwrap_or(&path);

            // Check if this path matches any exclude pattern
            let relative_str = relative.to_string_lossy();
            let should_exclude = exclude_patterns.iter().any(|pattern| {
                let pattern = pattern.trim_end_matches("/**");
                relative_str.starts_with(pattern) || relative_str.contains(&format!("/{}", pattern))
            });

            if should_exclude {
                continue;
            }

            if path.is_dir() {
                self.add_directory_to_tar(tar, &path, base, exclude_patterns)?;
            } else {
                tar.append_path_with_name(&path, relative)
                    .map_err(GeneratorError::Io)?;
            }
        }
        Ok(())
    }
}

// ============================================================================
// Orchestrator
// ============================================================================

/// Main orchestrator for synthetic workspace generation.
pub struct SyntheticWorkspaceOrchestrator {
    /// LLM client for generation.
    llm: Arc<dyn LlmProvider>,
    /// Configuration.
    config: SyntheticWorkspaceConfig,
    /// Debate orchestrator.
    debate_orchestrator: DebateOrchestrator,
    /// Workspace cleaner.
    cleaner: WorkspaceCleaner,
}

impl SyntheticWorkspaceOrchestrator {
    /// Creates a new orchestrator.
    pub fn new(llm: Arc<dyn LlmProvider>, config: SyntheticWorkspaceConfig) -> Self {
        let debate_orchestrator =
            DebateOrchestrator::new(Arc::clone(&llm), config.debate_model.clone())
                .with_temperature(config.debate_temperature)
                .with_consensus_threshold(config.consensus_threshold);

        Self {
            llm,
            config,
            debate_orchestrator,
            cleaner: WorkspaceCleaner::new(),
        }
    }

    /// Returns the configuration.
    pub fn config(&self) -> &SyntheticWorkspaceConfig {
        &self.config
    }

    /// Generates a complete synthetic workspace.
    #[instrument(skip(self, event_tx))]
    pub async fn generate(
        &self,
        event_tx: mpsc::Sender<GenerationEvent>,
    ) -> Result<SyntheticWorkspace, GeneratorError> {
        let start_time = std::time::Instant::now();
        let workspace_id = Uuid::new_v4().to_string();

        info!(
            workspace_id = %workspace_id,
            language = %self.config.language,
            category = %self.config.category,
            difficulty = %self.config.difficulty,
            "Starting synthetic workspace generation"
        );

        // Emit started event
        self.send_event(
            &event_tx,
            GenerationEvent::started(
                &workspace_id,
                self.config.language,
                self.config.category,
                self.config.difficulty,
            ),
        )
        .await;

        // Stage 1: Planning
        self.send_event(
            &event_tx,
            GenerationEvent::stage_started(GenerationStage::Planning),
        )
        .await;
        let stage_start = std::time::Instant::now();

        let project_spec = self.plan_project().await?;

        self.send_event(
            &event_tx,
            GenerationEvent::stage_completed(
                GenerationStage::Planning,
                stage_start.elapsed().as_millis() as u64,
            ),
        )
        .await;

        // Create workspace
        let mut workspace = SyntheticWorkspace::new(project_spec);
        workspace.id = workspace_id.clone();

        // Stage 2: Multi-agent debate
        self.send_event(
            &event_tx,
            GenerationEvent::stage_started(GenerationStage::Debate),
        )
        .await;
        let stage_start = std::time::Instant::now();

        let debates = self.conduct_debates(&workspace.spec, &event_tx).await?;
        for debate in debates {
            workspace.add_debate(debate);
        }

        self.send_event(
            &event_tx,
            GenerationEvent::stage_completed(
                GenerationStage::Debate,
                stage_start.elapsed().as_millis() as u64,
            ),
        )
        .await;

        // Stage 3: Code generation
        self.send_event(
            &event_tx,
            GenerationEvent::stage_started(GenerationStage::CodeGeneration),
        )
        .await;
        let stage_start = std::time::Instant::now();

        let files = self
            .generate_code(&workspace.spec, &workspace.debates, &event_tx)
            .await?;
        for file in files {
            workspace.add_file(file);
        }

        self.send_event(
            &event_tx,
            GenerationEvent::stage_completed(
                GenerationStage::CodeGeneration,
                stage_start.elapsed().as_millis() as u64,
            ),
        )
        .await;

        // Stage 4: Vulnerability injection
        self.send_event(
            &event_tx,
            GenerationEvent::stage_started(GenerationStage::VulnerabilityInjection),
        )
        .await;
        let stage_start = std::time::Instant::now();

        let vulnerabilities = self
            .inject_vulnerabilities(&mut workspace, &event_tx)
            .await?;
        for vuln in vulnerabilities {
            workspace.add_vulnerability(vuln);
        }

        self.send_event(
            &event_tx,
            GenerationEvent::stage_completed(
                GenerationStage::VulnerabilityInjection,
                stage_start.elapsed().as_millis() as u64,
            ),
        )
        .await;

        // Stage 5: Cleaning
        if self.config.auto_clean {
            self.send_event(
                &event_tx,
                GenerationEvent::stage_started(GenerationStage::Cleaning),
            )
            .await;
            let stage_start = std::time::Instant::now();

            let (files_cleaned, patterns_removed) = self.clean_workspace(&mut workspace)?;

            self.send_event(
                &event_tx,
                GenerationEvent::CleaningCompleted {
                    files_cleaned,
                    patterns_removed,
                    timestamp: Utc::now(),
                },
            )
            .await;

            self.send_event(
                &event_tx,
                GenerationEvent::stage_completed(
                    GenerationStage::Cleaning,
                    stage_start.elapsed().as_millis() as u64,
                ),
            )
            .await;
        }

        // Stage 6: Generate task prompt
        workspace.task_prompt = self.generate_task_prompt(&workspace).await?;
        workspace.build_instructions = self.generate_build_instructions(&workspace.spec);
        workspace.test_instructions = self.generate_test_instructions(&workspace.spec);

        // Emit completed event
        let total_duration = start_time.elapsed().as_millis() as u64;
        self.send_event(
            &event_tx,
            GenerationEvent::completed(
                &workspace.id,
                workspace.files.len(),
                workspace.vulnerabilities.len(),
                workspace.total_loc,
                total_duration,
            ),
        )
        .await;

        info!(
            workspace_id = %workspace.id,
            files = workspace.files.len(),
            vulnerabilities = workspace.vulnerabilities.len(),
            loc = workspace.total_loc,
            duration_ms = total_duration,
            "Workspace generation completed"
        );

        Ok(workspace)
    }

    /// Plans the project structure and specification.
    async fn plan_project(&self) -> Result<ProjectSpec, GeneratorError> {
        // Get template for this language/category
        let template = WorkspaceTemplate::get_template(self.config.language, self.config.category)
            .unwrap_or_else(WorkspaceTemplate::python_flask_api);

        let spec = ProjectSpec::new(
            format!(
                "{}-{}",
                self.config
                    .category
                    .to_string()
                    .to_lowercase()
                    .replace(' ', "-"),
                Uuid::new_v4()
                    .to_string()
                    .split('-')
                    .next()
                    .unwrap_or("default")
            ),
            self.config.language,
        )
        .with_description(template.description.clone())
        .with_category(self.config.category)
        .with_difficulty(self.config.difficulty)
        .with_structure(
            ProjectStructure::new()
                .with_directories(template.directories.clone())
                .with_key_files(
                    template
                        .files
                        .iter()
                        .map(|f| f.path.clone())
                        .collect::<Vec<_>>(),
                ),
        )
        .with_dependencies(
            template
                .dependencies
                .iter()
                .map(|d| d.name.clone())
                .collect::<Vec<_>>(),
        );

        Ok(spec)
    }

    /// Conducts multi-agent debates for key decisions.
    async fn conduct_debates(
        &self,
        spec: &ProjectSpec,
        event_tx: &mpsc::Sender<GenerationEvent>,
    ) -> Result<Vec<DebateSession>, GeneratorError> {
        let mut debates = Vec::new();

        // Create debate agents
        let agents = vec![
            DebateAgent::architect(),
            DebateAgent::security_expert(),
            DebateAgent::developer(),
            DebateAgent::quality_analyst(),
        ];

        // Debate 1: Vulnerability selection
        let vuln_context = format!(
            "Project: {} ({}) with {} difficulty.\nCategory: {}.\nSelect {} vulnerabilities to inject.",
            spec.name,
            spec.language,
            spec.difficulty,
            spec.category,
            self.config.vulnerabilities.max_count
        );

        let vuln_debate = self
            .debate_orchestrator
            .conduct_debate(
                DebateTopic::VulnerabilitySelection,
                &vuln_context,
                agents.clone(),
                self.config.debate_rounds,
            )
            .await
            .map_err(|e| GeneratorError::Template(format!("Debate failed: {}", e)))?;

        // Emit debate events
        for round in &vuln_debate.rounds {
            self.send_event(
                event_tx,
                GenerationEvent::DebateRound {
                    topic: DebateTopic::VulnerabilitySelection.to_string(),
                    round: round.round_number,
                    agreement_score: round.agreement_score,
                    emerging_consensus: round.emerging_consensus.clone(),
                    timestamp: Utc::now(),
                },
            )
            .await;
        }

        self.send_event(
            event_tx,
            GenerationEvent::DebateCompleted {
                topic: DebateTopic::VulnerabilitySelection.to_string(),
                consensus_reached: vuln_debate.has_consensus(),
                decision: vuln_debate
                    .consensus
                    .as_ref()
                    .and_then(|c| c.position.clone()),
                timestamp: Utc::now(),
            },
        )
        .await;

        debates.push(vuln_debate);

        // Debate 2: Difficulty calibration
        let diff_context = format!(
            "Calibrate difficulty for {} project with {} target difficulty.\nShould include {} vulnerabilities.",
            spec.language,
            spec.difficulty,
            self.config.vulnerabilities.max_count
        );

        let diff_agents = vec![
            DebateAgent::difficulty_assessor(),
            DebateAgent::security_expert(),
            DebateAgent::developer(),
        ];

        let diff_debate = self
            .debate_orchestrator
            .conduct_debate(
                DebateTopic::DifficultyCalibration,
                &diff_context,
                diff_agents,
                self.config.debate_rounds,
            )
            .await
            .map_err(|e| GeneratorError::Template(format!("Debate failed: {}", e)))?;

        self.send_event(
            event_tx,
            GenerationEvent::DebateCompleted {
                topic: DebateTopic::DifficultyCalibration.to_string(),
                consensus_reached: diff_debate.has_consensus(),
                decision: diff_debate
                    .consensus
                    .as_ref()
                    .and_then(|c| c.position.clone()),
                timestamp: Utc::now(),
            },
        )
        .await;

        debates.push(diff_debate);

        Ok(debates)
    }

    /// Generates the code for the workspace.
    async fn generate_code(
        &self,
        spec: &ProjectSpec,
        debates: &[DebateSession],
        event_tx: &mpsc::Sender<GenerationEvent>,
    ) -> Result<Vec<GeneratedFile>, GeneratorError> {
        let template = WorkspaceTemplate::get_template(spec.language, spec.category)
            .unwrap_or_else(WorkspaceTemplate::python_flask_api);

        let mut files = Vec::new();

        // Build context from debates
        let debate_context = debates
            .iter()
            .filter_map(|d| d.consensus.as_ref())
            .map(|c| c.summary.clone())
            .collect::<Vec<_>>()
            .join("\n");

        // Generate each file using LLM
        for file_template in &template.files {
            let file = self
                .generate_file(
                    spec,
                    &file_template.path,
                    &file_template.description,
                    &template,
                    &debate_context,
                )
                .await?;

            self.send_event(
                event_tx,
                GenerationEvent::file_generated(
                    &file.path.to_string_lossy(),
                    file.file_type,
                    file.line_count(),
                ),
            )
            .await;

            files.push(file);
        }

        // Generate package file
        let package_file = self.generate_package_file(spec, &template)?;
        files.push(package_file);

        Ok(files)
    }

    /// Generates a single file.
    async fn generate_file(
        &self,
        spec: &ProjectSpec,
        path: &str,
        description: &str,
        template: &WorkspaceTemplate,
        debate_context: &str,
    ) -> Result<GeneratedFile, GeneratorError> {
        // Calculate target lines based on difficulty level
        let (min_lines, max_lines) = spec.difficulty.loc_range();
        let total_files = template.files.len();
        let target_total_lines = (min_lines + max_lines) / 2;
        let target_lines_per_file = target_total_lines / total_files.max(1);
        // For 5000+ LOC, each file should be 200-500 lines
        let target_lines = target_lines_per_file.clamp(150, 600);

        let language_name = spec.language.display_name();
        let framework = template.framework.as_deref().unwrap_or("none");
        let difficulty = format!("{}", spec.difficulty);

        let system_prompt = format!(
            r#"You are an expert {language_name} developer creating REALISTIC, production-quality enterprise code.

Your code must:
1. Look like actual production code - no placeholders, no TODO comments
2. Follow {language_name} best practices and conventions
3. Be complete, functional, and COMPREHENSIVE
4. Include proper error handling with custom error types
5. Have realistic variable names and business logic
6. Include detailed inline documentation and module-level docs
7. NOT contain any comments about vulnerabilities, security issues, or TODOs
8. Target approximately {target_lines} lines of code for this file

You are creating code for: {template_desc}
Framework: {framework}
Target complexity: {difficulty} difficulty (requires substantial, production-ready implementation)

CRITICAL: Output ONLY the code. No explanations, no markdown fences, just the raw code.
IMPORTANT: Generate comprehensive, enterprise-grade code - this should be substantial production code, not minimal examples."#,
            language_name = language_name,
            target_lines = target_lines,
            template_desc = template.description,
            framework = framework,
            difficulty = difficulty
        );

        let user_prompt = format!(
            r#"Generate the complete, production-ready code for: {path}

Purpose: {description}

REQUIREMENTS:
- Generate approximately {target_lines} lines of code
- Include comprehensive implementation with all edge cases
- Add proper logging/tracing statements
- Include input validation and sanitization
- Implement proper error handling with Result types
- Add documentation comments for public items
- Include type definitions, constants, and helper functions as needed

Project context:
- Name: {name}
- Language: {language}
- Difficulty: {difficulty} (requires substantial implementation)
- Dependencies available: {deps}

Design decisions from team discussion:
{debate_context}

Generate realistic, production-quality code. Remember:
- No TODO/FIXME comments
- No placeholder implementations
- Complete, working code with FULL functionality
- Proper imports, types, and module structure
- Include comprehensive test cases or validation logic where appropriate"#,
            path = path,
            description = description,
            target_lines = target_lines,
            name = spec.name,
            language = spec.language.display_name(),
            difficulty = spec.difficulty,
            deps = spec.dependencies.join(", "),
            debate_context = if debate_context.is_empty() {
                "No specific decisions."
            } else {
                debate_context
            }
        );

        let request = GenerationRequest::new(
            &self.config.generation_model,
            vec![Message::system(&system_prompt), Message::user(&user_prompt)],
        )
        .with_temperature(self.config.generation_temperature)
        .with_max_tokens(self.config.max_generation_tokens);

        let response = self
            .llm
            .generate(request)
            .await
            .map_err(|e| GeneratorError::Template(format!("Code generation failed: {}", e)))?;

        let content = response
            .first_content()
            .ok_or_else(|| GeneratorError::Template("Empty response from LLM".to_string()))?;

        // Clean up any markdown fences
        let clean_content = self.clean_code_output(content);

        let file_type = if path.contains("test") || path.contains("spec") {
            FileType::Test
        } else if path.ends_with(".json")
            || path.ends_with(".yaml")
            || path.ends_with(".toml")
            || path.ends_with(".txt")
        {
            FileType::Configuration
        } else {
            FileType::Source
        };

        Ok(GeneratedFile::new(path, clean_content)
            .with_type(file_type)
            .with_description(description))
    }

    /// Generates the package file (requirements.txt, package.json, etc.).
    fn generate_package_file(
        &self,
        spec: &ProjectSpec,
        template: &WorkspaceTemplate,
    ) -> Result<GeneratedFile, GeneratorError> {
        let path = spec.language.package_file();
        let content = match spec.language {
            LanguageTarget::Python => {
                let deps: Vec<String> = template
                    .dependencies
                    .iter()
                    .map(|d| format!("{}{}", d.name, d.version))
                    .collect();
                deps.join("\n")
            }
            LanguageTarget::JavaScript | LanguageTarget::TypeScript => {
                let deps: HashMap<&str, &str> = template
                    .dependencies
                    .iter()
                    .filter(|d| !d.dev_only)
                    .map(|d| (d.name.as_str(), d.version.as_str()))
                    .collect();
                let dev_deps: HashMap<&str, &str> = template
                    .dependencies
                    .iter()
                    .filter(|d| d.dev_only)
                    .map(|d| (d.name.as_str(), d.version.as_str()))
                    .collect();

                serde_json::json!({
                    "name": spec.name,
                    "version": "1.0.0",
                    "description": spec.description,
                    "main": "src/index.js",
                    "scripts": {
                        "start": "node src/index.js",
                        "test": "jest"
                    },
                    "dependencies": deps,
                    "devDependencies": dev_deps
                })
                .to_string()
            }
            LanguageTarget::Rust => {
                format!(
                    r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
{}
"#,
                    spec.name.replace('-', "_"),
                    template
                        .dependencies
                        .iter()
                        .map(|d| format!(
                            "{} = \"{}\"",
                            d.name,
                            d.version
                                .trim_start_matches(['>', '=', '^'])
                        ))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            }
            LanguageTarget::Go => {
                format!("module {}\n\ngo 1.21\n", spec.name)
            }
            _ => String::new(),
        };

        Ok(GeneratedFile::config(path, content).with_description("Package dependencies"))
    }

    /// Injects vulnerabilities into the workspace.
    async fn inject_vulnerabilities(
        &self,
        workspace: &mut SyntheticWorkspace,
        event_tx: &mpsc::Sender<GenerationEvent>,
    ) -> Result<Vec<InjectedVulnerability>, GeneratorError> {
        let template =
            WorkspaceTemplate::get_template(workspace.spec.language, workspace.spec.category)
                .unwrap_or_else(WorkspaceTemplate::python_flask_api);

        let mut vulnerabilities = Vec::new();
        let vuln_count = self
            .config
            .vulnerabilities
            .max_count
            .min(template.vulnerability_patterns.len());

        // Clone the spec to avoid borrow issues
        let spec_clone = workspace.spec.clone();

        for (idx, pattern) in template
            .vulnerability_patterns
            .iter()
            .take(vuln_count)
            .enumerate()
        {
            // Find a target file
            let target_file = pattern
                .applicable_files
                .first()
                .cloned()
                .unwrap_or_default();

            if let Some(file) = workspace.get_file_mut(&target_file) {
                // Inject the vulnerability
                let injected = self
                    .inject_vulnerability_into_file(file, pattern, idx, &spec_clone)
                    .await?;

                self.send_event(
                    event_tx,
                    GenerationEvent::vulnerability_injected(
                        &injected.vulnerability_type,
                        &injected.file_path,
                        injected.severity,
                    ),
                )
                .await;

                vulnerabilities.push(injected);
            }
        }

        Ok(vulnerabilities)
    }

    /// Injects a single vulnerability into a file.
    async fn inject_vulnerability_into_file(
        &self,
        file: &mut GeneratedFile,
        pattern: &super::templates::VulnerabilityPattern,
        index: usize,
        spec: &ProjectSpec,
    ) -> Result<InjectedVulnerability, GeneratorError> {
        let original_content = file.text_content().unwrap_or_default().to_string();

        let system_prompt = format!(
            r#"You are a security expert injecting SUBTLE vulnerabilities into code for a security benchmark.

Your task is to modify the code to contain a {} vulnerability.

CRITICAL RULES:
1. The vulnerability must be SUBTLE - no obvious comments or markers
2. The code must still COMPILE and RUN without errors
3. Do NOT add any comments mentioning the vulnerability
4. Do NOT add TODO, FIXME, or any hint comments
5. The vulnerability should look like a natural mistake a developer might make
6. Make the vulnerability realistic and exploitable

Vulnerability description: {}

Output ONLY the modified code with the vulnerability. No explanations."#,
            pattern.vulnerability_type, pattern.description
        );

        let user_prompt = format!(
            r#"Inject a {} vulnerability into this {} code:

```
{}
```

Remember:
- NO comments mentioning the vulnerability
- Code must still work
- Make it subtle and realistic
- Output ONLY the modified code"#,
            pattern.vulnerability_type,
            spec.language.display_name(),
            original_content
        );

        let request = GenerationRequest::new(
            &self.config.generation_model,
            vec![Message::system(&system_prompt), Message::user(&user_prompt)],
        )
        .with_temperature(0.5)
        .with_max_tokens(self.config.max_generation_tokens);

        let response = self.llm.generate(request).await.map_err(|e| {
            GeneratorError::Template(format!("Vulnerability injection failed: {}", e))
        })?;

        let modified_content = response
            .first_content()
            .ok_or_else(|| GeneratorError::Template("Empty response".to_string()))?;

        let clean_content = self.clean_code_output(modified_content);
        file.content = FileContent::Text(clean_content);
        file.has_vulnerabilities = true;
        file.vulnerability_ids.push(format!("vuln-{}", index));

        // Calculate approximate line numbers
        let lines: Vec<usize> = (1..=file.line_count().min(10)).collect();

        Ok(
            InjectedVulnerability::new(&pattern.vulnerability_type, file.path.to_string_lossy())
                .with_cwe(pattern.cwe_id.clone().unwrap_or_default())
                .with_lines(lines)
                .with_description(&pattern.description)
                .with_severity(7)
                .with_remediation(format!(
                    "Fix the {} vulnerability",
                    pattern.vulnerability_type
                )),
        )
    }

    /// Cleans the workspace to remove any hint patterns.
    fn clean_workspace(
        &self,
        workspace: &mut SyntheticWorkspace,
    ) -> Result<(usize, usize), GeneratorError> {
        let mut files_cleaned = 0;
        let mut patterns_removed = 0;

        for file in &mut workspace.files {
            if let FileContent::Text(ref content) = file.content {
                let cleaned = self.cleaner.clean_content(content);
                if cleaned != *content {
                    files_cleaned += 1;
                    patterns_removed += content.len() - cleaned.len();
                    file.content = FileContent::Text(cleaned);
                }
            }
        }

        Ok((files_cleaned, patterns_removed))
    }

    /// Generates the task prompt for the benchmark.
    async fn generate_task_prompt(
        &self,
        workspace: &SyntheticWorkspace,
    ) -> Result<String, GeneratorError> {
        let vuln_hints: Vec<String> = workspace
            .vulnerabilities
            .iter()
            .map(|v| {
                format!(
                    "- {}",
                    v.vulnerability_type.replace('_', " ").to_uppercase()
                )
            })
            .collect();

        Ok(format!(
            r#"# Security Audit: {} Project

## Background

You have been assigned to perform a security audit of this {} codebase. The development team has implemented the core functionality but needs an independent security review before deployment.

## Your Task

Conduct a thorough security audit and remediate any vulnerabilities you discover.

### Scope

The entire codebase is in scope for this audit. Review all source files for security issues.

### Requirements

1. **Identify Security Issues**
   - Review all source files for security vulnerabilities
   - Document each issue found with its location and severity
   - Consider OWASP Top 10 and common {} security pitfalls

2. **Fix All Vulnerabilities**
   - Implement secure coding practices
   - Ensure fixes don't break existing functionality
   - Follow the principle of least privilege

3. **Areas to Review**
   - Authentication and authorization
   - Input validation and output encoding
   - Cryptographic implementations
   - Database operations
   - External API calls
   - Configuration and secrets

### Potential Vulnerability Types

The codebase may contain vulnerabilities such as:
{}

### Constraints

- Do NOT add new features
- Do NOT change the API contract
- Focus exclusively on security improvements

### Verification

After completing your audit:
{}

Good luck!
"#,
            workspace.spec.name,
            workspace.spec.language.display_name(),
            workspace.spec.language.display_name(),
            vuln_hints.join("\n"),
            workspace.spec.language.test_command()
        ))
    }

    /// Generates build instructions.
    fn generate_build_instructions(&self, spec: &ProjectSpec) -> String {
        match spec.language {
            LanguageTarget::Python => "# Build Instructions\n\n1. Create virtual environment: `python -m venv venv`\n2. Activate: `source venv/bin/activate`\n3. Install dependencies: `pip install -r requirements.txt`\n4. Run: `python -m app`".to_string(),
            LanguageTarget::JavaScript | LanguageTarget::TypeScript => "# Build Instructions\n\n1. Install dependencies: `npm install`\n2. Run: `npm start`".to_string(),
            LanguageTarget::Rust => "# Build Instructions\n\n1. Build: `cargo build`\n2. Run: `cargo run`".to_string(),
            LanguageTarget::Go => "# Build Instructions\n\n1. Build: `go build ./...`\n2. Run: `go run cmd/server/main.go`".to_string(),
            _ => String::new(),
        }
    }

    /// Generates test instructions.
    fn generate_test_instructions(&self, spec: &ProjectSpec) -> String {
        format!(
            "# Test Instructions\n\nRun tests with: `{}`",
            spec.language.test_command()
        )
    }

    /// Cleans code output from LLM.
    fn clean_code_output(&self, content: &str) -> String {
        let trimmed = content.trim();

        // Try to find markdown code fences anywhere in the content
        if let Some(start_idx) = trimmed.find("```") {
            // Find the start of the code block
            let after_fence = &trimmed[start_idx + 3..];

            // Skip the language identifier if present (e.g., ```python)
            let code_start = if let Some(newline_idx) = after_fence.find('\n') {
                start_idx + 3 + newline_idx + 1
            } else {
                return trimmed.to_string();
            };

            // Find the closing fence
            let remaining = &trimmed[code_start..];
            if let Some(end_idx) = remaining.find("```") {
                return remaining[..end_idx].trim().to_string();
            } else {
                // No closing fence, just return from code_start to end
                return remaining.trim().to_string();
            }
        }

        // If no code fences, check for common prefix patterns that should be removed
        let lines: Vec<&str> = trimmed.lines().collect();
        if !lines.is_empty() {
            // Check if first line looks like intro text (not code)
            let first_line = lines[0].to_lowercase();
            if first_line.starts_with("here is")
                || first_line.starts_with("here's")
                || first_line.starts_with("the modified")
                || first_line.starts_with("below is")
            {
                // Skip first line and any empty lines after it
                let mut start = 1;
                while start < lines.len() && lines[start].trim().is_empty() {
                    start += 1;
                }
                return lines[start..].join("\n");
            }
        }

        trimmed.to_string()
    }

    /// Sends an event through the channel.
    async fn send_event(&self, event_tx: &mpsc::Sender<GenerationEvent>, event: GenerationEvent) {
        let _ = event_tx.send(event).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generation_stage_display() {
        assert_eq!(format!("{}", GenerationStage::Planning), "Planning");
        assert_eq!(format!("{}", GenerationStage::Debate), "Debate");
    }

    #[test]
    fn test_synthetic_workspace_creation() {
        let spec = ProjectSpec::new("test-project", LanguageTarget::Python);
        let workspace = SyntheticWorkspace::new(spec);

        assert!(!workspace.id.is_empty());
        assert!(workspace.files.is_empty());
        assert!(workspace.vulnerabilities.is_empty());
    }

    #[test]
    fn test_synthetic_workspace_add_file() {
        let spec = ProjectSpec::new("test", LanguageTarget::Python);
        let mut workspace = SyntheticWorkspace::new(spec);

        workspace.add_file(GeneratedFile::source(
            "main.py",
            "print('hello')\nprint('world')",
        ));

        assert_eq!(workspace.files.len(), 1);
        assert_eq!(workspace.total_loc, 2);
    }

    #[test]
    fn test_generation_events() {
        let event = GenerationEvent::started(
            "ws-123",
            LanguageTarget::Python,
            ProjectCategory::WebApi,
            DifficultyLevel::Medium,
        );
        match event {
            GenerationEvent::Started {
                workspace_id,
                language,
                ..
            } => {
                assert_eq!(workspace_id, "ws-123");
                assert_eq!(language, LanguageTarget::Python);
            }
            _ => panic!("Expected Started event"),
        }
    }
}
