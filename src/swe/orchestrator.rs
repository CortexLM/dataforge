//! Orchestrator glue for SWE mining end-to-end.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::llm::LlmProvider;
use crate::swe::{pipeline::SwePipelineConfig, SwePipelineRunResult, SweTask};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweRunResult {
    pub tasks: Vec<SweTask>,
    pub attempted: usize,
    pub passed: usize,
    pub skipped: usize,
    pub finished_at: String,
}

#[derive(Debug, Clone)]
pub struct SweOrchestratorConfig {
    pub output_dir: String,
    pub min_stars: u32,
    pub languages: Vec<String>,
    pub max_tasks: usize,
    pub once: bool,
    pub validate_docker: bool,
    pub skip_prs: HashSet<(String, u64)>,
    pub pr_file: Option<String>,
    pub difficulty_filter: Option<String>,
}

impl Default for SweOrchestratorConfig {
    fn default() -> Self {
        Self {
            output_dir: crate::swe::DEFAULT_SWE_OUTPUT_DIR.to_string(),
            min_stars: 20,
            languages: Vec::new(),
            max_tasks: 1,
            once: true,
            validate_docker: false,
            skip_prs: HashSet::new(),
            pr_file: None,
            difficulty_filter: None,
        }
    }
}

pub struct SweOrchestrator {
    llm: Arc<dyn LlmProvider>,
    config: SweOrchestratorConfig,
}

impl SweOrchestrator {
    pub fn new(llm: Arc<dyn LlmProvider>, config: SweOrchestratorConfig) -> Self {
        Self { llm, config }
    }

    pub async fn mine(&self) -> anyhow::Result<SweRunResult> {
        // When filtering for hard tasks, we need many more candidates
        let candidate_multiplier = if self.config.difficulty_filter.as_deref() == Some("hard") {
            200
        } else if self.config.difficulty_filter.is_some() {
            100
        } else {
            50
        };

        let pipeline_config = SwePipelineConfig {
            min_stars: self.config.min_stars,
            languages: self.config.languages.clone(),
            max_candidates: self.config.max_tasks.saturating_mul(candidate_multiplier).max(10),
            max_tasks: self.config.max_tasks,
            once: self.config.once,
            validate_docker: self.config.validate_docker,
            skip_prs: self.config.skip_prs.clone(),
            difficulty_filter: self.config.difficulty_filter.clone(),
        };

        let pipeline = crate::swe::pipeline::SwePipeline::new(&pipeline_config, self.llm.clone())?;
        let mut run: SwePipelineRunResult = pipeline.run(&pipeline_config, None).await?;

        let mut passed = 0usize;
        for task in run.tasks.iter_mut() {
            if task.quality_passed {
                export_task_to_disk(task, &self.config.output_dir)?;
                task.status = crate::swe::SweTaskStatus::Exported;
                task.workspace_path = Some(format!("{}/{}", self.config.output_dir, task.id));
                append_pr_to_file(&self.config.pr_file, &task.repo, task.id.as_str());
                passed += 1;
            } else {
                task.status = crate::swe::SweTaskStatus::Rejected;
            }
        }

        Ok(SweRunResult {
            attempted: run.scored,
            tasks: run.tasks.clone(),
            passed,
            skipped: run.tasks.len().saturating_sub(passed),
            finished_at: run.finished_at.to_rfc3339(),
        })
    }
}

fn append_pr_to_file(pr_file: &Option<String>, repo: &str, task_id: &str) {
    let Some(path) = pr_file else { return };
    let pr_number: u64 = task_id
        .rsplit('-')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let line = serde_json::json!({"repo": repo, "pr": pr_number});
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", line);
    }
}

fn export_task_to_disk(task: &SweTask, output_dir: &str) -> anyhow::Result<()> {
    let dir = Path::new(output_dir).join(&task.id);
    fs::create_dir_all(&dir)?;

    // prompt.md -- clean LLM-rewritten prompt (no test plan, no watermarks)
    let prompt = format!("# {}\n\n{}\n", task.id, task.prompt);
    fs::write(dir.join("prompt.md"), prompt)?;

    // original_pr.md -- raw PR body before rewriting
    if !task.original_pr_body.is_empty() {
        let original = format!("# {} (original PR)\n\n{}\n", task.id, task.original_pr_body);
        fs::write(dir.join("original_pr.md"), original)?;
    }

    // workspace.yaml -- full task metadata
    let workspace = serde_yaml::to_string(task)?;
    fs::write(dir.join("workspace.yaml"), workspace)?;

    // tests/ directory -- individual test files
    let tests_dir = dir.join("tests");
    fs::create_dir_all(&tests_dir)?;

    for (i, cmd) in task.fail_to_pass.iter().enumerate() {
        let filename = format!("fail_to_pass_{}.sh", i + 1);
        fs::write(tests_dir.join(&filename), format!("#!/bin/bash\n# This test must FAIL on base commit, PASS after fix\n{cmd}\n"))?;
    }

    for (i, cmd) in task.pass_to_pass.iter().enumerate() {
        let filename = format!("pass_to_pass_{}.sh", i + 1);
        fs::write(tests_dir.join(&filename), format!("#!/bin/bash\n# This test must PASS on base commit AND after fix\n{cmd}\n"))?;
    }

    // checks.txt -- all test commands (legacy flat format)
    if !task.fail_to_pass.is_empty() {
        let checks = task
            .fail_to_pass
            .iter()
            .chain(task.pass_to_pass.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(dir.join("checks.txt"), checks)?;
    }

    Ok(())
}
