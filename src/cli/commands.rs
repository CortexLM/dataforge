//! Simplified CLI command definitions for swe_forge.
//!
//! This module provides a streamlined command-line interface for generating
//! SWE-derived benchmark datasets in one shot.

use crate::agents::{
    AntiMemorizationConfig, DifficultyScoring, DockerValidatorAgent, DockerValidatorConfig,
    HiddenSolution, SyntheticTask, TaskMetadata, VerificationSpec,
};
use crate::difficulty::DifficultyLevel;
use crate::llm::{LiteLlmClient, OpenRouterProvider};
use crate::swe::orchestrator::DifficultyTargets;
use crate::swe::{SweOrchestrator, SweOrchestratorConfig};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

/// Default model to use for generation.
const DEFAULT_MODEL: &str = "openai/gpt-5.2-codex:nitro";

/// Default output directory for generated datasets.
const DEFAULT_OUTPUT_DIR: &str = "./generated-datasets";
const DEFAULT_SWE_OUTPUT_DIR: &str = "./generated-swe";

/// SWE-derived benchmark dataset generator for LLM evaluation.
#[derive(Parser)]
#[command(name = "swe_forge")]
#[command(about = "Generate SWE-derived benchmark datasets for LLM evaluation")]
#[command(version)]
#[command(
    long_about = "swe_forge generates SWE-derived terminal/CLI benchmark tasks from mined GitHub PRs.\n\nTasks are validated and exported as workspace artifacts (workspace.yaml + prompt.md).\n\nExample usage:\n  swe_forge generate --count 5 --model openai/gpt-5.2-codex:nitro --output ./generated-datasets"
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
    /// Generate SWE DataForge tasks from real GitHub PRs.
    #[command(alias = "gen")]
    Generate(GenerateArgs),

    /// Evaluate generated tasks using an autonomous agent.
    ///
    /// This command runs an autonomous agent against generated benchmark tasks,
    /// providing only the problem statement without any hints. It measures success
    /// rate, duration, and correlates results with task difficulty.
    #[command(alias = "eval")]
    Evaluate(EvaluateArgs),

    /// Run SWE mining pipeline against real GitHub history and export SWE datasets.
    #[command(name = "swe")]
    Swe(Box<SweArgs>),
}

/// SWE pipeline entrypoint arguments.
#[derive(Parser, Debug)]
pub struct SweArgs {
    /// SWE subcommand to run.
    #[command(subcommand)]
    pub command: SweSubcommand,
}

/// SWE subcommands.
#[derive(clap::Subcommand, Debug)]
pub enum SweSubcommand {
    /// Mine real PRs and export SWE DataForge-style tasks.
    Mine(SweMineArgs),

    /// Validate mined SWE tasks with quality scoring and optional Docker probes.
    Validate(SweValidateArgs),

    /// Re-export existing SWE workspaces.
    Export(SweExportArgs),

    /// Run evaluation harness: execute an agent on tasks, then verify with tests.
    Harness(SweHarnessArgs),

    /// Load a dataset from HuggingFace or local parquet for inspection/evaluation.
    Load(SweLoadArgs),

    /// Run a benchmark on N PRs and output detailed pipeline metrics as JSON.
    Benchmark(SweBenchmarkArgs),
}

/// Arguments for `swe_forge swe mine`.
#[derive(Parser, Debug)]
pub struct SweMineArgs {
    /// Number of SWE tasks to emit.
    #[arg(short = 'n', long, default_value = "1")]
    pub max_tasks: usize,

    /// Return after mining the first accepted task.
    #[arg(long, default_value = "true")]
    pub once: bool,

    /// Minimum repo stars for a PR to be accepted.
    #[arg(long, default_value = "20")]
    pub min_stars: u32,

    /// Comma-separated allowed languages (e.g. python,rust,go).
    #[arg(long)]
    pub languages: Option<String>,

    /// JSONL file of already-processed PRs to skip (one {"repo":"...","pr":N} per line).
    /// New PRs will be appended to this file after export.
    #[arg(long)]
    pub pr_file: Option<String>,

    /// Only keep tasks matching this difficulty level (easy, medium, hard).
    #[arg(long, conflicts_with = "difficulty_targets")]
    pub difficulty: Option<String>,

    /// Mine multiple difficulty levels in a single pipeline run with per-level quotas.
    /// Format: "easy:50,medium:50,hard:50". Overrides --difficulty and --max-tasks.
    #[arg(long)]
    pub difficulty_targets: Option<String>,

    /// Output directory for generated SWE workspaces.
    #[arg(short = 'o', long, default_value = DEFAULT_SWE_OUTPUT_DIR)]
    pub output: String,

    /// LLM model to use for supplemental test generation and scoring.
    #[arg(short = 'm', long, default_value = DEFAULT_MODEL)]
    pub model: String,

    /// Enable Docker validation when exporting SWE tasks.
    #[arg(long)]
    pub validate_docker: bool,

    /// OpenRouter API key (can also be set via OPENROUTER_API_KEY or LITELLM_API_KEY env var).
    #[arg(long, env = "OPENROUTER_API_KEY")]
    pub api_key: Option<String>,

    /// HuggingFace dataset repo to upload to (e.g. "myorg/swe-forge-bench").
    /// Requires --hf-token.
    #[arg(long)]
    pub hf_repo: Option<String>,

    /// HuggingFace API token for dataset upload (can also use HF_TOKEN env var).
    #[arg(long, env = "HF_TOKEN")]
    pub hf_token: Option<String>,

    /// Make the HuggingFace dataset repo private.
    #[arg(long)]
    pub hf_private: bool,

    /// SQLite cache database for PR deduplication and triage caching.
    /// Avoids re-running LLM pre-classification on already-seen PRs.
    #[arg(long, default_value = "swe_cache.db")]
    pub cache_db: String,

    /// Override Docker image for mining containers (default: auto-select based on language).
    /// All repo cloning, test execution, and validation run inside Docker.
    #[arg(long)]
    pub mining_image: Option<String>,

    /// Data source for PR discovery: "gharchive" (default) or "github-search".
    #[arg(long, default_value = "gharchive")]
    pub source: String,

    /// Enable pre-export workspace validation in a fresh Docker container.
    /// Verifies install, fail_to_pass, pass_to_pass, and patch application.
    #[arg(long, default_value = "true")]
    pub validate_workspace: bool,

    /// Override enrichment concurrency (default: 10).
    #[arg(long)]
    pub concurrency_enrich: Option<usize>,

    /// Override deep processing concurrency (default: 8).
    #[arg(long)]
    pub concurrency_deep: Option<usize>,

    /// Output JSON summary.
    #[arg(short = 'j', long)]
    pub json: bool,
}

/// Arguments for `swe_forge swe benchmark`.
#[derive(Parser, Debug)]
pub struct SweBenchmarkArgs {
    /// Number of candidate PRs to process through the pipeline.
    #[arg(short = 'n', long, default_value = "100")]
    pub count: usize,

    /// Minimum repo stars for a PR to be accepted.
    #[arg(long, default_value = "20")]
    pub min_stars: u32,

    /// Comma-separated allowed languages (e.g. python,rust,go).
    #[arg(long)]
    pub languages: Option<String>,

    /// LLM model to use for classification and scoring.
    #[arg(short = 'm', long, default_value = DEFAULT_MODEL)]
    pub model: String,

    /// OpenRouter API key (can also be set via OPENROUTER_API_KEY env var).
    #[arg(long, env = "OPENROUTER_API_KEY")]
    pub api_key: Option<String>,

    /// SQLite cache database for PR deduplication and triage caching.
    #[arg(long, default_value = "benchmark_cache.db")]
    pub cache_db: String,

    /// Output directory for benchmark task artifacts.
    #[arg(short = 'o', long, default_value = "./benchmark-output")]
    pub output: String,

    /// Data source for PR discovery: "gharchive" (default) or "github-search".
    #[arg(long, default_value = "gharchive")]
    pub source: String,

    /// Override enrichment concurrency (default: 10).
    #[arg(long)]
    pub concurrency_enrich: Option<usize>,

    /// Override deep processing concurrency (default: 8).
    #[arg(long)]
    pub concurrency_deep: Option<usize>,
}

/// Arguments for `swe_forge swe validate`.
#[derive(Parser, Debug)]
pub struct SweValidateArgs {
    /// Directory containing SWE workspaces.
    #[arg(short = 'd', long, default_value = DEFAULT_SWE_OUTPUT_DIR)]
    pub input: String,

    /// OpenRouter model to use for optional quality rescoring.
    #[arg(short = 'm', long, default_value = DEFAULT_MODEL)]
    pub model: String,

    /// OpenRouter API key (can also be set via OPENROUTER_API_KEY or LITELLM_API_KEY env var).
    #[arg(long, env = "OPENROUTER_API_KEY")]
    pub api_key: Option<String>,

    /// Enable Docker environment validation using SWE quality-gated execution.
    #[arg(long)]
    pub validate_docker: bool,

    /// Return validation result as JSON.
    #[arg(short = 'j', long)]
    pub json: bool,
}

/// Arguments for `swe_forge swe export`.
#[derive(Parser, Debug)]
pub struct SweExportArgs {
    /// Input directory with existing SWE workspace artifacts.
    #[arg(short = 'i', long, default_value = DEFAULT_SWE_OUTPUT_DIR)]
    pub input: String,

    /// Output directory for re-exported workspaces.
    #[arg(short = 'o', long, default_value = "./exported-swe")]
    pub output: String,

    /// Return export summary as JSON.
    #[arg(short = 'j', long)]
    pub json: bool,
}

/// Arguments for `swe_forge swe harness`.
#[derive(Parser, Debug)]
pub struct SweHarnessArgs {
    /// Directory containing SWE workspaces (from `swe mine`).
    #[arg(short = 'i', long, default_value = DEFAULT_SWE_OUTPUT_DIR)]
    pub input: String,

    /// Path to the agent directory (must contain requirements.txt).
    #[arg(long)]
    pub agent_dir: String,

    /// Command to run the agent inside the container.
    #[arg(long, default_value = "python -m baseagent")]
    pub agent_cmd: String,

    /// Agent timeout in seconds.
    #[arg(long, default_value = "600")]
    pub agent_timeout: u64,

    /// Per-test command timeout in seconds.
    #[arg(long, default_value = "120")]
    pub test_timeout: u64,

    /// Base Docker image for the container.
    #[arg(long, default_value = "python:3.12-slim")]
    pub docker_image: String,

    /// Number of concurrent evaluations.
    #[arg(long, default_value = "1")]
    pub parallel: usize,

    /// Keep containers after evaluation (for debugging).
    #[arg(long)]
    pub keep_containers: bool,

    /// Output results as JSON.
    #[arg(short = 'j', long)]
    pub json: bool,
}

/// Arguments for `swe_forge swe load`.
#[derive(Parser, Debug)]
pub struct SweLoadArgs {
    /// HuggingFace dataset repo ID (e.g. "princeton-nlp/SWE-bench") or local path.
    #[arg(short = 'i', long)]
    pub source: String,

    /// Dataset split to load (e.g. "train", "easy", "medium", "hard").
    #[arg(long)]
    pub split: Option<String>,

    /// Output directory to save the downloaded parquet file.
    #[arg(short = 'o', long, default_value = "./downloaded-dataset")]
    pub output: String,

    /// Output JSON summary of loaded dataset.
    #[arg(short = 'j', long)]
    pub json: bool,
}

/// Arguments for the generate command.
#[derive(Parser, Debug)]
pub struct GenerateArgs {
    /// Number of datasets to generate.
    #[arg(short = 'n', long, default_value = "1")]
    pub count: u32,

    /// LLM model to use for generation (OpenRouter format).
    ///
    /// OpenRouter model identifier.
    #[arg(short = 'm', long, default_value = DEFAULT_MODEL)]
    pub model: String,

    /// Category / language filter for generated tasks.
    ///
    /// Examples: debugging, python, rust, go.
    #[arg(short = 'c', long)]
    pub category: Option<String>,

    /// Filter languages (comma-separated).
    #[arg(long)]
    pub languages: Option<String>,

    /// Minimum repo stars for accepted tasks.
    #[arg(long, default_value = "20")]
    pub min_stars: u32,

    /// Output directory for generated datasets.
    #[arg(short = 'o', long, default_value = DEFAULT_OUTPUT_DIR)]
    pub output: String,

    /// Output JSON to stdout instead of interactive progress.
    #[arg(short = 'j', long)]
    pub json: bool,

    /// OpenRouter API key (can also be set via OPENROUTER_API_KEY or LITELLM_API_KEY env var).
    #[arg(long, env = "OPENROUTER_API_KEY")]
    pub api_key: Option<String>,

    /// Enable Docker validation to verify tasks are executable in containers.
    #[arg(long)]
    pub validate_docker: bool,

    /// Disable Docker validation (useful in CI without Docker).
    #[arg(long, conflicts_with = "validate_docker")]
    pub no_docker: bool,
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
    /// OpenRouter model identifier.
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
/// This is the main entry point for the swe_forge CLI.
pub async fn run_with_cli(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Generate(args) => {
            run_generate_command(args).await?;
        }
        Commands::Evaluate(args) => {
            run_evaluate_command(args).await?;
        }
        Commands::Swe(args) => {
            run_swe_command(*args).await?;
        }
    }
    Ok(())
}

// ============================================================================
// SWE Command Implementation
// ============================================================================

async fn run_swe_command(args: SweArgs) -> anyhow::Result<()> {
    match args.command {
        SweSubcommand::Mine(args) => run_swe_mine_command(args).await,
        SweSubcommand::Validate(args) => run_swe_validate_command(args).await,
        SweSubcommand::Export(args) => run_swe_export_command(args).await,
        SweSubcommand::Harness(args) => run_swe_harness_command(args).await,
        SweSubcommand::Load(args) => run_swe_load_command(args).await,
        SweSubcommand::Benchmark(args) => run_swe_benchmark_command(args).await,
    }
}

#[derive(Debug, Clone, Serialize)]
struct SweValidateEntry {
    task_id: String,
    repo: String,
    quality_score: f64,
    quality_passed: bool,
    docker_passed: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct SweValidateOutput {
    status: String,
    tasks: usize,
    quality_passed: usize,
    docker_requested: bool,
    docker_passed: usize,
    results: Vec<SweValidateEntry>,
}

async fn run_swe_validate_command(args: SweValidateArgs) -> anyhow::Result<()> {
    let input_dir = Path::new(&args.input);
    if !input_dir.exists() {
        return Err(anyhow::anyhow!(
            "Input directory does not exist: {}",
            args.input
        ));
    }

    let llm_client = build_llm_client(args.api_key, args.model).await?;
    let quality_scorer = crate::swe::QualityScorer::new(llm_client.clone(), Default::default());

    let docker_validator = if args.validate_docker {
        Some(DockerValidatorAgent::with_defaults().map_err(|e| {
            anyhow::anyhow!(
                "Docker validator is unavailable; rerun without `--validate-docker` or install Docker: {}",
                e
            )
        })?)
    } else {
        None
    };

    let mut total = 0usize;
    let mut quality_ok = 0usize;
    let mut docker_passed = 0usize;
    let mut results = Vec::new();

    for entry in fs::read_dir(input_dir)? {
        let entry = entry?;
        let task_dir = entry.path();
        if !task_dir.is_dir() {
            continue;
        }

        let workspace_yaml = task_dir.join("workspace.yaml");
        if !workspace_yaml.exists() {
            continue;
        }

        let mut task = load_swe_workspace_task(&workspace_yaml)?;
        total = total.saturating_add(1);

        let assessment = quality_scorer.assess(&task).await?;
        task.quality_score = Some(assessment.score);
        task.quality_passed = assessment.passed;

        let mut task_docker_passed = None;
        if assessment.passed {
            if args.validate_docker {
                if let Some(validator) = &docker_validator {
                    let config = DockerValidatorConfig::new().with_solution_validation(false);
                    let task = synthetic_task_from_swe_task(&task, &config);
                    let docker_result = validator.validate_task(&task).await;
                    match docker_result {
                        Ok(result) => {
                            task_docker_passed = Some(result.passed);
                            if result.passed {
                                docker_passed = docker_passed.saturating_add(1);
                            }
                        }
                        Err(_) => {
                            task_docker_passed = Some(false);
                        }
                    }
                }
            }
            quality_ok = quality_ok.saturating_add(1);
        }

        results.push(SweValidateEntry {
            task_id: task.id,
            repo: task.repo,
            quality_score: task.quality_score.unwrap_or(0.0),
            quality_passed: task.quality_passed,
            docker_passed: task_docker_passed,
        });
    }

    let passed = if args.validate_docker {
        results
            .iter()
            .filter(|r| r.quality_passed && r.docker_passed.unwrap_or(false))
            .count()
    } else {
        quality_ok
    };

    let output = SweValidateOutput {
        status: if passed > 0 {
            "success".to_string()
        } else {
            "failed".to_string()
        },
        tasks: total,
        quality_passed: quality_ok,
        docker_requested: args.validate_docker,
        docker_passed,
        results,
    };

    let json_output = serde_json::to_string_pretty(&output)
        .map_err(|e| anyhow::anyhow!("Failed to serialize JSON output: {}", e))?;

    if args.json {
        println!("{}", json_output);
        return Ok(());
    }

    println!("{}", json_output);
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
struct SweExportOutput {
    status: String,
    source: String,
    destination: String,
    copied: usize,
    skipped: usize,
}

async fn run_swe_harness_command(args: SweHarnessArgs) -> anyhow::Result<()> {
    use crate::swe::harness;

    let input_dir = Path::new(&args.input);
    if !input_dir.exists() {
        return Err(anyhow::anyhow!(
            "Input directory does not exist: {}",
            args.input
        ));
    }

    let agent_dir = Path::new(&args.agent_dir);
    if !agent_dir.exists() {
        return Err(anyhow::anyhow!(
            "Agent directory does not exist: {}",
            args.agent_dir
        ));
    }

    let config = harness::HarnessConfig {
        agent_dir: agent_dir.to_path_buf(),
        agent_cmd: args.agent_cmd,
        agent_timeout_secs: args.agent_timeout,
        test_timeout_secs: args.test_timeout,
        docker_image: args.docker_image,
        keep_containers: args.keep_containers,
        parallel: args.parallel,
    };

    info!(
        "Running SWE harness on {} with agent from {}",
        args.input, args.agent_dir
    );
    let summary = harness::run_harness(input_dir, &config).await?;

    if args.json {
        let json = serde_json::to_string_pretty(&summary)?;
        println!("{json}");
    } else {
        println!("\n=== SWE Harness Results ===");
        println!("Total tasks:    {}", summary.total);
        println!("Resolved:       {}", summary.resolved);
        println!("Unresolved:     {}", summary.unresolved);
        println!("Agent errors:   {}", summary.agent_error);
        println!("Test errors:    {}", summary.test_error);
        println!("Setup errors:   {}", summary.setup_error);
        println!("Sanity failures:{}", summary.sanity_fail);
        println!("Avg agent time: {:.1}s", summary.avg_agent_time_secs);
        println!();

        for r in &summary.results {
            let f2p_ok = r.fail_to_pass.iter().filter(|t| t.passed).count();
            let p2p_ok = r.pass_to_pass.iter().filter(|t| t.passed).count();
            println!(
                "  {} [{}] f2p={}/{} p2p={}/{} agent={:.1}s",
                r.task_id,
                r.status,
                f2p_ok,
                r.fail_to_pass.len(),
                p2p_ok,
                r.pass_to_pass.len(),
                r.agent_duration_secs,
            );
            if let Some(err) = &r.error {
                println!("    error: {err}");
            }
        }
    }

    Ok(())
}

async fn run_swe_export_command(args: SweExportArgs) -> anyhow::Result<()> {
    let source_dir = Path::new(&args.input);
    let destination_dir = Path::new(&args.output);

    if !source_dir.exists() {
        return Err(anyhow::anyhow!(
            "Input directory does not exist: {}",
            args.input
        ));
    }

    fs::create_dir_all(destination_dir)?;

    let mut copied = 0usize;
    let mut skipped = 0usize;

    for entry in fs::read_dir(source_dir)? {
        let entry = entry?;
        let task_dir = entry.path();
        if !task_dir.is_dir() {
            continue;
        }

        let workspace_yaml = task_dir.join("workspace.yaml");
        if !workspace_yaml.exists() {
            skipped = skipped.saturating_add(1);
            continue;
        }

        let task = load_swe_workspace_task(&workspace_yaml)?;
        let task_output = destination_dir.join(&task.id);
        fs::create_dir_all(&task_output)?;

        fs::copy(&workspace_yaml, task_output.join("workspace.yaml"))?;
        let prompt_md = task_dir.join("prompt.md");
        if prompt_md.exists() {
            fs::copy(&prompt_md, task_output.join("prompt.md"))?;
        }
        let checks = task_dir.join("checks.txt");
        if checks.exists() {
            fs::copy(&checks, task_output.join("checks.txt"))?;
        }

        copied = copied.saturating_add(1);
    }

    let output = SweExportOutput {
        status: if copied > 0 {
            "success".to_string()
        } else {
            "failed".to_string()
        },
        source: args.input,
        destination: args.output,
        copied,
        skipped,
    };

    let json_output = serde_json::to_string_pretty(&output)
        .map_err(|e| anyhow::anyhow!("Failed to serialize JSON output: {}", e))?;

    if args.json {
        println!("{}", json_output);
        return Ok(());
    }

    if output.status == "failed" {
        warn!(
            "No workspace artifacts were exported from {}",
            output.source
        );
    }

    println!("{}", json_output);
    Ok(())
}

async fn run_swe_mine_command(args: SweMineArgs) -> anyhow::Result<()> {
    // --- Validate required env upfront ---
    if std::env::var("GITHUB_TOKEN").is_err()
        && std::env::var("GITHUB_PERSONAL_ACCESS_TOKEN").is_err()
    {
        anyhow::bail!(
            "GITHUB_TOKEN is required but not set.\n\
             Set the GITHUB_TOKEN environment variable before running this command."
        );
    }

    let languages = parse_language_filter(args.languages.as_deref().unwrap_or_default());
    let output_dir = args.output.clone();
    let api_key = args
        .api_key
        .clone()
        .or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
        .or_else(|| std::env::var("LITELLM_API_KEY").ok());

    if api_key.is_none() {
        anyhow::bail!(
            "OPENROUTER_API_KEY is required but not set.\n\
             Provide it via --api-key <KEY> or set the OPENROUTER_API_KEY environment variable."
        );
    }

    let llm_client: Arc<dyn crate::llm::LlmProvider> = {
        let key = api_key.unwrap();
        info!(model = %args.model, "Using OpenRouter with specified API key");
        Arc::new(OpenRouterProvider::with_model(key, args.model.clone()))
    };

    let output_path = Path::new(&args.output);
    fs::create_dir_all(output_path)?;

    let skip_prs = load_skip_prs(args.pr_file.as_deref())?;

    let difficulty_targets = match args.difficulty_targets {
        Some(ref raw) => Some(DifficultyTargets::parse(raw)?),
        None => None,
    };

    let (effective_max_tasks, effective_difficulty_filter) =
        if let Some(ref dt) = difficulty_targets {
            (dt.total_tasks(), None)
        } else {
            (args.max_tasks, args.difficulty.clone())
        };

    let hf_upload = match (&args.hf_repo, &args.hf_token) {
        (Some(repo), Some(token)) => Some(crate::export::HfUploadConfig {
            repo_id: repo.clone(),
            token: token.clone(),
            private: args.hf_private,
        }),
        (Some(_), None) => {
            anyhow::bail!("--hf-repo requires --hf-token (or set HF_TOKEN env var)");
        }
        _ => None,
    };

    let pr_cache = crate::swe::PrCache::open(&args.cache_db).await?;
    let cache = crate::swe::OptionalCache::some(pr_cache);

    let config = SweOrchestratorConfig {
        output_dir: output_dir.clone(),
        min_stars: args.min_stars,
        languages,
        max_tasks: effective_max_tasks,
        once: args.once,
        validate_docker: args.validate_docker,
        skip_prs,
        pr_file: args.pr_file.clone(),
        difficulty_filter: effective_difficulty_filter,
        difficulty_targets,
        hf_upload,
        cache: cache.clone(),
        mining_image: args.mining_image.clone(),
        validate_workspace: args.validate_workspace,
        concurrency_enrich: args.concurrency_enrich,
        concurrency_deep: args.concurrency_deep,
    };

    let orchestrator = SweOrchestrator::new(llm_client, config);
    let result = orchestrator.mine().await?;

    if args.json {
        #[derive(Serialize)]
        struct SweMineOutput {
            status: String,
            attempted: usize,
            passed: usize,
            skipped: usize,
            finished_at: String,
            tasks: usize,
        }

        let output = SweMineOutput {
            status: if result.passed > 0 {
                "success".to_string()
            } else {
                "failed".to_string()
            },
            attempted: result.attempted,
            passed: result.passed,
            skipped: result.skipped,
            finished_at: result.finished_at,
            tasks: result.tasks.len(),
        };
        let json_output = serde_json::to_string_pretty(&output)
            .map_err(|e| anyhow::anyhow!("Failed to serialize JSON output: {}", e))?;
        println!("{}", json_output);
    } else {
        info!(
            attempted = result.attempted,
            passed = result.passed,
            skipped = result.skipped,
            finished_at = result.finished_at
        );
        println!("✓ SWE mine completed");
        println!("  Output dir: {}", output_dir);
        println!(
            "  Tasks: {} attempted, {} passed, {} skipped",
            result.attempted, result.passed, result.skipped
        );

        // Show per-difficulty breakdown
        let mut per_level: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for task in &result.tasks {
            if task.quality_passed {
                let level = task
                    .meta
                    .get("difficulty")
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                *per_level.entry(level).or_insert(0) += 1;
            }
        }
        if !per_level.is_empty() {
            println!("  Per-difficulty breakdown:");
            for level in &["easy", "medium", "hard"] {
                if let Some(&count) = per_level.get(*level) {
                    println!("    {}: {}", level, count);
                }
            }
        }
    }

    Ok(())
}

async fn run_swe_benchmark_command(args: SweBenchmarkArgs) -> anyhow::Result<()> {
    if std::env::var("GITHUB_TOKEN").is_err()
        && std::env::var("GITHUB_PERSONAL_ACCESS_TOKEN").is_err()
    {
        anyhow::bail!(
            "GITHUB_TOKEN is required but not set.\n\
             Set the GITHUB_TOKEN environment variable before running this command."
        );
    }

    let languages = parse_language_filter(args.languages.as_deref().unwrap_or_default());
    let api_key = args
        .api_key
        .clone()
        .or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
        .or_else(|| std::env::var("LITELLM_API_KEY").ok());

    if api_key.is_none() {
        anyhow::bail!(
            "OPENROUTER_API_KEY is required but not set.\n\
             Provide it via --api-key <KEY> or set the OPENROUTER_API_KEY environment variable."
        );
    }

    let llm_client: Arc<dyn crate::llm::LlmProvider> = {
        let key = api_key.unwrap();
        info!(model = %args.model, "Using OpenRouter for benchmark");
        Arc::new(OpenRouterProvider::with_model(key, args.model.clone()))
    };

    let output_dir = args.output.clone();
    fs::create_dir_all(&output_dir)?;

    let pr_cache = crate::swe::PrCache::open(&args.cache_db).await?;
    let cache = crate::swe::OptionalCache::some(pr_cache);

    let config = SweOrchestratorConfig {
        output_dir: output_dir.clone(),
        min_stars: args.min_stars,
        languages,
        max_tasks: args.count,
        once: true,
        validate_docker: false,
        skip_prs: HashSet::new(),
        pr_file: None,
        difficulty_filter: None,
        difficulty_targets: None,
        hf_upload: None,
        cache,
        mining_image: None,
        validate_workspace: false,
        concurrency_enrich: args.concurrency_enrich,
        concurrency_deep: args.concurrency_deep,
    };

    let orchestrator = SweOrchestrator::new(llm_client, config);
    let result = orchestrator.mine().await?;

    let json_output = serde_json::to_string_pretty(&result)
        .map_err(|e| anyhow::anyhow!("Failed to serialize benchmark JSON: {}", e))?;
    println!("{}", json_output);

    Ok(())
}

async fn run_swe_load_command(args: SweLoadArgs) -> anyhow::Result<()> {
    let source = &args.source;
    let output_dir = Path::new(&args.output);

    let tasks = if source.contains('/') && !Path::new(source).exists() {
        // Looks like a HuggingFace repo ID
        info!(repo = source, "Downloading dataset from HuggingFace");
        crate::export::download_dataset(source, args.split.as_deref(), output_dir).await?
    } else {
        // Local path
        let path = Path::new(source);
        crate::export::load_dataset(path)?
    };

    if tasks.is_empty() {
        warn!("No tasks found in dataset");
        if args.json {
            println!("{{\"status\":\"empty\",\"tasks\":0}}");
        } else {
            println!("No tasks found.");
        }
        return Ok(());
    }

    // Compute stats
    let mut by_difficulty: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut by_language: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for task in &tasks {
        let diff = task
            .meta
            .get("difficulty")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        *by_difficulty.entry(diff).or_insert(0) += 1;
        *by_language.entry(task.language.clone()).or_insert(0) += 1;
    }

    if args.json {
        let output = serde_json::json!({
            "status": "success",
            "total_tasks": tasks.len(),
            "by_difficulty": by_difficulty,
            "by_language": by_language,
            "source": source,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("Loaded {} tasks from {}", tasks.len(), source);
        println!("  By difficulty:");
        for level in &["easy", "medium", "hard"] {
            if let Some(&count) = by_difficulty.get(*level) {
                println!("    {}: {}", level, count);
            }
        }
        println!("  By language:");
        let mut langs: Vec<_> = by_language.iter().collect();
        langs.sort_by(|a, b| b.1.cmp(a.1));
        for (lang, count) in langs.iter().take(10) {
            println!("    {}: {}", lang, count);
        }
    }

    Ok(())
}

fn load_swe_workspace_task(path: &Path) -> anyhow::Result<crate::swe::SweTask> {
    let content = fs::read_to_string(path)?;
    serde_yaml::from_str(&content).map_err(|e| {
        anyhow::anyhow!(
            "Failed to parse SWE workspace yaml {}: {}",
            path.display(),
            e
        )
    })
}

fn synthetic_task_from_swe_task(
    task: &crate::swe::SweTask,
    _config: &DockerValidatorConfig,
) -> SyntheticTask {
    let mut tags = Vec::<String>::new();
    tags.push(format!("language:{}", task.language.to_lowercase()));
    tags.push(format!("repo:{}", task.repo));

    let mut metadata = TaskMetadata::new("swe", task.id.clone()).with_tags(tags);
    metadata.subcategory = "mined-pr".to_string();

    let hidden_solution = HiddenSolution::new(format!(
        "Solve the bug described in issue context for {}",
        task.id
    ))
    .with_reference_commands(task.fail_to_pass.clone())
    .with_expected_time_seconds(900)
    .with_step_count(5);

    let verification = VerificationSpec::new().with_success_criteria(
        task.fail_to_pass
            .iter()
            .chain(task.pass_to_pass.iter())
            .cloned()
            .collect::<Vec<_>>(),
    );

    let difficulty = match task.difficulty_score {
        0 => DifficultyScoring::new(DifficultyLevel::Easy),
        1..=2 => DifficultyScoring::new(DifficultyLevel::Medium),
        _ => DifficultyScoring::new(DifficultyLevel::Hard),
    };

    let mut synthetic_task = SyntheticTask::new(
        task.prompt.clone(),
        hidden_solution,
        verification,
        difficulty,
        metadata,
    )
    .with_anti_memorization(AntiMemorizationConfig::default());
    synthetic_task.id = task.id.clone();

    synthetic_task
}

async fn build_llm_client(
    api_key: Option<String>,
    model: String,
) -> anyhow::Result<Arc<dyn crate::llm::LlmProvider>> {
    let resolved_api_key = api_key
        .or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
        .or_else(|| std::env::var("LITELLM_API_KEY").ok());

    if let Some(key) = resolved_api_key {
        info!(model = %model, "Using OpenRouter with specified API key");
        Ok(Arc::new(OpenRouterProvider::with_model(key, model)))
    } else {
        info!("Using LiteLLM client from environment");
        Ok(Arc::new(LiteLlmClient::from_env().map_err(|e| {
            anyhow::anyhow!(
                "Failed to initialize LLM client: {}. Please provide --api-key or set OPENROUTER_API_KEY/LITELLM_API_KEY env var.",
                e
            )
        })?))
    }
}

fn load_skip_prs(path: Option<&str>) -> anyhow::Result<HashSet<(String, u64)>> {
    let Some(path) = path else {
        return Ok(HashSet::new());
    };
    if !Path::new(path).exists() {
        return Ok(HashSet::new());
    }
    let content = fs::read_to_string(path)?;
    let mut set = HashSet::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            if let (Some(repo), Some(pr)) = (
                val.get("repo").and_then(|v| v.as_str()),
                val.get("pr").and_then(|v| v.as_u64()),
            ) {
                set.insert((repo.to_string(), pr));
            }
        }
    }
    info!(
        skip_count = set.len(),
        path = path,
        "Loaded PRs to skip from file"
    );
    Ok(set)
}

fn parse_language_filter(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|lang| !lang.is_empty())
        .map(|lang| lang.to_lowercase())
        .collect::<Vec<_>>()
}

fn map_difficulty_label(score: Option<f64>, fallback: usize) -> String {
    let effective = score.unwrap_or(fallback as f64);
    if effective >= 0.8 {
        "Hard".to_string()
    } else if effective >= 0.5 {
        "Medium".to_string()
    } else {
        "Easy".to_string()
    }
}

/// JSON output structure for generation results.
#[derive(Debug, Clone, Serialize)]
pub struct GenerationOutput {
    pub status: String,
    pub model: String,
    pub tasks: Vec<GeneratedTaskOutput>,
    pub total_duration_ms: u64,
    pub output_directory: String,
}

/// JSON output structure for a generated task.
#[derive(Debug, Clone, Serialize)]
pub struct GeneratedTaskOutput {
    pub task_id: String,
    pub category: String,
    pub problem_statement: String,
    pub difficulty: String,
    pub tags: Vec<String>,
    pub verification_criteria: Vec<String>,
    pub saved_path: Option<String>,
}

/// SWE-backed task generation command.
async fn run_generate_command(args: GenerateArgs) -> anyhow::Result<()> {
    let api_key = args
        .api_key
        .clone()
        .or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
        .or_else(|| std::env::var("LITELLM_API_KEY").ok());

    let llm_client: Arc<dyn crate::llm::LlmProvider> = if let Some(key) = api_key {
        info!(model = %args.model, "Using OpenRouter with specified API key");
        Arc::new(OpenRouterProvider::with_model(key, args.model.clone()))
    } else {
        info!("Using LiteLLM client from environment");
        Arc::new(LiteLlmClient::from_env().map_err(|e| {
            anyhow::anyhow!(
                "Failed to initialize LLM client: {}. Please provide --api-key or set \
                 OPENROUTER_API_KEY/LITELLM_API_KEY env var.",
                e
            )
        })?)
    };

    let output_path = Path::new(&args.output);
    fs::create_dir_all(output_path)?;

    let config = SweOrchestratorConfig {
        output_dir: args.output.clone(),
        min_stars: args.min_stars,
        languages: parse_language_filter(&args.languages.unwrap_or_default()),
        max_tasks: args.count.max(1) as usize,
        once: args.count <= 1,
        validate_docker: args.validate_docker && !args.no_docker,
        ..SweOrchestratorConfig::default()
    };

    let orchestrator = SweOrchestrator::new(llm_client, config);
    let start = std::time::Instant::now();
    let result = orchestrator.mine().await?;

    let requested_category = args.category.clone().map(|c| c.to_lowercase());

    let generated_tasks: Vec<GeneratedTaskOutput> = result
        .tasks
        .into_iter()
        .filter(|task| {
            requested_category
                .as_ref()
                .map(|requested| {
                    task.meta
                        .values()
                        .any(|value: &String| value.to_lowercase().contains(requested))
                        || task.language.to_lowercase().contains(requested)
                        || task.repo.to_lowercase().contains(requested)
                        || task.prompt.to_lowercase().contains(requested)
                        || task.id.to_lowercase().contains(requested)
                })
                .unwrap_or(true)
        })
        .map(|task| {
            let saved_path = task
                .workspace_path
                .clone()
                .or_else(|| Some(format!("{}/{}", args.output, task.id)));

            GeneratedTaskOutput {
                task_id: task.id,
                category: requested_category
                    .clone()
                    .unwrap_or_else(|| "swe-mining".to_string()),
                problem_statement: task.prompt,
                difficulty: map_difficulty_label(
                    task.quality_score,
                    task.difficulty_score as usize,
                ),
                tags: vec![
                    "swe".to_string(),
                    format!("language:{}", task.language.to_lowercase()),
                    format!("repo:{}", task.repo),
                ],
                verification_criteria: task
                    .fail_to_pass
                    .into_iter()
                    .chain(task.pass_to_pass)
                    .collect(),
                saved_path,
            }
        })
        .collect();

    let output = GenerationOutput {
        status: if generated_tasks.is_empty() && result.attempted > 0 {
            "failed".to_string()
        } else {
            "success".to_string()
        },
        model: args.model.clone(),
        tasks: generated_tasks,
        total_duration_ms: start.elapsed().as_millis() as u64,
        output_directory: args.output,
    };

    let json_output = serde_json::to_string_pretty(&output)
        .map_err(|e| anyhow::anyhow!("Failed to serialize JSON output: {}", e))?;

    if args.json {
        println!("{}", json_output);
        return Ok(());
    }

    if output.status == "failed" {
        warn!("No tasks were generated with the requested constraints.");
        println!("{json_output}");
    } else {
        println!("{}", json_output);
    }

    Ok(())
}

// ============================================================================
// Generate Command Implementation
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
        let args = vec!["swe_forge", "generate"];
        let cli = Cli::try_parse_from(args).expect("should parse");

        match cli.command {
            Commands::Generate(args) => {
                assert_eq!(args.count, 1);
                assert_eq!(args.model, DEFAULT_MODEL);
                assert!(args.category.is_none());
                assert!(args.languages.is_none());
                assert_eq!(args.min_stars, 20);
                assert_eq!(args.output, DEFAULT_OUTPUT_DIR);
                assert!(!args.json);
            }
            _ => panic!("Expected Generate command"),
        }
    }

    #[test]
    fn test_generate_command_with_all_options() {
        let args = vec![
            "swe_forge",
            "generate",
            "-n",
            "5",
            "-m",
            "anthropic/claude-3-opus",
            "-c",
            "debugging",
            "--languages",
            "python,rust",
            "--min-stars",
            "50",
            "-o",
            "./my-output",
            "-j",
        ];
        let cli = Cli::try_parse_from(args).expect("should parse");

        match cli.command {
            Commands::Generate(args) => {
                assert_eq!(args.count, 5);
                assert_eq!(args.model, "anthropic/claude-3-opus");
                assert_eq!(args.category, Some("debugging".to_string()));
                assert_eq!(args.languages, Some("python,rust".to_string()));
                assert_eq!(args.min_stars, 50);
                assert_eq!(args.output, "./my-output");
                assert!(args.json);
            }
            _ => panic!("Expected Generate command"),
        }
    }

    #[test]
    fn test_generate_alias() {
        let args = vec!["swe_forge", "gen", "-n", "2"];
        let cli = Cli::try_parse_from(args).expect("should parse with alias");

        match cli.command {
            Commands::Generate(args) => {
                assert_eq!(args.count, 2);
            }
            _ => panic!("Expected Generate command"),
        }
    }

    #[test]
    fn test_swe_validate_parses() {
        let args = vec![
            "swe_forge",
            "swe",
            "validate",
            "--input",
            "./workspace-dir",
            "--validate-docker",
            "-m",
            "anthropic/claude-3-opus",
        ];
        let cli = Cli::try_parse_from(args).expect("should parse");

        match cli.command {
            Commands::Swe(boxed_args) => match boxed_args.command {
                SweSubcommand::Validate(s) => {
                    assert_eq!(s.input, "./workspace-dir");
                    assert_eq!(s.model, "anthropic/claude-3-opus");
                    assert!(s.validate_docker);
                }
                _ => panic!("Expected swe validate subcommand"),
            },
            _ => panic!("Expected swe command"),
        }
    }

    #[test]
    fn test_generation_output_serialization() {
        let output = GenerationOutput {
            status: "success".to_string(),
            model: "openai/gpt-5.2-codex:nitro".to_string(),
            tasks: vec![GeneratedTaskOutput {
                task_id: "swe_forge-task-001".to_string(),
                category: "debugging".to_string(),
                problem_statement: "Find the bug in the code".to_string(),
                difficulty: "Medium".to_string(),
                tags: vec!["memory".to_string(), "profiling".to_string()],
                verification_criteria: vec!["Bug identified".to_string()],
                saved_path: Some("./output/swe_forge-task-001".to_string()),
            }],
            total_duration_ms: 5000,
            output_directory: "./output".to_string(),
        };

        let json = serde_json::to_string_pretty(&output).expect("serialization should succeed");

        // Verify key fields are present in output
        assert!(json.contains("\"model\": \"openai/gpt-5.2-codex:nitro\""));
        assert!(json.contains("\"task_id\": \"swe_forge-task-001\""));
        assert!(json.contains("\"category\": \"debugging\""));
        assert!(json.contains("\"total_duration_ms\": 5000"));
    }

    #[test]
    fn test_evaluate_command_defaults() {
        let args = vec!["swe_forge", "evaluate", "--tasks-dir", "/tmp/tasks"];
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
            "swe_forge",
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
        let args = vec!["swe_forge", "eval", "-t", "/tmp/tasks"];
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
            model: "openai/gpt-5.2-codex:nitro".to_string(),
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
        assert!(json.contains("\"model\": \"openai/gpt-5.2-codex:nitro\""));
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
