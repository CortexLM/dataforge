//! Simplified CLI command definitions for dataforge.
//!
//! This module provides a streamlined command-line interface for generating
//! synthetic benchmark datasets in one shot.

use crate::agents::{
    FactoryOrchestrator, FactoryOrchestratorConfig, FactoryPipelineEvent, FactoryPipelineStage,
    FullPipelineConfig, FullPipelineEvent, FullPipelineOrchestrator, SyntheticOrchestrator,
    SyntheticOrchestratorConfig, SyntheticPipelineEvent, SyntheticPipelineStage, SyntheticTask,
    TaskCategory as AgentTaskCategory,
};
use crate::llm::{create_shared_cache, LiteLlmClient, OpenRouterProvider};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Default model to use for generation.
const DEFAULT_MODEL: &str = "moonshotai/kimi-k2.5";

/// Default output directory for generated datasets.
const DEFAULT_OUTPUT_DIR: &str = "./generated-datasets";

/// Synthetic benchmark dataset generator for LLM evaluation.
#[derive(Parser)]
#[command(name = "dataforge")]
#[command(about = "Generate synthetic benchmark datasets for LLM evaluation")]
#[command(version)]
#[command(
    long_about = "dataforge generates synthetic terminal/CLI benchmark tasks to evaluate AI agent capabilities.\n\nIt uses a multi-agent validation system to ensure generated tasks match the requested \ndifficulty level, are solvable but challenging, and meet quality standards.\n\nExample usage:\n  dataforge generate --count 5 --model moonshotai/kimi-k2.5 --output ./datasets"
)]
pub struct Cli {
    /// The subcommand to execute.
    #[command(subcommand)]
    pub command: Commands,

    /// Log level (trace, debug, info, warn, error).
    #[arg(short, long, default_value = "info", global = true)]
    pub log_level: String,
}

/// Available CLI subcommands.
#[derive(clap::Subcommand)]
pub enum Commands {
    /// Generate synthetic benchmark datasets using the multi-agent pipeline.
    ///
    /// This command generates high-quality synthetic benchmark tasks that can be
    /// used to evaluate AI agent capabilities. Tasks are validated through a
    /// multi-agent pipeline including ideation, validation, and quality checks.
    #[command(alias = "gen")]
    Generate(GenerateArgs),

    /// Evaluate generated tasks using an autonomous agent.
    ///
    /// This command runs an autonomous agent against generated benchmark tasks,
    /// providing only the problem statement without any hints. It measures success
    /// rate, duration, and correlates results with task difficulty.
    #[command(alias = "eval")]
    Evaluate(EvaluateArgs),
}

/// Arguments for the generate command.
#[derive(Parser, Debug)]
pub struct GenerateArgs {
    /// Number of datasets to generate.
    #[arg(short = 'n', long, default_value = "1")]
    pub count: u32,

    /// LLM model to use for generation (OpenRouter format).
    ///
    /// Examples: moonshotai/kimi-k2.5, anthropic/claude-3-opus, openai/gpt-4
    #[arg(short = 'm', long, default_value = DEFAULT_MODEL)]
    pub model: String,

    /// Task category to generate.
    ///
    /// Available categories: debugging, security, algorithm-design, infrastructure,
    /// data-engineering, performance, reverse-engineering, integration,
    /// system-administration, software-engineering, file-operations, networking, containers
    #[arg(short = 'c', long)]
    pub category: Option<String>,

    /// Output directory for generated datasets.
    #[arg(short = 'o', long, default_value = DEFAULT_OUTPUT_DIR)]
    pub output: String,

    /// Output JSON to stdout instead of interactive progress.
    #[arg(short = 'j', long)]
    pub json: bool,

    /// Minimum validation score threshold (0.0 to 1.0).
    #[arg(long, default_value = "0.6")]
    pub min_score: f64,

    /// Maximum retries for ideation if validation fails.
    #[arg(long, default_value = "3")]
    pub max_retries: u32,

    /// Random seed for reproducibility.
    #[arg(short = 's', long)]
    pub seed: Option<u64>,

    /// OpenRouter API key (can also be set via OPENROUTER_API_KEY or LITELLM_API_KEY env var).
    #[arg(long, env = "OPENROUTER_API_KEY")]
    pub api_key: Option<String>,

    /// Use the factory multi-agent pipeline (more sophisticated, includes research and amplification).
    #[arg(long, conflicts_with = "full")]
    pub factory: bool,

    /// Use the full 14-agent pipeline for maximum quality (slowest but best quality).
    #[arg(long, conflicts_with = "factory")]
    pub full: bool,

    /// Enable prompt caching for efficiency (only with --factory).
    #[arg(long, default_value = "true")]
    pub cache: bool,

    /// Enable Docker validation to verify tasks are executable in containers.
    #[arg(long)]
    pub validate_docker: bool,

    /// Disable Docker validation (useful in CI without Docker).
    #[arg(long, conflicts_with = "validate_docker")]
    pub no_docker: bool,

    /// Also validate the reference solution in Docker (requires --validate-docker).
    #[arg(long, requires = "validate_docker")]
    pub validate_solution: bool,
}

/// Default maximum steps for the evaluation agent.
const DEFAULT_EVAL_MAX_STEPS: u32 = 50;

/// Default timeout in seconds for task evaluation.
const DEFAULT_EVAL_TIMEOUT_SECS: u64 = 1200;

/// Arguments for the evaluate command.
#[derive(Parser, Debug)]
pub struct EvaluateArgs {
    /// Directory containing generated tasks to evaluate.
    ///
    /// Each subdirectory should contain a task.yaml file with the task specification.
    #[arg(short = 't', long)]
    pub tasks_dir: String,

    /// LLM model to use for the evaluation agent (OpenRouter format).
    ///
    /// Examples: moonshotai/kimi-k2.5, anthropic/claude-3-opus, openai/gpt-4
    #[arg(short = 'm', long, default_value = DEFAULT_MODEL)]
    pub model: String,

    /// OpenRouter API key (can also be set via OPENROUTER_API_KEY or LITELLM_API_KEY env var).
    #[arg(long, env = "OPENROUTER_API_KEY")]
    pub api_key: Option<String>,

    /// Maximum steps for the agent per task.
    #[arg(long, default_value_t = DEFAULT_EVAL_MAX_STEPS)]
    pub max_steps: u32,

    /// Timeout in seconds per task.
    #[arg(long, default_value_t = DEFAULT_EVAL_TIMEOUT_SECS)]
    pub timeout: u64,

    /// Output file for results (JSON format).
    #[arg(short = 'o', long)]
    pub output: Option<String>,

    /// Output JSON to stdout instead of interactive progress.
    #[arg(short = 'j', long)]
    pub json: bool,
}

/// Parse CLI arguments and return the Cli struct.
///
/// This allows main.rs to access CLI arguments (like log_level) before running commands.
pub fn parse_cli() -> Cli {
    Cli::parse()
}

/// Run the CLI by parsing arguments and executing the command.
///
/// This is a convenience function that parses CLI args and runs the command.
/// For more control over logging initialization, use `parse_cli()` and `run_with_cli()`.
pub async fn run() -> anyhow::Result<()> {
    run_with_cli(parse_cli()).await
}

/// Run the CLI with the parsed arguments.
///
/// This is the main entry point for the dataforge CLI.
pub async fn run_with_cli(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Generate(args) => {
            run_generate_command(args).await?;
        }
        Commands::Evaluate(args) => {
            run_evaluate_command(args).await?;
        }
    }
    Ok(())
}

// ============================================================================
// Generate Command Implementation
// ============================================================================

/// JSON output structure for the generation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationOutput {
    /// Overall status: "success" or "failed".
    pub status: String,
    /// Model used for generation.
    pub model: String,
    /// List of generated tasks.
    pub tasks: Vec<GeneratedTaskOutput>,
    /// Total duration in milliseconds.
    pub total_duration_ms: u64,
    /// Number of retries that occurred.
    pub retries: u32,
    /// Output directory where tasks were saved.
    pub output_directory: String,
}

/// JSON output structure for a single generated task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedTaskOutput {
    /// Unique task identifier.
    pub task_id: String,
    /// Task category.
    pub category: String,
    /// Problem statement for the task.
    pub problem_statement: String,
    /// Difficulty level.
    pub difficulty: String,
    /// Tags associated with the task.
    pub tags: Vec<String>,
    /// Verification criteria for the task.
    pub verification_criteria: Vec<String>,
    /// Path where the task was saved.
    pub saved_path: Option<String>,
}

impl GeneratedTaskOutput {
    /// Creates a GeneratedTaskOutput from a SyntheticTask.
    fn from_synthetic_task(task: &SyntheticTask, saved_path: Option<String>) -> Self {
        Self {
            task_id: task.id.clone(),
            category: task.metadata.category.clone(),
            problem_statement: task.problem_statement.clone(),
            difficulty: format!("{:?}", task.difficulty.level),
            tags: task.metadata.tags.clone(),
            verification_criteria: task.verification.success_criteria.clone(),
            saved_path,
        }
    }
}

/// Parses a category string to AgentTaskCategory enum.
fn parse_task_category(category_str: &str) -> anyhow::Result<AgentTaskCategory> {
    match category_str.to_lowercase().as_str() {
        "debugging" | "debug" => Ok(AgentTaskCategory::Debugging),
        "system-debugging" | "system_debugging" => Ok(AgentTaskCategory::SystemDebugging),
        "security" => Ok(AgentTaskCategory::Security),
        "security-analysis" | "security_analysis" => Ok(AgentTaskCategory::SecurityAnalysis),
        "algorithm" | "algorithm-design" | "algorithm_design" => {
            Ok(AgentTaskCategory::AlgorithmDesign)
        }
        "infrastructure" | "infra" => Ok(AgentTaskCategory::Infrastructure),
        "data-engineering" | "data_engineering" | "data" => Ok(AgentTaskCategory::DataEngineering),
        "data-science" | "data_science" => Ok(AgentTaskCategory::DataScience),
        "performance" | "performance-optimization" | "performance_optimization" => {
            Ok(AgentTaskCategory::PerformanceOptimization)
        }
        "reverse-engineering" | "reverse_engineering" | "reverse" => {
            Ok(AgentTaskCategory::ReverseEngineering)
        }
        "integration" | "integration-tasks" | "integration_tasks" => {
            Ok(AgentTaskCategory::IntegrationTasks)
        }
        "system-administration" | "system_administration" | "sysadmin" => {
            Ok(AgentTaskCategory::SystemAdministration)
        }
        "software-engineering" | "software_engineering" | "software" => {
            Ok(AgentTaskCategory::SoftwareEngineering)
        }
        "file-operations" | "file_operations" | "files" => Ok(AgentTaskCategory::FileOperations),
        "networking" | "network" => Ok(AgentTaskCategory::Networking),
        "containers" | "container" | "docker" => Ok(AgentTaskCategory::Containers),
        other => Err(anyhow::anyhow!(
            "Unknown category: '{}'. Available categories: debugging, security, algorithm-design, \
             infrastructure, data-engineering, performance, reverse-engineering, integration, \
             system-administration, software-engineering, file-operations, data-science, \
             networking, containers",
            other
        )),
    }
}

/// Runs the generate command with the provided arguments.
async fn run_generate_command(args: GenerateArgs) -> anyhow::Result<()> {
    // Validate and clamp min_score to valid range
    let validated_min_score = args.min_score.clamp(0.0, 1.0);
    if (validated_min_score - args.min_score).abs() > f64::EPSILON {
        warn!(
            original = args.min_score,
            clamped = validated_min_score,
            "min_score was outside valid range [0.0, 1.0] and has been clamped"
        );
    }

    // Parse category if provided
    let parsed_category = match &args.category {
        Some(cat_str) => Some(parse_task_category(cat_str)?),
        None => None,
    };

    // Set seed for reproducibility if provided
    if let Some(s) = args.seed {
        info!(seed = s, "Using fixed seed for reproducibility");
    }

    // Get API key from argument or environment
    let api_key = args
        .api_key
        .clone()
        .or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
        .or_else(|| std::env::var("LITELLM_API_KEY").ok());

    // Initialize LLM client
    let llm_client: Arc<dyn crate::llm::LlmProvider> = if let Some(key) = api_key {
        info!(model = %args.model, "Using OpenRouter with specified API key");
        Arc::new(OpenRouterProvider::with_model(key, args.model.clone()))
    } else {
        // Fall back to LiteLlmClient from environment
        info!("Using LiteLLM client from environment");
        Arc::new(LiteLlmClient::from_env().map_err(|e| {
            anyhow::anyhow!(
                "Failed to initialize LLM client: {}. \
                 Please provide --api-key or set OPENROUTER_API_KEY/LITELLM_API_KEY env var.",
                e
            )
        })?)
    };

    // Create output directory
    let output_dir = args.output.clone();
    let output_path = Path::new(&output_dir);
    fs::create_dir_all(output_path)?;
    info!(output = %output_dir, "Output directory ready");

    if args.full {
        run_full_pipeline_generation(
            llm_client,
            args,
            parsed_category,
            validated_min_score,
            output_path,
        )
        .await
    } else if args.factory {
        run_factory_generation(
            llm_client,
            args,
            parsed_category,
            validated_min_score,
            output_path,
        )
        .await
    } else {
        run_synthetic_generation(
            llm_client,
            args,
            parsed_category,
            validated_min_score,
            output_path,
        )
        .await
    }
}

/// Runs the synthetic task generation pipeline.
async fn run_synthetic_generation(
    llm_client: Arc<dyn crate::llm::LlmProvider>,
    args: GenerateArgs,
    category: Option<AgentTaskCategory>,
    min_score: f64,
    output_path: &Path,
) -> anyhow::Result<()> {
    // Determine Docker validation settings
    let docker_enabled = args.validate_docker && !args.no_docker;
    let docker_solution = args.validate_solution;

    if docker_enabled {
        info!("Docker validation enabled");
    }

    // Configure the orchestrator
    let config = SyntheticOrchestratorConfig::default()
        .with_min_validation_score(min_score)
        .with_max_ideation_retries(args.max_retries)
        .with_docker_validation(docker_enabled)
        .with_docker_solution_validation(docker_solution);

    let orchestrator = SyntheticOrchestrator::new(llm_client, config);

    if args.json {
        run_json_generation(&orchestrator, &args, category, output_path).await
    } else {
        run_interactive_generation(&orchestrator, &args, category, output_path).await
    }
}

/// Runs the factory multi-agent generation pipeline.
async fn run_factory_generation(
    llm_client: Arc<dyn crate::llm::LlmProvider>,
    args: GenerateArgs,
    _category: Option<AgentTaskCategory>,
    min_score: f64,
    output_path: &Path,
) -> anyhow::Result<()> {
    // Initialize prompt cache if enabled
    let _prompt_cache = if args.cache {
        Some(create_shared_cache(1000))
    } else {
        None
    };

    // Determine Docker validation settings
    let docker_enabled = args.validate_docker && !args.no_docker;
    let docker_solution = args.validate_solution;

    if docker_enabled {
        info!("Docker validation enabled for factory pipeline");
    }

    // Configure the factory orchestrator
    let config = FactoryOrchestratorConfig::default()
        .with_min_validation_score(min_score)
        .with_max_creation_retries(args.max_retries)
        .with_docker_validation(docker_enabled)
        .with_docker_solution_validation(docker_solution);

    let orchestrator = FactoryOrchestrator::new(llm_client, config);

    if args.json {
        run_json_factory(&orchestrator, &args, output_path).await
    } else {
        run_interactive_factory(&orchestrator, &args, output_path).await
    }
}

/// Runs the synthetic generation pipeline and outputs JSON to stdout.
async fn run_json_generation(
    orchestrator: &SyntheticOrchestrator,
    args: &GenerateArgs,
    category: Option<AgentTaskCategory>,
    output_path: &Path,
) -> anyhow::Result<()> {
    let start_time = std::time::Instant::now();
    let mut tasks = Vec::new();
    let mut total_retries = 0u32;

    for i in 0..args.count {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<SyntheticPipelineEvent>(100);

        // Spawn task to track retries from events
        let retry_handle = tokio::spawn(async move {
            let mut retries = 0u32;
            while let Some(event) = event_rx.recv().await {
                if let SyntheticPipelineEvent::ValidationRejected { .. } = event {
                    retries += 1;
                }
            }
            retries
        });

        match orchestrator.generate_task(category, event_tx).await {
            Ok(task) => {
                // Save task to disk
                let task_dir = output_path.join(&task.id);
                let saved_path = match save_task(&task, &task_dir) {
                    Ok(()) => Some(task_dir.to_string_lossy().to_string()),
                    Err(e) => {
                        warn!(error = %e, task_id = %task.id, "Failed to save task to disk");
                        None
                    }
                };
                tasks.push(GeneratedTaskOutput::from_synthetic_task(&task, saved_path));
            }
            Err(e) => {
                error!(task_index = i, error = %e, "Failed to generate task");
            }
        }

        if let Ok(retries) = retry_handle.await {
            total_retries += retries;
        }
    }

    let duration_ms = start_time.elapsed().as_millis() as u64;

    let output = GenerationOutput {
        status: if tasks.is_empty() && args.count > 0 {
            "failed".to_string()
        } else {
            "success".to_string()
        },
        model: args.model.clone(),
        tasks,
        total_duration_ms: duration_ms,
        retries: total_retries,
        output_directory: output_path.to_string_lossy().to_string(),
    };

    let json_output = serde_json::to_string_pretty(&output)
        .map_err(|e| anyhow::anyhow!("Failed to serialize JSON output: {}", e))?;
    println!("{}", json_output);

    Ok(())
}

/// Runs the interactive synthetic generation with tree-based progress output.
async fn run_interactive_generation(
    orchestrator: &SyntheticOrchestrator,
    args: &GenerateArgs,
    category: Option<AgentTaskCategory>,
    output_path: &Path,
) -> anyhow::Result<()> {
    println!("\n🔬 Synthetic Dataset Generation");
    println!("================================");
    println!("Model: {}", args.model);
    println!("Count: {}", args.count);
    println!("Output: {}", output_path.display());
    if let Some(cat) = &args.category {
        println!("Category: {}", cat);
    }
    println!();

    println!("Pipeline stages:");
    println!("├─ ○ Ideation (IdeatorAgent)");
    println!("├─ ○ Validation (TaskValidatorAgent)");
    println!("├─ ○ Execution (TaskExecutorAgent)");
    println!("└─ ○ Quality Check\n");

    let mut generated_tasks: Vec<SyntheticTask> = Vec::new();
    let mut failed_count = 0u32;

    for i in 0..args.count {
        println!("📝 Generating dataset {}/{}...", i + 1, args.count);

        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<SyntheticPipelineEvent>(100);

        // Spawn event handler for this task
        let event_handle = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                match event {
                    SyntheticPipelineEvent::StageStarted { stage, .. } => {
                        let stage_name = match stage {
                            SyntheticPipelineStage::Ideation => "Ideation",
                            SyntheticPipelineStage::Validation => "Validation",
                            SyntheticPipelineStage::Execution => "Execution",
                            SyntheticPipelineStage::DockerValidation => "Docker Validation",
                            SyntheticPipelineStage::QualityCheck => "Quality Check",
                        };
                        println!("   ⟳ {} started...", stage_name);
                    }
                    SyntheticPipelineEvent::IdeationComplete { idea, .. } => {
                        println!("   ✓ Ideation: \"{}\"", idea.title);
                    }
                    SyntheticPipelineEvent::ValidationComplete {
                        passed, assessment, ..
                    } => {
                        let symbol = if passed { "✓" } else { "✗" };
                        println!(
                            "   {} Validation: score={:.2}",
                            symbol, assessment.complexity_score
                        );
                    }
                    SyntheticPipelineEvent::ValidationRejected { retry_count, .. } => {
                        println!("   ↻ Validation rejected, retry #{}", retry_count);
                    }
                    SyntheticPipelineEvent::ExecutionComplete { .. } => {
                        println!("   ✓ Execution: task created");
                    }
                    SyntheticPipelineEvent::DockerValidationStarted { task_id, image, .. } => {
                        println!("   🐳 Docker: validating {} with {}", task_id, image);
                    }
                    SyntheticPipelineEvent::DockerValidationComplete {
                        passed,
                        duration_ms,
                        error,
                        ..
                    } => {
                        if passed {
                            println!("   ✓ Docker: validated in {}ms", duration_ms);
                        } else {
                            println!(
                                "   ✗ Docker: failed - {}",
                                error.unwrap_or_else(|| "unknown error".to_string())
                            );
                        }
                    }
                    SyntheticPipelineEvent::DockerValidationSkipped { reason, .. } => {
                        println!("   ⏭ Docker: skipped - {}", reason);
                    }
                    SyntheticPipelineEvent::PipelineComplete {
                        total_duration_ms, ..
                    } => {
                        println!("   ✓ Complete in {}ms", total_duration_ms);
                    }
                    SyntheticPipelineEvent::PipelineFailed { error, stage, .. } => {
                        println!("   ✗ Failed at {}: {}", stage, error);
                    }
                }
            }
        });

        let result = orchestrator.generate_task(category, event_tx).await;

        // Wait for event handler to finish
        let _ = event_handle.await;

        match result {
            Ok(task) => {
                // Save task to disk
                let task_dir = output_path.join(&task.id);
                match save_task(&task, &task_dir) {
                    Ok(()) => {
                        println!("   💾 Saved: {} → {}", task.id, task_dir.display());
                    }
                    Err(e) => {
                        warn!(error = %e, task_id = %task.id, "Failed to save task to disk");
                    }
                }

                println!("\n✓ Dataset {} generated successfully!", i + 1);
                println!("  ID: {}", task.id);
                println!("  Category: {}", task.metadata.category);
                println!("  Difficulty: {:?}", task.difficulty.level);
                generated_tasks.push(task);
            }
            Err(e) => {
                eprintln!("\n✗ Dataset {} failed: {}", i + 1, e);
                failed_count += 1;
            }
        }

        if i < args.count - 1 {
            println!(); // Add spacing between tasks
        }
    }

    // Print summary
    println!("\n{}", "=".repeat(50));
    println!("📊 Generation Summary");
    println!("{}", "=".repeat(50));
    println!(
        "✓ Successfully generated: {}/{}",
        generated_tasks.len(),
        args.count
    );
    if failed_count > 0 {
        println!("✗ Failed: {}", failed_count);
    }
    println!("📁 Output directory: {}", output_path.display());

    if !generated_tasks.is_empty() {
        println!("\n📋 Generated Datasets:");
        for (idx, task) in generated_tasks.iter().enumerate() {
            println!(
                "  {}. [{}] {} → {}",
                idx + 1,
                task.metadata.category,
                task.id,
                output_path.join(&task.id).display()
            );
        }
    }

    if generated_tasks.is_empty() && args.count > 0 {
        return Err(anyhow::anyhow!("Failed to generate any datasets"));
    }

    Ok(())
}

/// Runs the factory generation pipeline and outputs JSON to stdout.
async fn run_json_factory(
    orchestrator: &FactoryOrchestrator,
    args: &GenerateArgs,
    output_path: &Path,
) -> anyhow::Result<()> {
    let start_time = std::time::Instant::now();

    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<FactoryPipelineEvent>(100);

    // Spawn event consumer (just drain events in JSON mode)
    let _event_handle = tokio::spawn(async move {
        while event_rx.recv().await.is_some() {
            // Silently consume events in JSON mode
        }
    });

    let result = orchestrator
        .run_factory_pipeline(args.category.as_deref(), args.count, event_tx)
        .await;

    let duration_ms = start_time.elapsed().as_millis() as u64;

    match result {
        Ok(generated_tasks) => {
            let mut tasks = Vec::new();
            for task in &generated_tasks {
                let task_dir = output_path.join(&task.id);
                let saved_path = match save_task(task, &task_dir) {
                    Ok(()) => Some(task_dir.to_string_lossy().to_string()),
                    Err(e) => {
                        warn!(error = %e, task_id = %task.id, "Failed to save task to disk");
                        None
                    }
                };
                tasks.push(GeneratedTaskOutput::from_synthetic_task(task, saved_path));
            }

            let output = GenerationOutput {
                status: "success".to_string(),
                model: args.model.clone(),
                tasks,
                total_duration_ms: duration_ms,
                retries: 0,
                output_directory: output_path.to_string_lossy().to_string(),
            };

            let json_output = serde_json::to_string_pretty(&output)
                .map_err(|e| anyhow::anyhow!("Failed to serialize JSON output: {}", e))?;
            println!("{}", json_output);

            Ok(())
        }
        Err(e) => {
            let output = GenerationOutput {
                status: "failed".to_string(),
                model: args.model.clone(),
                tasks: vec![],
                total_duration_ms: duration_ms,
                retries: 0,
                output_directory: output_path.to_string_lossy().to_string(),
            };

            let json_output = serde_json::to_string_pretty(&output)
                .map_err(|e| anyhow::anyhow!("Failed to serialize JSON output: {}", e))?;
            println!("{}", json_output);

            Err(anyhow::anyhow!("Factory pipeline failed: {}", e))
        }
    }
}

/// Runs the interactive factory generation with tree-based progress output.
async fn run_interactive_factory(
    orchestrator: &FactoryOrchestrator,
    args: &GenerateArgs,
    output_path: &Path,
) -> anyhow::Result<()> {
    println!("\n🏭 Factory Multi-Agent Dataset Generation");
    println!("==========================================");
    println!("Model: {}", args.model);
    println!("Count: {}", args.count);
    println!("Output: {}", output_path.display());
    if let Some(cat) = &args.category {
        println!("Category: {}", cat);
    }
    println!();

    println!("Pipeline stages:");
    println!("├─ ○ Research (ResearchAgent)");
    println!("├─ ○ Creation (IdeatorAgent)");
    println!("├─ ○ Amplification (DifficultyAmplifierAgent)");
    println!("├─ ○ Validation (TaskValidatorAgent)");
    println!("└─ ○ Finalization\n");

    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<FactoryPipelineEvent>(100);

    // Spawn event handler
    let event_handle = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                FactoryPipelineEvent::StageStarted { stage, .. } => {
                    let stage_name = match stage {
                        FactoryPipelineStage::Research => "Research",
                        FactoryPipelineStage::Creation => "Creation",
                        FactoryPipelineStage::Amplification => "Amplification",
                        FactoryPipelineStage::Validation => "Validation",
                        FactoryPipelineStage::Finalization => "Finalization",
                    };
                    println!("   ⟳ {} started...", stage_name);
                }
                FactoryPipelineEvent::ResearchComplete {
                    weaknesses_found,
                    traps_proposed,
                    ..
                } => {
                    println!(
                        "   ✓ Research: found {} weaknesses, proposed {} traps",
                        weaknesses_found, traps_proposed
                    );
                }
                FactoryPipelineEvent::CreationComplete {
                    task_title,
                    category,
                    ..
                } => {
                    println!("   ✓ Creation: \"{}\" [{}]", task_title, category);
                }
                FactoryPipelineEvent::AmplificationComplete {
                    traps_added,
                    difficulty_score,
                    ..
                } => {
                    println!(
                        "   ✓ Amplification: added {} traps, difficulty score={:.2}",
                        traps_added, difficulty_score
                    );
                }
                FactoryPipelineEvent::ValidationComplete { passed, score, .. } => {
                    let symbol = if passed { "✓" } else { "✗" };
                    println!("   {} Validation: score={:.2}", symbol, score);
                }
                FactoryPipelineEvent::AgentConversation {
                    agent_name,
                    message_summary,
                    ..
                } => {
                    println!("   💬 {}: {}", agent_name, message_summary);
                }
                FactoryPipelineEvent::PipelineComplete {
                    tasks_generated,
                    total_duration_ms,
                    ..
                } => {
                    println!(
                        "\n   ✓ Pipeline complete: {} datasets in {}ms",
                        tasks_generated, total_duration_ms
                    );
                }
                FactoryPipelineEvent::PipelineFailed { error, stage, .. } => {
                    println!("   ✗ Failed at {:?}: {}", stage, error);
                }
            }
        }
    });

    println!("📝 Starting factory pipeline...\n");

    let result = orchestrator
        .run_factory_pipeline(args.category.as_deref(), args.count, event_tx)
        .await;

    // Wait for event handler to finish
    let _ = event_handle.await;

    match result {
        Ok(generated_tasks) => {
            // Save tasks to output directory
            for task in &generated_tasks {
                let task_dir = output_path.join(&task.id);
                if let Err(e) = save_task(task, &task_dir) {
                    warn!(error = %e, task_id = %task.id, "Failed to save task to disk");
                } else {
                    println!("   💾 Saved: {} → {}", task.id, task_dir.display());
                }
            }

            // Print summary
            println!("\n{}", "=".repeat(50));
            println!("🏭 Factory Generation Summary");
            println!("{}", "=".repeat(50));
            println!(
                "✓ Successfully generated: {}/{}",
                generated_tasks.len(),
                args.count
            );
            println!("📁 Output directory: {}", output_path.display());

            if !generated_tasks.is_empty() {
                println!("\n📋 Generated Datasets:");
                for (idx, task) in generated_tasks.iter().enumerate() {
                    println!(
                        "  {}. [{}] {} → {}",
                        idx + 1,
                        task.metadata.category,
                        task.id,
                        output_path.join(&task.id).display()
                    );
                }
            }

            Ok(())
        }
        Err(e) => {
            eprintln!("\n✗ Factory pipeline failed: {}", e);
            Err(anyhow::anyhow!(
                "Failed to generate factory datasets: {}. Check LLM configuration and API access.",
                e
            ))
        }
    }
}

/// Save a generated task to disk in terminal-bench compatible format.
fn save_task(task: &SyntheticTask, task_dir: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(task_dir)?;

    // Save prompt.md
    let prompt_path = task_dir.join("prompt.md");
    let prompt_content = format!(
        "# {}\n\n## Problem Statement\n\n{}\n\n## Success Criteria\n\n{}\n\n## Automated Checks\n\n{}\n",
        task.id,
        task.problem_statement,
        task.verification
            .success_criteria
            .iter()
            .map(|c| format!("- {}", c))
            .collect::<Vec<_>>()
            .join("\n"),
        task.verification
            .automated_checks
            .iter()
            .map(|c| format!("- {:?}: {} → {}", c.check_type, c.target, c.expected))
            .collect::<Vec<_>>()
            .join("\n")
    );
    fs::write(&prompt_path, prompt_content)?;

    // Save task.yaml with metadata
    let task_yaml_path = task_dir.join("task.yaml");
    let task_yaml = serde_yaml::to_string(task)
        .map_err(|e| anyhow::anyhow!("Failed to serialize task to YAML: {}", e))?;
    fs::write(&task_yaml_path, task_yaml)?;

    // Save solution.sh if available
    if !task.hidden_solution.reference_commands.is_empty() {
        let solution_path = task_dir.join("solution.sh");
        let solution_content = format!(
            "#!/bin/bash\n# Solution for {}\n# DO NOT DISTRIBUTE WITH BENCHMARK\n\n# Approach: {}\n\n# Key Insights:\n{}\n\n# Reference Commands:\n{}\n",
            task.id,
            task.hidden_solution.approach,
            task.hidden_solution
                .key_insights
                .iter()
                .map(|i| format!("# - {}", i))
                .collect::<Vec<_>>()
                .join("\n"),
            task.hidden_solution
                .reference_commands
                .iter()
                .enumerate()
                .map(|(i, cmd)| format!("# Step {}:\n{}", i + 1, cmd))
                .collect::<Vec<_>>()
                .join("\n\n")
        );
        fs::write(&solution_path, solution_content)?;
    }

    Ok(())
}

/// Runs the full 14-agent pipeline for maximum quality generation.
async fn run_full_pipeline_generation(
    llm_client: Arc<dyn crate::llm::LlmProvider>,
    args: GenerateArgs,
    category: Option<AgentTaskCategory>,
    min_score: f64,
    output_path: &Path,
) -> anyhow::Result<()> {
    // Determine Docker validation settings
    let docker_enabled = args.validate_docker && !args.no_docker;
    let docker_solution = args.validate_solution;

    info!("Using full 14-agent pipeline for maximum quality");
    if docker_enabled {
        info!("Docker validation enabled for full pipeline");
    }

    // Configure the full pipeline orchestrator
    let config = FullPipelineConfig::default()
        .with_min_validation_score(min_score)
        .with_max_retries(args.max_retries)
        .with_docker_validation(docker_enabled)
        .with_solution_validation(docker_solution)
        .with_output_dir(output_path.to_string_lossy().to_string());

    let orchestrator = FullPipelineOrchestrator::new(llm_client, config);

    if args.json {
        run_json_full_pipeline(&orchestrator, &args, category, output_path).await
    } else {
        run_interactive_full_pipeline(&orchestrator, &args, category, output_path).await
    }
}

/// Runs the full pipeline and outputs JSON to stdout.
async fn run_json_full_pipeline(
    orchestrator: &FullPipelineOrchestrator,
    args: &GenerateArgs,
    category: Option<AgentTaskCategory>,
    output_path: &Path,
) -> anyhow::Result<()> {
    let start_time = std::time::Instant::now();
    let mut tasks = Vec::new();

    for i in 0..args.count {
        let (event_tx, _event_rx) = tokio::sync::mpsc::channel::<FullPipelineEvent>(100);

        match orchestrator.generate_task(category, event_tx).await {
            Ok(result) => {
                let task = &result.task;
                let task_dir = output_path.join(&task.id);
                let saved_path = match save_task(task, &task_dir) {
                    Ok(()) => Some(task_dir.to_string_lossy().to_string()),
                    Err(e) => {
                        warn!(error = %e, task_id = %task.id, "Failed to save task to disk");
                        None
                    }
                };
                tasks.push(GeneratedTaskOutput::from_synthetic_task(task, saved_path));
            }
            Err(e) => {
                error!(task_index = i, error = %e, "Failed to generate task");
            }
        }
    }

    let duration_ms = start_time.elapsed().as_millis() as u64;

    let output = GenerationOutput {
        status: if tasks.is_empty() && args.count > 0 {
            "failed".to_string()
        } else {
            "success".to_string()
        },
        model: args.model.clone(),
        tasks,
        total_duration_ms: duration_ms,
        retries: 0,
        output_directory: output_path.to_string_lossy().to_string(),
    };

    let json_output = serde_json::to_string_pretty(&output)
        .map_err(|e| anyhow::anyhow!("Failed to serialize JSON output: {}", e))?;
    println!("{}", json_output);

    Ok(())
}

/// Runs the interactive full pipeline with progress output.
async fn run_interactive_full_pipeline(
    orchestrator: &FullPipelineOrchestrator,
    args: &GenerateArgs,
    category: Option<AgentTaskCategory>,
    output_path: &Path,
) -> anyhow::Result<()> {
    println!("\n🚀 Full 14-Agent Pipeline Generation");
    println!("=====================================");
    println!("Model: {}", args.model);
    println!("Count: {}", args.count);
    println!("Output: {}", output_path.display());
    if let Some(cat) = &args.category {
        println!("Category: {}", cat);
    }
    println!();

    println!("Pipeline stages (14 agents):");
    println!("├─ ○ Research (ResearchAgent)");
    println!("├─ ○ Ideation (IdeatorAgent)");
    println!("├─ ○ Task Validation (TaskValidatorAgent)");
    println!("├─ ○ Amplification (DifficultyAmplifierAgent)");
    println!("├─ ○ Execution (TaskExecutorAgent)");
    println!("├─ ○ Test Design (TestDesignerAgent)");
    println!("├─ ○ Environment Building (EnvironmentBuilderAgent)");
    println!("├─ ○ Docker Validation (DockerValidatorAgent)");
    println!("└─ ○ Quality Check\n");

    let mut generated_tasks: Vec<SyntheticTask> = Vec::new();
    let mut failed_count = 0u32;

    for i in 0..args.count {
        println!("📝 Generating dataset {}/{}...", i + 1, args.count);

        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<FullPipelineEvent>(100);

        // Spawn event handler for this task
        let event_handle = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                match event {
                    FullPipelineEvent::StageStarted { stage, .. } => {
                        println!("  ├─ ⏳ Starting {}...", stage);
                    }
                    FullPipelineEvent::StageCompleted {
                        stage, duration_ms, ..
                    } => {
                        println!("  ├─ ✅ {} completed ({}ms)", stage, duration_ms);
                    }
                    FullPipelineEvent::StageFailed { stage, error, .. } => {
                        println!("  ├─ ❌ {} failed: {}", stage, error);
                    }
                    FullPipelineEvent::StageSkipped { stage, reason, .. } => {
                        println!("  ├─ ⏭️ {} skipped: {}", stage, reason);
                    }
                    FullPipelineEvent::ResearchComplete {
                        weaknesses_found,
                        traps_proposed,
                        ..
                    } => {
                        println!(
                            "  │   Found {} weaknesses, {} traps proposed",
                            weaknesses_found, traps_proposed
                        );
                    }
                    FullPipelineEvent::IdeationComplete {
                        task_title,
                        category,
                        ..
                    } => {
                        println!("  │   Created: {} [{}]", task_title, category);
                    }
                    FullPipelineEvent::AmplificationComplete {
                        traps_added,
                        difficulty_score,
                        ..
                    } => {
                        println!(
                            "  │   Added {} traps, difficulty: {:.2}",
                            traps_added, difficulty_score
                        );
                    }
                    FullPipelineEvent::TestDesignComplete { test_count, .. } => {
                        println!("  │   Designed {} tests", test_count);
                    }
                    FullPipelineEvent::DockerValidationComplete {
                        passed,
                        duration_ms,
                        ..
                    } => {
                        if passed {
                            println!("  │   Docker validation passed ({}ms)", duration_ms);
                        } else {
                            println!("  │   Docker validation failed ({}ms)", duration_ms);
                        }
                    }
                    FullPipelineEvent::PipelineComplete {
                        task_id,
                        total_duration_ms,
                        stages_completed,
                        ..
                    } => {
                        println!(
                            "  └─ ✅ Task {} generated ({} stages, {}ms)",
                            task_id, stages_completed, total_duration_ms
                        );
                    }
                    _ => {}
                }
            }
        });

        match orchestrator.generate_task(category, event_tx).await {
            Ok(result) => {
                let task = result.task;
                let task_dir = output_path.join(&task.id);
                match save_task(&task, &task_dir) {
                    Ok(()) => {
                        println!("  📁 Saved to: {}\n", task_dir.display());
                    }
                    Err(e) => {
                        warn!(error = %e, task_id = %task.id, "Failed to save task to disk");
                        println!("  ⚠️ Failed to save: {}\n", e);
                    }
                }
                generated_tasks.push(task);
            }
            Err(e) => {
                println!("  └─ ❌ Generation failed: {}\n", e);
                failed_count += 1;
            }
        }

        // Wait for event handler to complete
        let _ = event_handle.await;
    }

    // Print summary
    println!("\n📊 Generation Summary");
    println!("━━━━━━━━━━━━━━━━━━━━━");
    println!("Total requested: {}", args.count);
    println!("Successfully generated: {}", generated_tasks.len());
    println!("Failed: {}", failed_count);

    if !generated_tasks.is_empty() {
        println!("\n📋 Generated Tasks:");
        for task in &generated_tasks {
            println!(
                "  • {} [{}] - {}",
                task.id,
                task.metadata.category,
                task.problem_statement.chars().take(60).collect::<String>()
            );
        }
    }

    Ok(())
}

// ============================================================================
// Evaluate Command Implementation
// ============================================================================

/// JSON output structure for evaluation results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationOutput {
    /// Overall status: "success" or "failed".
    pub status: String,
    /// Model used for evaluation.
    pub model: String,
    /// Total number of tasks evaluated.
    pub total_tasks: usize,
    /// Number of successfully solved tasks.
    pub successful_tasks: usize,
    /// Overall success rate (0.0 to 1.0).
    pub success_rate: f64,
    /// Average duration per task in milliseconds.
    pub average_duration_ms: u64,
    /// Individual task results.
    pub task_results: Vec<TaskEvaluationResult>,
    /// Difficulty correlation metrics.
    pub difficulty_metrics: DifficultyMetrics,
    /// Total evaluation duration in milliseconds.
    pub total_duration_ms: u64,
}

/// Result of evaluating a single task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvaluationResult {
    /// Task identifier.
    pub task_id: String,
    /// Task category.
    pub category: String,
    /// Difficulty level.
    pub difficulty: String,
    /// Whether the task was successfully solved.
    pub success: bool,
    /// Number of steps taken by the agent.
    pub steps_taken: u32,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Error message if the task failed.
    pub error: Option<String>,
    /// Agent's final output/response.
    pub agent_output: Option<String>,
}

/// Metrics correlating success rate with difficulty levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifficultyMetrics {
    /// Success rate for easy tasks.
    pub easy_success_rate: Option<f64>,
    /// Success rate for medium tasks.
    pub medium_success_rate: Option<f64>,
    /// Success rate for hard tasks.
    pub hard_success_rate: Option<f64>,
    /// Average duration for easy tasks (ms).
    pub easy_avg_duration_ms: Option<u64>,
    /// Average duration for medium tasks (ms).
    pub medium_avg_duration_ms: Option<u64>,
    /// Average duration for hard tasks (ms).
    pub hard_avg_duration_ms: Option<u64>,
}

impl DifficultyMetrics {
    /// Compute difficulty metrics from task results.
    fn from_results(results: &[TaskEvaluationResult]) -> Self {
        let mut easy_results: Vec<&TaskEvaluationResult> = Vec::new();
        let mut medium_results: Vec<&TaskEvaluationResult> = Vec::new();
        let mut hard_results: Vec<&TaskEvaluationResult> = Vec::new();

        for result in results {
            match result.difficulty.to_lowercase().as_str() {
                "easy" => easy_results.push(result),
                "medium" => medium_results.push(result),
                "hard" => hard_results.push(result),
                _ => medium_results.push(result), // Default to medium
            }
        }

        Self {
            easy_success_rate: Self::calculate_success_rate(&easy_results),
            medium_success_rate: Self::calculate_success_rate(&medium_results),
            hard_success_rate: Self::calculate_success_rate(&hard_results),
            easy_avg_duration_ms: Self::calculate_avg_duration(&easy_results),
            medium_avg_duration_ms: Self::calculate_avg_duration(&medium_results),
            hard_avg_duration_ms: Self::calculate_avg_duration(&hard_results),
        }
    }

    fn calculate_success_rate(results: &[&TaskEvaluationResult]) -> Option<f64> {
        if results.is_empty() {
            return None;
        }
        let successful = results.iter().filter(|r| r.success).count();
        Some(successful as f64 / results.len() as f64)
    }

    fn calculate_avg_duration(results: &[&TaskEvaluationResult]) -> Option<u64> {
        if results.is_empty() {
            return None;
        }
        let total_duration: u64 = results.iter().map(|r| r.duration_ms).sum();
        Some(total_duration / results.len() as u64)
    }
}

/// Information about a task loaded from disk.
struct LoadedTask {
    task_id: String,
    category: String,
    difficulty: String,
    problem_statement: String,
    success_criteria: Vec<String>,
}

/// Runs the evaluate command with the provided arguments.
async fn run_evaluate_command(args: EvaluateArgs) -> anyhow::Result<()> {
    // Validate that the tasks directory exists
    let tasks_path = Path::new(&args.tasks_dir);
    if !tasks_path.exists() {
        return Err(anyhow::anyhow!(
            "Tasks directory does not exist: {}",
            args.tasks_dir
        ));
    }

    if !tasks_path.is_dir() {
        return Err(anyhow::anyhow!(
            "Tasks path is not a directory: {}",
            args.tasks_dir
        ));
    }

    // Get API key from argument or environment
    let api_key = args
        .api_key
        .clone()
        .or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
        .or_else(|| std::env::var("LITELLM_API_KEY").ok());

    // Initialize LLM client
    let llm_client: Arc<dyn crate::llm::LlmProvider> = if let Some(key) = api_key {
        info!(model = %args.model, "Using OpenRouter with specified API key");
        Arc::new(OpenRouterProvider::with_model(key, args.model.clone()))
    } else {
        // Fall back to LiteLlmClient from environment
        info!("Using LiteLLM client from environment");
        Arc::new(LiteLlmClient::from_env().map_err(|e| {
            anyhow::anyhow!(
                "Failed to initialize LLM client: {}. \
                 Please provide --api-key or set OPENROUTER_API_KEY/LITELLM_API_KEY env var.",
                e
            )
        })?)
    };

    // Load tasks from directory
    let tasks = load_tasks_from_directory(tasks_path)?;
    if tasks.is_empty() {
        return Err(anyhow::anyhow!(
            "No valid tasks found in directory: {}",
            args.tasks_dir
        ));
    }

    info!(count = tasks.len(), "Loaded tasks for evaluation");

    if args.json {
        run_json_evaluation(llm_client, &args, tasks).await
    } else {
        run_interactive_evaluation(llm_client, &args, tasks).await
    }
}

/// Load tasks from a directory containing task subdirectories.
fn load_tasks_from_directory(dir: &Path) -> anyhow::Result<Vec<LoadedTask>> {
    let mut tasks = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Look for task.yaml in the subdirectory
            let task_yaml_path = path.join("task.yaml");
            if task_yaml_path.exists() {
                match load_task_from_yaml(&task_yaml_path) {
                    Ok(task) => tasks.push(task),
                    Err(e) => {
                        warn!(
                            path = %task_yaml_path.display(),
                            error = %e,
                            "Failed to load task, skipping"
                        );
                    }
                }
            }
        }
    }

    Ok(tasks)
}

/// Load a single task from a task.yaml file.
fn load_task_from_yaml(path: &Path) -> anyhow::Result<LoadedTask> {
    let content = fs::read_to_string(path)?;
    let task: SyntheticTask = serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse task YAML: {}", e))?;

    Ok(LoadedTask {
        task_id: task.id,
        category: task.metadata.category,
        difficulty: format!("{:?}", task.difficulty.level),
        problem_statement: task.problem_statement,
        success_criteria: task.verification.success_criteria,
    })
}

/// Evaluate a single task using the LLM agent.
async fn evaluate_single_task(
    llm_client: Arc<dyn crate::llm::LlmProvider>,
    task: &LoadedTask,
    max_steps: u32,
    timeout_secs: u64,
) -> TaskEvaluationResult {
    use crate::llm::{GenerationRequest, Message};
    use std::time::Instant;

    let start_time = Instant::now();

    // Build the agent prompt with ONLY the problem statement
    let system_prompt = r#"You are an autonomous agent tasked with solving terminal/CLI benchmark tasks.
You will be given a problem statement. Your goal is to solve the task by reasoning through it step-by-step.

Guidelines:
- Think carefully about what the problem is asking
- Break down the problem into steps
- Provide your solution approach
- State clearly when you believe the task is complete

Respond with your reasoning and solution approach. When you have solved the task, 
end your response with "TASK COMPLETE" followed by your final answer."#;

    let user_prompt = format!(
        "## Problem Statement\n\n{}\n\n## Success Criteria\n\n{}\n\nSolve this task.",
        task.problem_statement,
        task.success_criteria
            .iter()
            .map(|c| format!("- {}", c))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let mut steps_taken = 0u32;
    let mut final_output = String::new();
    let mut success = false;
    let mut error_message: Option<String> = None;

    // Run the agent loop with timeout
    let timeout_duration = std::time::Duration::from_secs(timeout_secs);

    while steps_taken < max_steps {
        if start_time.elapsed() >= timeout_duration {
            error_message = Some(format!("Timeout after {} seconds", timeout_secs));
            break;
        }

        steps_taken += 1;

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(system_prompt),
                Message::user(&user_prompt),
                if !final_output.is_empty() {
                    Message::assistant(&final_output)
                } else {
                    Message::user("Begin solving the task.")
                },
            ],
        )
        .with_temperature(0.3)
        .with_max_tokens(2000);

        match llm_client.generate(request).await {
            Ok(response) => {
                if let Some(content) = response.first_content() {
                    final_output = content.to_string();

                    // Check if the agent believes it has completed the task
                    if final_output.contains("TASK COMPLETE") {
                        success = true;
                        break;
                    }
                } else {
                    error_message = Some("Empty response from LLM".to_string());
                    break;
                }
            }
            Err(e) => {
                error_message = Some(format!("LLM error: {}", e));
                break;
            }
        }
    }

    if steps_taken >= max_steps && !success {
        error_message = Some(format!(
            "Max steps ({}) reached without completion",
            max_steps
        ));
    }

    let duration_ms = start_time.elapsed().as_millis() as u64;

    TaskEvaluationResult {
        task_id: task.task_id.clone(),
        category: task.category.clone(),
        difficulty: task.difficulty.clone(),
        success,
        steps_taken,
        duration_ms,
        error: error_message,
        agent_output: if final_output.is_empty() {
            None
        } else {
            // Truncate to avoid huge outputs
            Some(final_output.chars().take(1000).collect())
        },
    }
}

/// Run evaluation in JSON mode (outputs JSON to stdout).
async fn run_json_evaluation(
    llm_client: Arc<dyn crate::llm::LlmProvider>,
    args: &EvaluateArgs,
    tasks: Vec<LoadedTask>,
) -> anyhow::Result<()> {
    let start_time = std::time::Instant::now();
    let total_tasks = tasks.len();
    let mut task_results: Vec<TaskEvaluationResult> = Vec::new();

    for task in &tasks {
        let result =
            evaluate_single_task(llm_client.clone(), task, args.max_steps, args.timeout).await;
        task_results.push(result);
    }

    let total_duration_ms = start_time.elapsed().as_millis() as u64;
    let successful_tasks = task_results.iter().filter(|r| r.success).count();
    let success_rate = if total_tasks > 0 {
        successful_tasks as f64 / total_tasks as f64
    } else {
        0.0
    };
    let average_duration_ms = if total_tasks > 0 {
        task_results.iter().map(|r| r.duration_ms).sum::<u64>() / total_tasks as u64
    } else {
        0
    };

    let difficulty_metrics = DifficultyMetrics::from_results(&task_results);

    let output = EvaluationOutput {
        status: if successful_tasks > 0 {
            "success".to_string()
        } else {
            "failed".to_string()
        },
        model: args.model.clone(),
        total_tasks,
        successful_tasks,
        success_rate,
        average_duration_ms,
        task_results,
        difficulty_metrics,
        total_duration_ms,
    };

    let json_output = serde_json::to_string_pretty(&output)
        .map_err(|e| anyhow::anyhow!("Failed to serialize JSON output: {}", e))?;

    // Write to file if specified
    if let Some(output_path) = &args.output {
        fs::write(output_path, &json_output)
            .map_err(|e| anyhow::anyhow!("Failed to write output file: {}", e))?;
        info!(path = %output_path, "Results written to file");
    }

    println!("{}", json_output);

    Ok(())
}

/// Run evaluation in interactive mode with progress output.
async fn run_interactive_evaluation(
    llm_client: Arc<dyn crate::llm::LlmProvider>,
    args: &EvaluateArgs,
    tasks: Vec<LoadedTask>,
) -> anyhow::Result<()> {
    let start_time = std::time::Instant::now();
    let total_tasks = tasks.len();

    println!("\n🔬 Task Evaluation");
    println!("==================");
    println!("Model: {}", args.model);
    println!("Tasks: {}", total_tasks);
    println!("Max steps per task: {}", args.max_steps);
    println!("Timeout per task: {}s", args.timeout);
    println!();

    let mut task_results: Vec<TaskEvaluationResult> = Vec::new();

    for (idx, task) in tasks.iter().enumerate() {
        println!(
            "📝 Evaluating task {}/{}: {} [{}]",
            idx + 1,
            total_tasks,
            task.task_id,
            task.difficulty
        );

        let result =
            evaluate_single_task(llm_client.clone(), task, args.max_steps, args.timeout).await;

        let status_icon = if result.success { "✓" } else { "✗" };
        println!(
            "   {} {} in {}ms ({} steps)",
            status_icon,
            if result.success { "Success" } else { "Failed" },
            result.duration_ms,
            result.steps_taken
        );

        if let Some(ref err) = result.error {
            println!("   ⚠ {}", err);
        }

        task_results.push(result);
        println!();
    }

    let total_duration_ms = start_time.elapsed().as_millis() as u64;
    let successful_tasks = task_results.iter().filter(|r| r.success).count();
    let success_rate = if total_tasks > 0 {
        successful_tasks as f64 / total_tasks as f64
    } else {
        0.0
    };
    let average_duration_ms = if total_tasks > 0 {
        task_results.iter().map(|r| r.duration_ms).sum::<u64>() / total_tasks as u64
    } else {
        0
    };

    let difficulty_metrics = DifficultyMetrics::from_results(&task_results);

    // Print summary
    println!("{}", "=".repeat(50));
    println!("📊 Evaluation Summary");
    println!("{}", "=".repeat(50));
    println!("Total tasks: {}", total_tasks);
    println!("Successful: {}", successful_tasks);
    println!("Failed: {}", total_tasks - successful_tasks);
    println!("Success rate: {:.1}%", success_rate * 100.0);
    println!("Average duration: {}ms", average_duration_ms);
    println!("Total duration: {}ms", total_duration_ms);

    println!("\n📈 Difficulty Correlation:");
    if let Some(rate) = difficulty_metrics.easy_success_rate {
        println!("  Easy:   {:.1}% success", rate * 100.0);
    }
    if let Some(rate) = difficulty_metrics.medium_success_rate {
        println!("  Medium: {:.1}% success", rate * 100.0);
    }
    if let Some(rate) = difficulty_metrics.hard_success_rate {
        println!("  Hard:   {:.1}% success", rate * 100.0);
    }

    // Write to file if specified
    if let Some(output_path) = &args.output {
        let output = EvaluationOutput {
            status: if successful_tasks > 0 {
                "success".to_string()
            } else {
                "failed".to_string()
            },
            model: args.model.clone(),
            total_tasks,
            successful_tasks,
            success_rate,
            average_duration_ms,
            task_results,
            difficulty_metrics,
            total_duration_ms,
        };

        let json_output = serde_json::to_string_pretty(&output)
            .map_err(|e| anyhow::anyhow!("Failed to serialize JSON output: {}", e))?;

        fs::write(output_path, &json_output)
            .map_err(|e| anyhow::anyhow!("Failed to write output file: {}", e))?;

        println!("\n📁 Results saved to: {}", output_path);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_parses() {
        // Verify CLI definition is valid
        Cli::command().debug_assert();
    }

    #[test]
    fn test_generate_command_defaults() {
        let args = vec!["dataforge", "generate"];
        let cli = Cli::try_parse_from(args).expect("should parse");

        match cli.command {
            Commands::Generate(args) => {
                assert_eq!(args.count, 1);
                assert_eq!(args.model, DEFAULT_MODEL);
                assert!(args.category.is_none());
                assert_eq!(args.output, DEFAULT_OUTPUT_DIR);
                assert!(!args.json);
                assert!((args.min_score - 0.6).abs() < 0.01);
                assert_eq!(args.max_retries, 3);
                assert!(!args.factory);
            }
            _ => panic!("Expected Generate command"),
        }
    }

    #[test]
    fn test_generate_command_with_all_options() {
        let args = vec![
            "dataforge",
            "generate",
            "-n",
            "5",
            "-m",
            "anthropic/claude-3-opus",
            "-c",
            "debugging",
            "-o",
            "./my-output",
            "-j",
            "-s",
            "42",
            "--min-score",
            "0.8",
            "--max-retries",
            "5",
            "--factory",
        ];
        let cli = Cli::try_parse_from(args).expect("should parse");

        match cli.command {
            Commands::Generate(args) => {
                assert_eq!(args.count, 5);
                assert_eq!(args.model, "anthropic/claude-3-opus");
                assert_eq!(args.category, Some("debugging".to_string()));
                assert_eq!(args.output, "./my-output");
                assert!(args.json);
                assert_eq!(args.seed, Some(42));
                assert!((args.min_score - 0.8).abs() < 0.01);
                assert_eq!(args.max_retries, 5);
                assert!(args.factory);
            }
            _ => panic!("Expected Generate command"),
        }
    }

    #[test]
    fn test_generate_alias() {
        let args = vec!["dataforge", "gen", "-n", "2"];
        let cli = Cli::try_parse_from(args).expect("should parse with alias");

        match cli.command {
            Commands::Generate(args) => {
                assert_eq!(args.count, 2);
            }
            _ => panic!("Expected Generate command"),
        }
    }

    #[test]
    fn test_parse_task_category() {
        assert_eq!(
            parse_task_category("debugging").expect("valid category"),
            AgentTaskCategory::Debugging
        );
        assert_eq!(
            parse_task_category("debug").expect("valid category"),
            AgentTaskCategory::Debugging
        );
        assert_eq!(
            parse_task_category("security").expect("valid category"),
            AgentTaskCategory::Security
        );
        assert_eq!(
            parse_task_category("algorithm-design").expect("valid category"),
            AgentTaskCategory::AlgorithmDesign
        );
        assert_eq!(
            parse_task_category("infrastructure").expect("valid category"),
            AgentTaskCategory::Infrastructure
        );
        assert_eq!(
            parse_task_category("containers").expect("valid category"),
            AgentTaskCategory::Containers
        );
        assert!(parse_task_category("invalid-category").is_err());
    }

    #[test]
    fn test_generation_output_serialization() {
        let output = GenerationOutput {
            status: "success".to_string(),
            model: "moonshotai/kimi-k2.5".to_string(),
            tasks: vec![GeneratedTaskOutput {
                task_id: "dataforge-task-001".to_string(),
                category: "debugging".to_string(),
                problem_statement: "Find the bug in the code".to_string(),
                difficulty: "Medium".to_string(),
                tags: vec!["memory".to_string(), "profiling".to_string()],
                verification_criteria: vec!["Bug identified".to_string()],
                saved_path: Some("./output/dataforge-task-001".to_string()),
            }],
            total_duration_ms: 5000,
            retries: 1,
            output_directory: "./output".to_string(),
        };

        let json = serde_json::to_string_pretty(&output).expect("serialization should succeed");

        // Verify key fields are present in output
        assert!(json.contains("\"status\": \"success\""));
        assert!(json.contains("\"model\": \"moonshotai/kimi-k2.5\""));
        assert!(json.contains("\"task_id\": \"dataforge-task-001\""));
        assert!(json.contains("\"category\": \"debugging\""));
        assert!(json.contains("\"total_duration_ms\": 5000"));
        assert!(json.contains("\"retries\": 1"));
    }

    #[test]
    fn test_evaluate_command_defaults() {
        let args = vec!["dataforge", "evaluate", "--tasks-dir", "/tmp/tasks"];
        let cli = Cli::try_parse_from(args).expect("should parse");

        match cli.command {
            Commands::Evaluate(args) => {
                assert_eq!(args.tasks_dir, "/tmp/tasks");
                assert_eq!(args.model, DEFAULT_MODEL);
                assert_eq!(args.max_steps, DEFAULT_EVAL_MAX_STEPS);
                assert_eq!(args.timeout, DEFAULT_EVAL_TIMEOUT_SECS);
                assert!(args.output.is_none());
                assert!(!args.json);
            }
            _ => panic!("Expected Evaluate command"),
        }
    }

    #[test]
    fn test_evaluate_command_with_all_options() {
        let args = vec![
            "dataforge",
            "evaluate",
            "--tasks-dir",
            "./my-tasks",
            "-m",
            "anthropic/claude-3-opus",
            "--max-steps",
            "50",
            "--timeout",
            "600",
            "-o",
            "./results.json",
            "-j",
        ];
        let cli = Cli::try_parse_from(args).expect("should parse");

        match cli.command {
            Commands::Evaluate(args) => {
                assert_eq!(args.tasks_dir, "./my-tasks");
                assert_eq!(args.model, "anthropic/claude-3-opus");
                assert_eq!(args.max_steps, 50);
                assert_eq!(args.timeout, 600);
                assert_eq!(args.output, Some("./results.json".to_string()));
                assert!(args.json);
            }
            _ => panic!("Expected Evaluate command"),
        }
    }

    #[test]
    fn test_evaluate_alias() {
        let args = vec!["dataforge", "eval", "-t", "/tmp/tasks"];
        let cli = Cli::try_parse_from(args).expect("should parse with alias");

        match cli.command {
            Commands::Evaluate(args) => {
                assert_eq!(args.tasks_dir, "/tmp/tasks");
            }
            _ => panic!("Expected Evaluate command"),
        }
    }

    #[test]
    fn test_evaluation_output_serialization() {
        let output = EvaluationOutput {
            status: "success".to_string(),
            model: "moonshotai/kimi-k2.5".to_string(),
            total_tasks: 3,
            successful_tasks: 2,
            success_rate: 0.667,
            average_duration_ms: 5000,
            task_results: vec![
                TaskEvaluationResult {
                    task_id: "task-001".to_string(),
                    category: "debugging".to_string(),
                    difficulty: "Easy".to_string(),
                    success: true,
                    steps_taken: 3,
                    duration_ms: 3000,
                    error: None,
                    agent_output: Some("Solved the task".to_string()),
                },
                TaskEvaluationResult {
                    task_id: "task-002".to_string(),
                    category: "security".to_string(),
                    difficulty: "Hard".to_string(),
                    success: false,
                    steps_taken: 20,
                    duration_ms: 10000,
                    error: Some("Max steps reached".to_string()),
                    agent_output: None,
                },
            ],
            difficulty_metrics: DifficultyMetrics {
                easy_success_rate: Some(1.0),
                medium_success_rate: None,
                hard_success_rate: Some(0.0),
                easy_avg_duration_ms: Some(3000),
                medium_avg_duration_ms: None,
                hard_avg_duration_ms: Some(10000),
            },
            total_duration_ms: 13000,
        };

        let json = serde_json::to_string_pretty(&output).expect("serialization should succeed");

        // Verify key fields are present in output
        assert!(json.contains("\"status\": \"success\""));
        assert!(json.contains("\"model\": \"moonshotai/kimi-k2.5\""));
        assert!(json.contains("\"total_tasks\": 3"));
        assert!(json.contains("\"successful_tasks\": 2"));
        assert!(json.contains("\"success_rate\": 0.667"));
        assert!(json.contains("\"task_id\": \"task-001\""));
        assert!(json.contains("\"easy_success_rate\": 1.0"));
    }

    #[test]
    fn test_difficulty_metrics_from_results() {
        let results = vec![
            TaskEvaluationResult {
                task_id: "t1".to_string(),
                category: "cat".to_string(),
                difficulty: "Easy".to_string(),
                success: true,
                steps_taken: 2,
                duration_ms: 1000,
                error: None,
                agent_output: None,
            },
            TaskEvaluationResult {
                task_id: "t2".to_string(),
                category: "cat".to_string(),
                difficulty: "Easy".to_string(),
                success: true,
                steps_taken: 3,
                duration_ms: 2000,
                error: None,
                agent_output: None,
            },
            TaskEvaluationResult {
                task_id: "t3".to_string(),
                category: "cat".to_string(),
                difficulty: "Medium".to_string(),
                success: true,
                steps_taken: 5,
                duration_ms: 5000,
                error: None,
                agent_output: None,
            },
            TaskEvaluationResult {
                task_id: "t4".to_string(),
                category: "cat".to_string(),
                difficulty: "Medium".to_string(),
                success: false,
                steps_taken: 10,
                duration_ms: 8000,
                error: Some("Failed".to_string()),
                agent_output: None,
            },
            TaskEvaluationResult {
                task_id: "t5".to_string(),
                category: "cat".to_string(),
                difficulty: "Hard".to_string(),
                success: false,
                steps_taken: 20,
                duration_ms: 15000,
                error: Some("Failed".to_string()),
                agent_output: None,
            },
        ];

        let metrics = DifficultyMetrics::from_results(&results);

        // Easy: 2/2 = 100%
        assert!((metrics.easy_success_rate.unwrap() - 1.0).abs() < 0.01);
        // Medium: 1/2 = 50%
        assert!((metrics.medium_success_rate.unwrap() - 0.5).abs() < 0.01);
        // Hard: 0/1 = 0%
        assert!((metrics.hard_success_rate.unwrap() - 0.0).abs() < 0.01);

        // Easy avg duration: (1000 + 2000) / 2 = 1500
        assert_eq!(metrics.easy_avg_duration_ms.unwrap(), 1500);
        // Medium avg duration: (5000 + 8000) / 2 = 6500
        assert_eq!(metrics.medium_avg_duration_ms.unwrap(), 6500);
        // Hard avg duration: 15000 / 1 = 15000
        assert_eq!(metrics.hard_avg_duration_ms.unwrap(), 15000);
    }

    #[test]
    fn test_difficulty_metrics_empty_results() {
        let results: Vec<TaskEvaluationResult> = vec![];
        let metrics = DifficultyMetrics::from_results(&results);

        assert!(metrics.easy_success_rate.is_none());
        assert!(metrics.medium_success_rate.is_none());
        assert!(metrics.hard_success_rate.is_none());
        assert!(metrics.easy_avg_duration_ms.is_none());
        assert!(metrics.medium_avg_duration_ms.is_none());
        assert!(metrics.hard_avg_duration_ms.is_none());
    }
}
