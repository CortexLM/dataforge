//! Simplified CLI command definitions for synth-bench.
//!
//! This module provides a streamlined command-line interface for generating
//! synthetic benchmark datasets in one shot.

use crate::agents::{
    FactoryOrchestrator, FactoryOrchestratorConfig, FactoryPipelineEvent, FactoryPipelineStage,
    SyntheticOrchestrator, SyntheticOrchestratorConfig, SyntheticPipelineEvent,
    SyntheticPipelineStage, SyntheticTask, TaskCategory as AgentTaskCategory,
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
#[command(name = "synth-bench")]
#[command(about = "Generate synthetic benchmark datasets for LLM evaluation")]
#[command(version)]
#[command(
    long_about = "synth-bench generates synthetic terminal/CLI benchmark tasks to evaluate AI agent capabilities.\n\nIt uses a multi-agent validation system to ensure generated tasks match the requested \ndifficulty level, are solvable but challenging, and meet quality standards.\n\nExample usage:\n  synth-bench generate --count 5 --model moonshotai/kimi-k2.5 --output ./datasets"
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
    #[arg(long)]
    pub factory: bool,

    /// Enable prompt caching for efficiency (only with --factory).
    #[arg(long, default_value = "true")]
    pub cache: bool,
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
/// This is the main entry point for the synth-bench CLI.
pub async fn run_with_cli(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Generate(args) => {
            run_generate_command(args).await?;
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

    if args.factory {
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
    // Configure the orchestrator
    let config = SyntheticOrchestratorConfig::default()
        .with_min_validation_score(min_score)
        .with_max_ideation_retries(args.max_retries);

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

    // Configure the factory orchestrator
    let config = FactoryOrchestratorConfig::default()
        .with_min_validation_score(min_score)
        .with_max_creation_retries(args.max_retries);

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
        let args = vec!["synth-bench", "generate"];
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
        }
    }

    #[test]
    fn test_generate_command_with_all_options() {
        let args = vec![
            "synth-bench",
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
        }
    }

    #[test]
    fn test_generate_alias() {
        let args = vec!["synth-bench", "gen", "-n", "2"];
        let cli = Cli::try_parse_from(args).expect("should parse with alias");

        match cli.command {
            Commands::Generate(args) => {
                assert_eq!(args.count, 2);
            }
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
                task_id: "synth-task-001".to_string(),
                category: "debugging".to_string(),
                problem_statement: "Find the bug in the code".to_string(),
                difficulty: "Medium".to_string(),
                tags: vec!["memory".to_string(), "profiling".to_string()],
                verification_criteria: vec!["Bug identified".to_string()],
                saved_path: Some("./output/synth-task-001".to_string()),
            }],
            total_duration_ms: 5000,
            retries: 1,
            output_directory: "./output".to_string(),
        };

        let json = serde_json::to_string_pretty(&output).expect("serialization should succeed");

        // Verify key fields are present in output
        assert!(json.contains("\"status\": \"success\""));
        assert!(json.contains("\"model\": \"moonshotai/kimi-k2.5\""));
        assert!(json.contains("\"task_id\": \"synth-task-001\""));
        assert!(json.contains("\"category\": \"debugging\""));
        assert!(json.contains("\"total_duration_ms\": 5000"));
        assert!(json.contains("\"retries\": 1"));
    }
}
