//! Validation prompt builder for task quality assessment.
//!
//! This module provides prompt construction for the validation stage of the
//! synthetic benchmark generation pipeline. Validation prompts are designed
//! to assess task complexity, detect memorization risks, and ensure genuine
//! reasoning requirements.

/// Prompts for the validation stage of task quality assessment.
///
/// Contains both the system prompt (defining the validator's role)
/// and the user prompt (the specific task to validate).
#[derive(Debug, Clone)]
pub struct ValidationPrompt {
    /// System prompt establishing the validator's role and criteria.
    pub system: String,
    /// User prompt with the task details to validate.
    pub user: String,
}

impl ValidationPrompt {
    /// Creates a new validation prompt with the given system and user messages.
    pub fn new(system: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            system: system.into(),
            user: user.into(),
        }
    }
}

/// System prompt for task validation.
const VALIDATION_SYSTEM_PROMPT: &str = r#"You are an expert benchmark validator assessing task quality for AI evaluation.

## YOUR ROLE

You evaluate proposed benchmark tasks to ensure they meet strict quality criteria:
1. **COMPLEXITY**: Tasks must require extended, multi-step reasoning
2. **NON-MEMORIZABLE**: Solutions must NOT exist in common training data
3. **GENUINE REASONING**: Tasks must require actual problem-solving, not recall
4. **FEASIBILITY**: Tasks must be completable by skilled humans
5. **VERIFIABILITY**: Success criteria must be objectively measurable

## EVALUATION CRITERIA

### Complexity Assessment (Score 0.0-1.0)
- 0.0-0.3: Simple, single-step tasks requiring no real reasoning
- 0.4-0.6: Moderate complexity, 2-4 steps, some reasoning required
- 0.7-0.8: Complex tasks requiring 5+ steps and domain expertise
- 0.9-1.0: Expert-level tasks requiring deep reasoning and novel approaches

### Memorization Risk (Score 0.0-1.0)
- 0.0-0.2: Unique scenario unlikely to exist in training data
- 0.3-0.5: Some similarity to common problems but with unique constraints
- 0.6-0.8: High similarity to documented problems or tutorials
- 0.9-1.0: Directly answerable from documentation or common knowledge

### Reasoning Authenticity (Score 0.0-1.0)
- 0.0-0.3: Can be solved by pattern matching or recall
- 0.4-0.6: Requires some reasoning but solution path is obvious
- 0.7-0.8: Requires genuine problem decomposition and analysis
- 0.9-1.0: Requires creative reasoning and discovery of hidden structure

### Time Estimation Guidelines
- **Easy (10-15 min)**: Straightforward tasks with clear approaches
- **Medium (20-30 min)**: Multi-step tasks requiring planning
- **Hard (30-60 min)**: Complex tasks requiring extensive reasoning

## VALIDATION DECISION

Based on your assessment:
- **ACCEPT**: complexity >= 0.7, memorization_risk <= 0.4, reasoning_authenticity >= 0.7
- **REJECT**: Any criterion significantly violated
- **REVISE**: Borderline scores suggesting task could be improved

## OUTPUT FORMAT

You must output a JSON object:
{
  "decision": "ACCEPT|REJECT|REVISE",
  "complexity_score": 0.0-1.0,
  "memorization_risk": 0.0-1.0,
  "reasoning_authenticity": 0.0-1.0,
  "estimated_time_minutes": integer,
  "reasoning": "Detailed explanation of your assessment",
  "concerns": ["specific concern 1", "specific concern 2"],
  "suggestions": ["improvement suggestion 1", "improvement suggestion 2"]
}

Output ONLY the JSON object. No additional text."#;

/// Builds a validation prompt for assessing task quality.
///
/// # Arguments
///
/// * `task_title` - The title of the task to validate
/// * `task_description` - The full description of the task
/// * `category` - The category the task belongs to
/// * `skills` - The skills required for this task
///
/// # Returns
///
/// A `ValidationPrompt` ready for use with an LLM.
///
/// # Examples
///
/// ```
/// use synth_bench::prompts::build_validation_prompt;
///
/// let prompt = build_validation_prompt(
///     "Complex Task Title",
///     "Detailed task description...",
///     "AlgorithmDesign",
///     &["rust".to_string(), "optimization".to_string()],
/// );
/// assert!(!prompt.system.is_empty());
/// assert!(prompt.user.contains("Complex Task Title"));
/// ```
pub fn build_validation_prompt(
    task_title: &str,
    task_description: &str,
    category: &str,
    skills: &[String],
) -> ValidationPrompt {
    let skills_formatted = if skills.is_empty() {
        "None specified".to_string()
    } else {
        skills.join(", ")
    };

    let user = format!(
        r#"## TASK TO VALIDATE

**Category**: {}
**Title**: {}

**Description**:
{}

**Required Skills**: {}

## VALIDATION QUESTIONS TO CONSIDER

1. **Complexity**: How many distinct steps are required? What level of expertise is needed?
2. **Memorization Risk**: Could this task be answered from common documentation, tutorials, or Stack Overflow?
3. **Reasoning Authenticity**: Does this require genuine problem-solving or just recall/pattern matching?
4. **Feasibility**: Can a skilled professional complete this in reasonable time?
5. **Verifiability**: Are the success criteria clear and objectively measurable?

## ADDITIONAL CHECKS

- Is the task specific enough to avoid ambiguity?
- Are there hidden assumptions that should be explicit?
- Could the task be "gamed" with simple tricks?
- Does the task test the claimed skills appropriately?

Provide your validation assessment as a JSON object now."#,
        category, task_title, task_description, skills_formatted
    );

    ValidationPrompt::new(VALIDATION_SYSTEM_PROMPT, user)
}

/// Builds a validation prompt with additional context about existing similar tasks.
///
/// This variant helps detect tasks that are too similar to existing ones in the benchmark.
///
/// # Arguments
///
/// * `task_title` - The title of the task to validate
/// * `task_description` - The full description of the task
/// * `category` - The category the task belongs to
/// * `skills` - The skills required for this task
/// * `similar_task_titles` - Titles of existing tasks in the same category
///
/// # Returns
///
/// A `ValidationPrompt` with similarity checking instructions.
pub fn build_validation_prompt_with_similarity(
    task_title: &str,
    task_description: &str,
    category: &str,
    skills: &[String],
    similar_task_titles: &[String],
) -> ValidationPrompt {
    let base_prompt = build_validation_prompt(task_title, task_description, category, skills);

    let similarity_section = if similar_task_titles.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n## SIMILARITY CHECK\n\nExisting tasks in this category:\n{}\n\nConsider whether the new task is sufficiently different from these existing tasks.",
            similar_task_titles
                .iter()
                .map(|t| format!("- {}", t))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    let extended_user = format!("{}{}", base_prompt.user, similarity_section);

    ValidationPrompt::new(base_prompt.system, extended_user)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_validation_prompt_basic() {
        let prompt = build_validation_prompt(
            "Test Task Title",
            "This is a test task description with enough detail.",
            "AlgorithmDesign",
            &["rust".to_string(), "algorithms".to_string()],
        );

        assert!(!prompt.system.is_empty());
        assert!(!prompt.user.is_empty());
        assert!(prompt.user.contains("Test Task Title"));
        assert!(prompt.user.contains("AlgorithmDesign"));
        assert!(prompt.user.contains("rust, algorithms"));
    }

    #[test]
    fn test_build_validation_prompt_empty_skills() {
        let prompt = build_validation_prompt(
            "Task Without Skills",
            "Description here",
            "SecurityAnalysis",
            &[],
        );

        assert!(prompt.user.contains("None specified"));
    }

    #[test]
    fn test_validation_system_prompt_content() {
        let prompt = build_validation_prompt("T", "D", "C", &[]);

        assert!(prompt.system.contains("Complexity"));
        assert!(prompt.system.contains("Memorization Risk"));
        assert!(prompt.system.contains("Reasoning"));
        assert!(prompt.system.contains("ACCEPT"));
        assert!(prompt.system.contains("REJECT"));
        assert!(prompt.system.contains("REVISE"));
        assert!(prompt.system.contains("JSON"));
    }

    #[test]
    fn test_build_validation_prompt_with_similarity_no_existing() {
        let prompt = build_validation_prompt_with_similarity(
            "New Task",
            "Task description",
            "Infrastructure",
            &["kubernetes".to_string()],
            &[],
        );

        assert!(!prompt.user.contains("SIMILARITY CHECK"));
    }

    #[test]
    fn test_build_validation_prompt_with_similarity_existing() {
        let similar_tasks = vec!["Existing Task 1".to_string(), "Existing Task 2".to_string()];
        let prompt = build_validation_prompt_with_similarity(
            "New Task",
            "Task description",
            "Infrastructure",
            &["kubernetes".to_string()],
            &similar_tasks,
        );

        assert!(prompt.user.contains("SIMILARITY CHECK"));
        assert!(prompt.user.contains("Existing Task 1"));
        assert!(prompt.user.contains("Existing Task 2"));
    }

    #[test]
    fn test_user_prompt_contains_validation_questions() {
        let prompt = build_validation_prompt(
            "Test",
            "Description",
            "DataEngineering",
            &["sql".to_string()],
        );

        assert!(prompt.user.contains("VALIDATION QUESTIONS"));
        assert!(prompt.user.contains("Complexity"));
        assert!(prompt.user.contains("Memorization Risk"));
        assert!(prompt.user.contains("Reasoning Authenticity"));
        assert!(prompt.user.contains("Feasibility"));
        assert!(prompt.user.contains("Verifiability"));
    }

    #[test]
    fn test_user_prompt_contains_additional_checks() {
        let prompt = build_validation_prompt("Test", "Description", "PerformanceOptimization", &[]);

        assert!(prompt.user.contains("ADDITIONAL CHECKS"));
        assert!(prompt.user.contains("ambiguity"));
        assert!(prompt.user.contains("gamed"));
    }

    #[test]
    fn test_system_prompt_scoring_criteria() {
        let prompt = build_validation_prompt("T", "D", "C", &[]);

        // Check that scoring ranges are documented
        assert!(prompt.system.contains("0.0-0.3"));
        assert!(prompt.system.contains("0.4-0.6"));
        assert!(prompt.system.contains("0.7-0.8"));
        assert!(prompt.system.contains("0.9-1.0"));
    }

    #[test]
    fn test_system_prompt_time_estimation() {
        let prompt = build_validation_prompt("T", "D", "C", &[]);

        assert!(prompt.system.contains("10-15 min"));
        assert!(prompt.system.contains("20-30 min"));
        assert!(prompt.system.contains("30-60 min"));
    }

    #[test]
    fn test_validation_prompt_new() {
        let prompt = ValidationPrompt::new("sys", "usr");
        assert_eq!(prompt.system, "sys");
        assert_eq!(prompt.user, "usr");
    }

    #[test]
    fn test_output_format_requirements() {
        let prompt = build_validation_prompt("T", "D", "C", &[]);

        // Ensure the expected JSON fields are documented
        assert!(prompt.system.contains("decision"));
        assert!(prompt.system.contains("complexity_score"));
        assert!(prompt.system.contains("memorization_risk"));
        assert!(prompt.system.contains("reasoning_authenticity"));
        assert!(prompt.system.contains("estimated_time_minutes"));
        assert!(prompt.system.contains("reasoning"));
        assert!(prompt.system.contains("concerns"));
        assert!(prompt.system.contains("suggestions"));
    }
}
