//! Execution prompt builder for privileged task execution.
//!
//! This module provides prompt construction for the execution stage of the
//! synthetic benchmark generation pipeline. The executor operates in a PRIVILEGED
//! context where it has full knowledge of the solution and generates both the
//! complete task environment and verification tests.

/// Prompts for the privileged execution stage.
///
/// Contains both the system prompt (establishing the privileged executor role)
/// and the user prompt (the specific task to implement with solution).
#[derive(Debug, Clone)]
pub struct ExecutionPrompt {
    /// System prompt establishing the privileged executor's role.
    pub system: String,
    /// User prompt with task details and execution instructions.
    pub user: String,
}

impl ExecutionPrompt {
    /// Creates a new execution prompt with the given system and user messages.
    pub fn new(system: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            system: system.into(),
            user: user.into(),
        }
    }
}

/// System prompt for privileged task execution.
const EXECUTION_SYSTEM_PROMPT: &str = r##"You are a PRIVILEGED EXECUTOR generating complete benchmark tasks with solutions.

## YOUR PRIVILEGED CONTEXT

You have FULL KNOWLEDGE of the solution. Your job is to:
1. Generate the COMPLETE TASK ENVIRONMENT (files, configuration, broken systems)
2. Generate the GOLD STANDARD SOLUTION (correct, complete implementation)
3. Generate VERIFICATION TESTS (prove the solution works)
4. Embed ANTI-MEMORIZATION mechanisms (canary tokens, unique identifiers)

## CRITICAL SEPARATION

You must maintain STRICT SEPARATION between:
- **TASK MATERIALS**: What the solver sees (problem description, broken files, environment)
- **SOLUTION MATERIALS**: What proves correctness (solution files, test scripts, verification)

The solver MUST NOT be able to derive the solution from task materials alone without reasoning.

## ANTI-MEMORIZATION REQUIREMENTS

Every task MUST include:
1. **Canary Token**: A unique identifier embedded in task materials that MUST appear in the solution
2. **Dynamic Elements**: Random identifiers, timestamps, or values that change per instance
3. **Environment Fingerprinting**: Unique configuration values that tie solution to environment

If the solver produces a solution without the canary token, it's INVALID (likely memorized).

## OUTPUT STRUCTURE

Generate materials in this structure:
```
task/
├── README.md           # Task description (what solver sees)
├── setup/              # Initial environment (broken state)
│   └── ...
├── solution/           # HIDDEN: Gold standard solution
│   └── ...
└── verification/       # HIDDEN: Test scripts
    └── verify.sh       # Returns 0 on success, non-zero on failure
```

## VERIFICATION REQUIREMENTS

The verification script MUST:
1. Check the canary token is present in the solution
2. Verify functional correctness (tests pass, service works, etc.)
3. Check for anti-patterns (hardcoded values, incomplete implementations)
4. Return clear pass/fail with diagnostic output

## QUALITY STANDARDS

- NO PLACEHOLDERS: Every file must be complete and functional
- NO AMBIGUITY: Success criteria must be deterministic
- NO SHORTCUTS: Solution must require the intended reasoning
- REALISTIC: Environment should mirror real-world scenarios

## OUTPUT FORMAT

Output a JSON object with this structure:

```json
{
  "task_id": "unique-task-identifier",
  "canary_token": "THE_CANARY_TOKEN_TO_EMBED",
  "task_materials": {
    "readme": "Task content in markdown...",
    "files": {
      "path/to/file": "file content"
    }
  },
  "solution_materials": {
    "files": {
      "path/to/solution": "solution content"
    },
    "explanation": "Why this solution is correct"
  },
  "verification": {
    "script": "#!/bin/bash\n# Verification script content",
    "expected_output": "What successful verification looks like",
    "failure_modes": ["List of ways the solution could fail"]
  }
}
```

Output ONLY the JSON object. No additional text."##;

/// Builds an execution prompt for generating complete benchmark tasks.
///
/// # Arguments
///
/// * `task_title` - The title of the task to implement
/// * `task_description` - The full description of the task
/// * `category` - The category the task belongs to
/// * `complexity_score` - The validated complexity score (0.0-1.0)
/// * `estimated_time_minutes` - Estimated completion time in minutes
/// * `canary_token` - The unique canary token to embed in the task
///
/// # Returns
///
/// An `ExecutionPrompt` ready for use with an LLM.
///
/// # Examples
///
/// ```
/// use swe_forge::prompts::build_execution_prompt;
///
/// let prompt = build_execution_prompt(
///     "Complex Debugging Task",
///     "Debug a multi-service system with cascading failures...",
///     "SystemDebugging",
///     0.85,
///     30,
///     "CANARY_TOKEN_abc123xyz",
/// );
/// assert!(prompt.system.contains("PRIVILEGED"));
/// assert!(prompt.user.contains("CANARY_TOKEN_abc123xyz"));
/// ```
pub fn build_execution_prompt(
    task_title: &str,
    task_description: &str,
    category: &str,
    complexity_score: f64,
    estimated_time_minutes: u32,
    canary_token: &str,
) -> ExecutionPrompt {
    let complexity_guidance = get_complexity_guidance(complexity_score);
    let time_guidance = get_time_guidance(estimated_time_minutes);

    let user = format!(
        r#"## TASK TO IMPLEMENT

**Category**: {}
**Title**: {}
**Complexity Score**: {:.2}
**Estimated Time**: {} minutes

**Description**:
{}

## CANARY TOKEN (REQUIRED)

You MUST embed this canary token in the task materials:
```
{}
```

The token should appear in:
1. Configuration files or environment variables
2. Any identifiers that the solution must reference
3. Comments or documentation that tie the solution to this specific instance

A solution that does NOT include this token is INVALID.

## COMPLEXITY GUIDANCE

{}

## TIME BUDGET

{}

## GENERATION REQUIREMENTS

1. **Task Materials**: Create a realistic broken/incomplete environment
   - Include all files the solver needs to see
   - Set up the scenario (logs, configuration, code with bugs)
   - Embed the canary token where the solver must interact with it

2. **Solution Materials**: Provide the complete, correct solution
   - Show exactly what changes need to be made
   - Explain WHY the solution works
   - Ensure the canary token is used correctly

3. **Verification**: Create a comprehensive test script
   - Verify the canary token is present
   - Test functional correctness
   - Check for common mistakes or anti-patterns

## ANTI-PATTERNS TO AVOID

- Don't make the solution obvious from file names
- Don't include hints that give away the answer
- Don't use standard examples that could be memorized
- Don't create trivial verification that misses edge cases

Generate the complete task as a JSON object now."#,
        category,
        task_title,
        complexity_score,
        estimated_time_minutes,
        task_description,
        canary_token,
        complexity_guidance,
        time_guidance,
    );

    ExecutionPrompt::new(EXECUTION_SYSTEM_PROMPT, user)
}

/// Returns guidance based on complexity score.
fn get_complexity_guidance(score: f64) -> &'static str {
    if score >= 0.9 {
        r#"EXPERT LEVEL (0.9+): This is an extremely challenging task.
- Create a deep, multi-layered problem with non-obvious connections
- The solution should require discovering hidden relationships
- Include multiple potential red herrings
- Verification should be thorough and catch subtle errors"#
    } else if score >= 0.7 {
        r#"ADVANCED LEVEL (0.7-0.9): This is a challenging task requiring significant expertise.
- Create a problem with multiple interacting components
- The solution should require systematic analysis
- Include some complexity in the environment setup
- Verification should test the core requirements thoroughly"#
    } else if score >= 0.5 {
        r#"INTERMEDIATE LEVEL (0.5-0.7): This is a moderate task requiring solid skills.
- Create a clear problem with defined scope
- The solution should require methodical work
- Environment should be realistic but not overwhelming
- Verification should cover main success criteria"#
    } else {
        r#"FOUNDATIONAL LEVEL (< 0.5): This is a straightforward task.
- Create a focused problem with clear objectives
- The solution should be achievable with standard approaches
- Environment should be simple and clear
- Verification should confirm basic functionality"#
    }
}

/// Returns guidance based on estimated time.
fn get_time_guidance(minutes: u32) -> &'static str {
    if minutes >= 45 {
        r#"EXTENDED TASK (45+ minutes): This is a substantial undertaking.
- Include comprehensive documentation in task materials
- Solution may involve multiple files or components
- Verification should be extensive
- Consider breaking the problem into discoverable phases"#
    } else if minutes >= 25 {
        r#"STANDARD TASK (25-45 minutes): This is a typical benchmark task.
- Provide clear but not excessive documentation
- Solution should be cohesive
- Verification should cover key scenarios
- Focus on depth over breadth"#
    } else {
        r#"QUICK TASK (< 25 minutes): This is a focused task.
- Keep documentation concise
- Solution should be targeted
- Verification should be efficient
- Emphasize clarity over complexity"#
    }
}

/// Builds an execution prompt with additional context about related tasks.
///
/// This variant includes information about similar tasks to ensure uniqueness.
///
/// # Arguments
///
/// * `task_title` - The title of the task
/// * `task_description` - The full description
/// * `category` - The category
/// * `complexity_score` - Complexity score (0.0-1.0)
/// * `estimated_time_minutes` - Estimated time
/// * `canary_token` - Canary token to embed
/// * `avoid_similarity_to` - Brief descriptions of similar tasks to avoid
///
/// # Returns
///
/// An `ExecutionPrompt` with uniqueness requirements.
pub fn build_execution_prompt_with_uniqueness(
    task_title: &str,
    task_description: &str,
    category: &str,
    complexity_score: f64,
    estimated_time_minutes: u32,
    canary_token: &str,
    avoid_similarity_to: &[String],
) -> ExecutionPrompt {
    let base_prompt = build_execution_prompt(
        task_title,
        task_description,
        category,
        complexity_score,
        estimated_time_minutes,
        canary_token,
    );

    if avoid_similarity_to.is_empty() {
        return base_prompt;
    }

    let uniqueness_section = format!(
        "\n\n## UNIQUENESS REQUIREMENT\n\nEnsure this task is distinctly different from:\n{}\n\nUse different scenarios, technologies, or problem structures.",
        avoid_similarity_to
            .iter()
            .map(|t| format!("- {}", t))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let extended_user = format!("{}{}", base_prompt.user, uniqueness_section);

    ExecutionPrompt::new(base_prompt.system, extended_user)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_execution_prompt_basic() {
        let prompt = build_execution_prompt(
            "Test Task",
            "Test description for the task",
            "SecurityAnalysis",
            0.75,
            30,
            "CANARY_abc123",
        );

        assert!(!prompt.system.is_empty());
        assert!(!prompt.user.is_empty());
        assert!(prompt.user.contains("Test Task"));
        assert!(prompt.user.contains("SecurityAnalysis"));
        assert!(prompt.user.contains("CANARY_abc123"));
        assert!(prompt.user.contains("0.75"));
        assert!(prompt.user.contains("30 minutes"));
    }

    #[test]
    fn test_system_prompt_contains_required_sections() {
        let prompt = build_execution_prompt("T", "D", "C", 0.5, 20, "TOKEN");

        assert!(prompt.system.contains("PRIVILEGED"));
        assert!(prompt.system.contains("ANTI-MEMORIZATION"));
        assert!(prompt.system.contains("VERIFICATION"));
        assert!(prompt.system.contains("canary"));
        assert!(prompt.system.contains("task_materials"));
        assert!(prompt.system.contains("solution_materials"));
    }

    #[test]
    fn test_complexity_guidance_expert() {
        let prompt = build_execution_prompt("T", "D", "C", 0.95, 30, "TOKEN");
        assert!(prompt.user.contains("EXPERT LEVEL"));
    }

    #[test]
    fn test_complexity_guidance_advanced() {
        let prompt = build_execution_prompt("T", "D", "C", 0.8, 30, "TOKEN");
        assert!(prompt.user.contains("ADVANCED LEVEL"));
    }

    #[test]
    fn test_complexity_guidance_intermediate() {
        let prompt = build_execution_prompt("T", "D", "C", 0.6, 30, "TOKEN");
        assert!(prompt.user.contains("INTERMEDIATE LEVEL"));
    }

    #[test]
    fn test_complexity_guidance_foundational() {
        let prompt = build_execution_prompt("T", "D", "C", 0.3, 30, "TOKEN");
        assert!(prompt.user.contains("FOUNDATIONAL LEVEL"));
    }

    #[test]
    fn test_time_guidance_extended() {
        let prompt = build_execution_prompt("T", "D", "C", 0.5, 60, "TOKEN");
        assert!(prompt.user.contains("EXTENDED TASK"));
    }

    #[test]
    fn test_time_guidance_standard() {
        let prompt = build_execution_prompt("T", "D", "C", 0.5, 30, "TOKEN");
        assert!(prompt.user.contains("STANDARD TASK"));
    }

    #[test]
    fn test_time_guidance_quick() {
        let prompt = build_execution_prompt("T", "D", "C", 0.5, 15, "TOKEN");
        assert!(prompt.user.contains("QUICK TASK"));
    }

    #[test]
    fn test_canary_token_emphasis() {
        let prompt = build_execution_prompt("T", "D", "C", 0.5, 20, "MY_UNIQUE_TOKEN_xyz");

        // Canary token should be prominently featured
        assert!(prompt.user.contains("MY_UNIQUE_TOKEN_xyz"));
        assert!(prompt.user.contains("CANARY TOKEN (REQUIRED)"));
        assert!(prompt.user.contains("INVALID"));
    }

    #[test]
    fn test_build_with_uniqueness_empty() {
        let prompt = build_execution_prompt_with_uniqueness(
            "Task",
            "Description",
            "Category",
            0.5,
            20,
            "TOKEN",
            &[],
        );

        assert!(!prompt.user.contains("UNIQUENESS REQUIREMENT"));
    }

    #[test]
    fn test_build_with_uniqueness_provided() {
        let similar = vec![
            "Similar task 1 about X".to_string(),
            "Another task about Y".to_string(),
        ];
        let prompt = build_execution_prompt_with_uniqueness(
            "Task",
            "Description",
            "Category",
            0.5,
            20,
            "TOKEN",
            &similar,
        );

        assert!(prompt.user.contains("UNIQUENESS REQUIREMENT"));
        assert!(prompt.user.contains("Similar task 1 about X"));
        assert!(prompt.user.contains("Another task about Y"));
    }

    #[test]
    fn test_output_format_documented() {
        let prompt = build_execution_prompt("T", "D", "C", 0.5, 20, "TOKEN");

        // Check that expected JSON structure is documented
        assert!(prompt.system.contains("task_id"));
        assert!(prompt.system.contains("canary_token"));
        assert!(prompt.system.contains("task_materials"));
        assert!(prompt.system.contains("solution_materials"));
        assert!(prompt.system.contains("verification"));
        assert!(prompt.system.contains("script"));
    }

    #[test]
    fn test_anti_patterns_section() {
        let prompt = build_execution_prompt("T", "D", "C", 0.5, 20, "TOKEN");

        assert!(prompt.user.contains("ANTI-PATTERNS TO AVOID"));
        assert!(prompt.user.contains("obvious"));
        assert!(prompt.user.contains("memorized"));
    }

    #[test]
    fn test_generation_requirements() {
        let prompt = build_execution_prompt("T", "D", "C", 0.5, 20, "TOKEN");

        assert!(prompt.user.contains("GENERATION REQUIREMENTS"));
        assert!(prompt.user.contains("Task Materials"));
        assert!(prompt.user.contains("Solution Materials"));
        assert!(prompt.user.contains("Verification"));
    }

    #[test]
    fn test_execution_prompt_new() {
        let prompt = ExecutionPrompt::new("system", "user");
        assert_eq!(prompt.system, "system");
        assert_eq!(prompt.user, "user");
    }

    #[test]
    fn test_system_prompt_directory_structure() {
        let prompt = build_execution_prompt("T", "D", "C", 0.5, 20, "TOKEN");

        // Check that expected directory structure is documented
        assert!(prompt.system.contains("README.md"));
        assert!(prompt.system.contains("setup/"));
        assert!(prompt.system.contains("solution/"));
        assert!(prompt.system.contains("verification/"));
        assert!(prompt.system.contains("verify.sh"));
    }
}
