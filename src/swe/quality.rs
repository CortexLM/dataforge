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
            min_quality_score: 0.3,
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

/// Quick triage result from pre-classification (title + body only, no diff).
#[derive(Debug, Clone)]
pub struct PreClassification {
    pub difficulty: String,
    pub dominated_out: bool,
}

const TRIAGE_SYSTEM_PROMPT: &str = r#"You triage GitHub PRs for a SWE benchmark. Given ONLY the PR title and description, estimate the difficulty for a top-tier LLM agent.

EASY: typo/doc fix, single rename, formatting
MEDIUM: bug fix, small feature, test additions
HARD: cross-cutting refactor, performance/security fix, architectural change

Be conservative: if unclear, say medium."#;

fn triage_tool() -> ToolDefinition {
    ToolDefinition::function(
        "triage",
        "Classify the difficulty of a GitHub PR",
        serde_json::json!({
            "type": "object",
            "properties": {
                "difficulty": { "type": "string", "enum": ["easy", "medium", "hard"] }
            },
            "required": ["difficulty"]
        }),
    )
}

#[derive(Debug, serde::Deserialize)]
struct TriageResponse {
    difficulty: String,
}

impl QualityScorer {
    pub fn new(llm: Arc<dyn LlmProvider>, config: QualityConfig) -> Self {
        Self { llm, config }
    }

    /// Fast pre-classification using only title + body (no diff, no tests).
    /// Returns whether the PR should be skipped based on the difficulty filter.
    pub async fn pre_classify(
        &self,
        repo: &str,
        pr: u64,
        title: &str,
        body: &str,
        difficulty_filter: &str,
    ) -> Result<PreClassification> {
        let body_truncated: &str = if body.len() > 500 {
            // Find a valid char boundary at or before 500
            let mut end = 500;
            while !body.is_char_boundary(end) && end > 0 {
                end -= 1;
            }
            &body[..end]
        } else {
            body
        };
        let prompt = format!(
            "PR: {repo}#{pr}\nTitle: {title}\nDescription: {body}",
            repo = repo, pr = pr, title = title, body = body_truncated,
        );

        let request = GenerationRequest::new(
            "",
            vec![
                Message::system(TRIAGE_SYSTEM_PROMPT),
                Message::user(prompt),
            ],
        )
        .with_temperature(0.1)
        .with_max_tokens(200)
        .with_tool(triage_tool());

        let response = self.llm.generate(request).await?;
        let content = response.first_content().unwrap_or("{}");

        let triage: TriageResponse = match serde_json::from_str(content.trim()) {
            Ok(v) => v,
            Err(_) => {
                let extracted = crate::utils::json_extraction::extract_json_from_response(content);
                serde_json::from_str(&extracted).unwrap_or(TriageResponse { difficulty: "medium".to_string() })
            }
        };

        let dominated_out = triage.difficulty != difficulty_filter;

        tracing::info!(
            repo = repo,
            pr = pr,
            triage_difficulty = %triage.difficulty,
            filter = difficulty_filter,
            skipped = dominated_out,
            "Pre-classification triage"
        );

        Ok(PreClassification {
            difficulty: triage.difficulty,
            dominated_out,
        })
    }

    pub async fn assess(&self, task: &SweTask) -> Result<QualityAssessment> {
        let patch_lines = task.patch.matches('\n').count();
        let files_changed = task.meta.get("files_changed")
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
            title = task.meta.get("pr_title").map(|s| s.as_str()).unwrap_or("(unknown)"),
            patch_lines = patch_lines,
            files = files_changed,
            has_tests = task.has_tests(),
            f2p = task.fail_to_pass.len(),
            p2p = task.pass_to_pass.len(),
            desc = {
                let p = &task.prompt;
                if p.len() > 2000 {
                    let mut end = 2000;
                    while !p.is_char_boundary(end) && end > 0 { end -= 1; }
                    &p[..end]
                } else { p }
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
        let passed = classification.quality_good && score >= self.config.min_quality_score;

        tracing::info!(
            task_id = %task.id,
            difficulty = %classification.difficulty,
            score = score,
            quality_good = classification.quality_good,
            "Difficulty classification done"
        );

        let mut reasons = vec![classification.reasoning];
        if !passed {
            reasons.push(format!("quality gate: score={:.2}, quality_good={}", score, classification.quality_good));
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
