//! LLM-based prompt rewriter.
//!
//! Rewrites the raw PR body into a clean task description that:
//! - Does NOT include test plans, testing sections, or validation steps
//! - Does NOT include LLM watermarks ("Generated with ...")
//! - IS precise enough for an agent to understand what code changes are needed
//! - Preserves technical details about the required implementation

use std::sync::Arc;

use anyhow::Result;
use tracing::warn;

use crate::llm::{GenerationRequest, LlmProvider, Message, ToolDefinition};

const REWRITE_SYSTEM_PROMPT: &str = r#"You rewrite GitHub Pull Request descriptions into task prompts for a coding benchmark.

The goal is to describe the PROBLEM or REQUIREMENT clearly, WITHOUT revealing the solution.

REMOVE:
- All test plans, testing sections, test results, CI/CD status, checkbox lists
- All watermarks ("Generated with [Claude Code]", "Co-authored-by", bot signatures)
- All issue references ("Closes #X", "Resolves #X") -- the agent has no access to issues
- Implementation details: specific function names created, exact code patterns used, file paths modified
- Architecture/design choices that were made (these ARE the solution)
- Any description of HOW things were implemented

KEEP:
- The high-level goal: what feature is needed, what bug needs fixing, what behavior should change
- User-facing requirements: what should the end result look like from the outside
- Constraints and acceptance criteria (without giving away the approach)
- Breaking changes from the USER perspective (not the implementation perspective)

REWRITE into:
- Imperative mood describing what NEEDS to be done, not what WAS done
- Focus on the desired outcome, not the implementation path
- Enough context that a skilled developer understands the scope, but must figure out the approach themselves

Example: instead of "Add a TokenValidator interface with user context injection and public path bypass for /health and /openapi.*", write "Add authentication middleware to the gateway. Unauthenticated requests to health and OpenAPI endpoints should be allowed through. All other endpoints require a valid bearer token."

Output ONLY the rewritten prompt text."#;

fn rewrite_tool() -> ToolDefinition {
    ToolDefinition::function(
        "rewrite_prompt",
        "Rewrite a PR description into a clean task prompt",
        serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "The rewritten task prompt (no test plan, no watermarks, precise technical description)"
                }
            },
            "required": ["prompt"]
        }),
    )
}

#[derive(Debug, serde::Deserialize)]
struct RewriteResponse {
    prompt: String,
}

pub struct PromptRewriter {
    llm: Arc<dyn LlmProvider>,
}

impl PromptRewriter {
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self { llm }
    }

    /// Rewrite a raw PR body into a clean task prompt.
    /// Returns the rewritten prompt, or the original on LLM failure.
    pub async fn rewrite(&self, repo: &str, pr_number: u64, title: &str, body: &str) -> Result<String> {
        let user_msg = format!(
            "Repository: {repo}\nPR #{pr_number}: {title}\n\n---\n\n{body}"
        );

        let request = GenerationRequest::new(
            "default",
            vec![
                Message::system(REWRITE_SYSTEM_PROMPT),
                Message::user(&user_msg),
            ],
        )
        .with_tool(rewrite_tool());

        let response = self.llm.generate(request).await?;
        let content = response
            .first_content()
            .unwrap_or_default()
            .to_string();

        match serde_json::from_str::<RewriteResponse>(&content) {
            Ok(parsed) => {
                if parsed.prompt.trim().is_empty() {
                    anyhow::bail!("LLM returned empty prompt");
                }
                Ok(parsed.prompt)
            }
            Err(e) => {
                warn!(repo, pr_number, "Failed to parse rewrite response: {e}, using raw content");
                if content.trim().is_empty() {
                    anyhow::bail!("LLM returned empty response");
                }
                Ok(content)
            }
        }
    }
}
