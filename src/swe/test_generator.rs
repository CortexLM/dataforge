//! Test generation adapter for mined PR tasks.

use anyhow::Result;
use std::sync::Arc;

use crate::agents::{
    AnalyzerTaskCategory, CollectedTask, PipelineAnalyzedTask, TaskSource, TestDesignerAgent,
    TestDesignerConfig,
};
use crate::difficulty::DifficultyLevel;
use crate::llm::LlmProvider;
use crate::swe::SweTask;

pub struct TestGenerator {
    designer: TestDesignerAgent,
    config: TestDesignerConfig,
}

impl TestGenerator {
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self {
            designer: TestDesignerAgent::new(llm),
            config: TestDesignerConfig::default(),
        }
    }

    pub async fn ensure_tests(&self, task: &mut SweTask, language: &str) -> Result<()> {
        if task.has_tests() {
            return Ok(());
        }

        let source = CollectedTask::new(
            TaskSource::GitHubIssues,
            format!("{} regression", task.repo),
            task.prompt.clone(),
        )
        .with_source_url(format!("https://github.com/{}", task.repo));

        let analyzed = PipelineAnalyzedTask::new(
            source,
            AnalyzerTaskCategory::Debugging,
            "bugfix",
            DifficultyLevel::Medium,
            vec![language.to_string()],
            task.prompt.clone(),
            15,
            vec!["infrastructure".to_string(), "testability".to_string()],
        );

        tracing::info!(task_id = %task.id, "Starting LLM test generation...");
        let spec = self
            .designer
            .design_tests_with_config(&analyzed, None, &self.config)
            .await?;
        tracing::info!(task_id = %task.id, fail_to_pass = spec.fail_to_pass.len(), pass_to_pass = spec.pass_to_pass.len(), "Test generation done");
        apply_designer_test_spec(task, &spec);
        Ok(())
    }
}

fn apply_designer_test_spec(task: &mut SweTask, spec: &crate::agents::test_designer::TestSpec) {
    task.fail_to_pass = spec
        .fail_to_pass
        .iter()
        .map(|command| command.command.clone())
        .collect();

    task.pass_to_pass = spec
        .pass_to_pass
        .iter()
        .map(|command| command.command.clone())
        .collect();

    task.meta
        .insert("test_generation".to_string(), "designer".to_string());
}


