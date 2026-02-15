//! Quality scoring for SWE mined tasks -- classifies difficulty and scores quality.

use anyhow::Result;
use std::sync::Arc;

use crate::llm::{GenerationRequest, LlmProvider, Message, ToolDefinition};
use crate::swe::SweTask;

#[derive(Debug, Clone)]
pub struct QualityConfig {
    pub min_quality_score: f64,
}

impl Default for QualityConfig {
    fn default() -> Self {
        Self {
            min_quality_score: 0.1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct QualityAssessment {
    pub score: f64,
    pub passed: bool,
    pub reasons: Vec<String>,
    pub difficulty_level: String,
    pub difficulty_score: f64,
}

pub struct QualityScorer {
    llm: Arc<dyn LlmProvider>,
    config: QualityConfig,
}

const CLASSIFY_SYSTEM_PROMPT: &str = r#"You are an expert evaluator of software engineering tasks for benchmarking top-tier LLM coding agents (like Claude, GPT-4, Gemini).

Given a GitHub Pull Request (title, description, diff stats), classify its difficulty for a state-of-the-art LLM agent to reproduce from scratch.

Difficulty criteria:

EASY (score 0.1-0.35):
- Typo fixes, documentation changes, simple renames
- Single-file changes with obvious intent
- Mechanical refactoring (imports, formatting)
- A top LLM solves this >90% of the time

MEDIUM (score 0.4-0.65):
- Bug fixes requiring understanding of 2-3 files
- Adding a new feature with clear spec in the PR description
- Test additions for existing functionality
- Requires domain knowledge but the PR description is sufficient
- A top LLM solves this 40-70% of the time

HARD (score 0.7-1.0):
- Cross-cutting changes touching many files/modules
- Performance optimizations requiring deep understanding
- Security fixes requiring subtle reasoning
- Architectural changes or complex refactoring
- Requires understanding implicit project conventions
- A top LLM solves this <30% of the time

Also assess quality: is this PR a good benchmark task? Good tasks have clear intent, testable outcomes, and non-trivial logic. Bad tasks are pure formatting, bot-generated, or have no clear acceptance criteria."#;

fn classify_tool() -> ToolDefinition {
    ToolDefinition::function(
        "classify_pr",
        "Classify difficulty and quality of a GitHub PR for SWE benchmarking",
        serde_json::json!({
            "type": "object",
            "properties": {
                "difficulty": {
                    "type": "string",
                    "enum": ["easy", "medium", "hard"],
                    "description": "Difficulty level for a top-tier LLM agent"
                },
                "score": {
                    "type": "number",
                    "description": "Numeric score 0.0-1.0 reflecting difficulty"
                },
                "quality_good": {
                    "type": "boolean",
                    "description": "Is this PR a good benchmark task?"
                },
                "reasoning": {
                    "type": "string",
                    "description": "Brief explanation of difficulty and quality assessment"
                }
            },
            "required": ["difficulty", "score", "quality_good", "reasoning"]
        }),
    )
}

#[derive(Debug, serde::Deserialize)]
struct ClassificationResponse {
    difficulty: String,
    score: f64,
    quality_good: bool,
    reasoning: String,
}

/// Full classification result using all available PR data.
#[derive(Debug, Clone)]
pub struct PreClassification {
    pub difficulty: String,
    pub dominated_out: bool,
}

/// Input data for full PR classification.
#[derive(Debug, Clone)]
pub struct ClassifyInput<'a> {
    pub repo: &'a str,
    pub pr: u64,
    pub title: &'a str,
    pub body: &'a str,
    pub language: &'a str,
    pub files_changed: usize,
    pub added_lines: usize,
    pub removed_lines: usize,
    pub changed_files: &'a [String],
}

const TRIAGE_SYSTEM_PROMPT: &str = r#"You classify GitHub PRs for a SWE benchmark. Given the PR title, description, language, changed files, and line counts, estimate the difficulty for a top-tier LLM agent to reproduce from scratch.

EASY (typo/doc fix, single rename, formatting, config tweaks):
- Touches 1-2 files, mostly non-code or trivial changes
- File names suggest docs, config, or CI (e.g. README, .yml, .json, .toml)
- Few lines added/removed

MEDIUM (bug fix, small feature, test additions):
- Touches 2-5 files, involves real logic changes
- Adds or modifies functions/methods in source files
- Requires understanding the code but the PR description is sufficient

HARD (cross-cutting refactor, performance/security fix, architectural change):
- Touches many files (5+) or deeply modifies core logic in fewer files
- Large line count changes (100+ added)
- File paths suggest multiple modules or packages
- Requires deep understanding of the project structure and conventions

Analyze the file paths carefully:
- Test-only PRs (all files in test/ or __tests__/) are usually EASY or MEDIUM
- PRs that only touch package.json/Cargo.toml/go.mod are usually EASY
- PRs touching both source and test files across multiple directories are often HARD"#;

fn triage_tool() -> ToolDefinition {
    ToolDefinition::function(
        "triage",
        "Classify the difficulty of a GitHub PR",
        serde_json::json!({
            "type": "object",
            "properties": {
                "difficulty": { "type": "string", "enum": ["easy", "medium", "hard"] },
                "reasoning": { "type": "string", "description": "Brief explanation" }
            },
            "required": ["difficulty", "reasoning"]
        }),
    )
}

#[derive(Debug, serde::Deserialize)]
struct TriageResponse {
    difficulty: String,
    #[serde(default)]
    reasoning: String,
}

impl QualityScorer {
    pub fn new(llm: Arc<dyn LlmProvider>, config: QualityConfig) -> Self {
        Self { llm, config }
    }

    /// Full classification using all available PR data (title, body, language,
    /// file paths, line counts). Replaces the old title-only pre-classification.
    pub async fn classify(
        &self,
        input: &ClassifyInput<'_>,
        difficulty_filter: &str,
    ) -> Result<PreClassification> {
        let body_truncated: &str = if input.body.len() > 1000 {
            let mut end = 1000;
            while !input.body.is_char_boundary(end) && end > 0 {
                end -= 1;
            }
            &input.body[..end]
        } else {
            input.body
        };

        let files_list = if input.changed_files.is_empty() {
            "(file list unavailable)".to_string()
        } else {
            input
                .changed_files
                .iter()
                .take(50)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n  ")
        };

        let prompt = format!(
            "PR: {repo}#{pr}\n\
             Title: {title}\n\
             Language: {lang}\n\
             Files changed: {files_count} (+{added}/-{removed} lines)\n\
             Changed files:\n  {files}\n\n\
             Description:\n{body}",
            repo = input.repo,
            pr = input.pr,
            title = input.title,
            lang = input.language,
            files_count = input.files_changed,
            added = input.added_lines,
            removed = input.removed_lines,
            files = files_list,
            body = body_truncated,
        );

        let request = GenerationRequest::new(
            "",
            vec![Message::system(TRIAGE_SYSTEM_PROMPT), Message::user(prompt)],
        )
        .with_temperature(0.1)
        .with_max_tokens(300)
        .with_tool(triage_tool());

        let response = self.llm.generate(request).await?;
        let content = response.first_content().unwrap_or("{}");

        let triage: TriageResponse = match serde_json::from_str(content.trim()) {
            Ok(v) => v,
            Err(_) => {
                let extracted = crate::utils::json_extraction::extract_json_from_response(content);
                serde_json::from_str(&extracted).unwrap_or(TriageResponse {
                    difficulty: "medium".to_string(),
                    reasoning: String::new(),
                })
            }
        };

        let dominated_out = triage.difficulty != difficulty_filter;

        tracing::info!(
            repo = input.repo,
            pr = input.pr,
            triage_difficulty = %triage.difficulty,
            filter = difficulty_filter,
            skipped = dominated_out,
            files_changed = input.files_changed,
            added = input.added_lines,
            removed = input.removed_lines,
            reasoning = %triage.reasoning,
            "Classification triage"
        );

        Ok(PreClassification {
            difficulty: triage.difficulty,
            dominated_out,
        })
    }

    pub async fn assess(&self, task: &SweTask) -> Result<QualityAssessment> {
        let patch_lines = task.patch.matches('\n').count();
        let files_changed = task
            .meta
            .get("files_changed")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);

        let user_prompt = format!(
            "Classify this PR:\n\n\
             Repository: {repo}\n\
             Language: {lang}\n\
             PR title: {title}\n\
             Diff size: {patch_lines} lines changed across ~{files} files\n\
             Has tests: {has_tests}\n\
             Fail-to-pass tests: {f2p}\n\
             Pass-to-pass tests: {p2p}\n\n\
             PR description:\n{desc}",
            repo = task.repo,
            lang = task.language,
            title = task
                .meta
                .get("pr_title")
                .map(|s| s.as_str())
                .unwrap_or("(unknown)"),
            patch_lines = patch_lines,
            files = files_changed,
            has_tests = task.has_tests(),
            f2p = task.fail_to_pass.len(),
            p2p = task.pass_to_pass.len(),
            desc = {
                let p = &task.prompt;
                if p.len() > 2000 {
                    let mut end = 2000;
                    while !p.is_char_boundary(end) && end > 0 {
                        end -= 1;
                    }
                    &p[..end]
                } else {
                    p
                }
            },
        );

        tracing::info!(task_id = %task.id, "Starting difficulty classification...");

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(CLASSIFY_SYSTEM_PROMPT),
                Message::user(user_prompt),
            ],
        )
        .with_temperature(0.3)
        .with_max_tokens(1000)
        .with_tool(classify_tool());

        let response = self.llm.generate(request).await?;

        let content = response
            .first_content()
            .ok_or_else(|| anyhow::anyhow!("Empty LLM response for difficulty classification"))?;

        let classification: ClassificationResponse = match serde_json::from_str(content.trim()) {
            Ok(v) => v,
            Err(_) => {
                // Fallback: try json extraction
                let extracted = crate::utils::json_extraction::extract_json_from_response(content);
                serde_json::from_str(&extracted)
                    .map_err(|e| anyhow::anyhow!("Failed to parse classification: {}", e))?
            }
        };

        let score = classification.score.clamp(0.0, 1.0);
        let passed = score >= self.config.min_quality_score;

        tracing::info!(
            task_id = %task.id,
            difficulty = %classification.difficulty,
            score = score,
            quality_good = classification.quality_good,
            "Difficulty classification done"
        );

        let mut reasons = vec![classification.reasoning];
        if !passed {
            reasons.push(format!(
                "quality gate: score={:.2}, quality_good={}",
                score, classification.quality_good
            ));
        }

        Ok(QualityAssessment {
            score,
            passed,
            reasons,
            difficulty_level: classification.difficulty,
            difficulty_score: score,
        })
    }
}
