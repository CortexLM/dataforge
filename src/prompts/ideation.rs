//! Ideation prompt builder for task generation.
//!
//! This module provides the prompt construction logic for the ideation stage
//! of the synthetic benchmark generation pipeline. Ideation prompts are designed
//! to encourage creative, challenging task concepts that require extended reasoning.

use crate::prompts::categories::CategoryPrompt;

/// Prompts for the ideation stage of task generation.
///
/// Contains both the system prompt (defining the AI's role and constraints)
/// and the user prompt (the specific request for this generation).
#[derive(Debug, Clone)]
pub struct IdeationPrompt {
    /// System prompt establishing the AI's role and constraints.
    pub system: String,
    /// User prompt with the specific generation request.
    pub user: String,
}

impl IdeationPrompt {
    /// Creates a new ideation prompt with the given system and user messages.
    pub fn new(system: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            system: system.into(),
            user: user.into(),
        }
    }
}

/// Base system prompt for all ideation tasks.
const IDEATION_BASE_SYSTEM: &str = r#"You are an expert benchmark designer creating CHALLENGING tasks for evaluating AI coding capabilities.

## CORE PRINCIPLES

1. **DIFFICULTY FIRST**: Tasks must require EXTENDED REASONING (5+ distinct steps) and DOMAIN EXPERTISE
2. **NO MEMORIZATION**: Tasks must NOT have answers that can be looked up or memorized from training data
3. **MULTI-FACETED**: Solutions should require combining multiple skills and techniques
4. **REALISTIC**: Tasks should represent REAL challenges faced by senior engineers
5. **VERIFIABLE**: Success criteria must be objectively measurable

## WHAT MAKES A TASK HARD

A truly challenging task:
- Has NO single canonical solution (multiple valid approaches exist)
- Requires DISCOVERY of hidden constraints or structure
- Involves TRADE-OFFS between competing concerns
- Cannot be solved by pattern matching against known problems
- Takes an experienced professional 15-45 minutes

## ANTI-PATTERNS TO AVOID

NEVER create tasks that:
- Have solutions findable in documentation, Stack Overflow, or textbooks
- Can be solved by simple application of a known algorithm
- Have obvious "trick" solutions
- Test only recall of facts or syntax
- Are underspecified or have ambiguous success criteria

## OUTPUT FORMAT

You must output a JSON object with these fields:
{
  "title": "Concise, descriptive title (max 80 characters)",
  "description": "Detailed task description (200-500 words) including:\n  - Context and scenario\n  - Specific constraints and requirements\n  - Success criteria\n  - What files/systems are involved",
  "estimated_difficulty": "easy|medium|hard",
  "estimated_time_minutes": 15-45,
  "required_skills": ["skill1", "skill2", ...],
  "anti_patterns": ["approach_to_avoid1", "approach_to_avoid2", ...]
}

Output ONLY the JSON object. No additional text or explanation."#;

/// Builds an ideation prompt for generating task ideas.
///
/// # Arguments
///
/// * `category_prompt` - The category-specific prompt configuration
/// * `temperature_hint` - A hint about the temperature setting (0.0-2.0) for the AI to adjust creativity
/// * `diversity_seed` - Optional seed phrase to encourage diversity in generated tasks
///
/// # Returns
///
/// An `IdeationPrompt` ready for use with an LLM.
///
/// # Examples
///
/// ```
/// use dataforge::prompts::{get_category_prompt, build_ideation_prompt};
///
/// let category = get_category_prompt("AlgorithmDesign").unwrap();
/// let prompt = build_ideation_prompt(category, 1.0, None);
/// assert!(!prompt.system.is_empty());
/// assert!(!prompt.user.is_empty());
/// ```
pub fn build_ideation_prompt(
    category_prompt: &CategoryPrompt,
    temperature_hint: f64,
    diversity_seed: Option<&str>,
) -> IdeationPrompt {
    // Build the system prompt by combining base and category-specific content
    let system = format!(
        "{}\n\n## CATEGORY-SPECIFIC GUIDANCE: {}\n\n{}\n\n## DIFFICULTY GUIDELINES\n\n{}\n\n## REQUIRED SKILLS FOR THIS CATEGORY\n\n{}\n\n## APPROACHES TO AVOID (ANTI-PATTERNS)\n\n{}",
        IDEATION_BASE_SYSTEM,
        category_prompt.category,
        category_prompt.system_prompt,
        category_prompt.difficulty_guidelines,
        format_skills(category_prompt.required_skills),
        format_anti_patterns(category_prompt.anti_patterns),
    );

    // Build the user prompt
    let creativity_guidance = get_creativity_guidance(temperature_hint);
    let diversity_instruction = diversity_seed
        .map(|seed| {
            format!(
                "\n\nDIVERSITY REQUIREMENT: Incorporate this theme or constraint: {}",
                seed
            )
        })
        .unwrap_or_default();

    let user = format!(
        r#"Generate a creative, CHALLENGING benchmark task for the **{}** category.

{}{}

## EXAMPLE THEMES FOR INSPIRATION

{}

## CONSTRAINTS

- Task MUST require 5+ distinct reasoning/implementation steps
- Task MUST NOT be solvable by memorizing standard solutions
- Task MUST have objectively verifiable success criteria
- Task SHOULD take 15-45 minutes for an experienced professional

Generate your task as a JSON object now."#,
        category_prompt.category,
        creativity_guidance,
        diversity_instruction,
        format_themes(category_prompt.example_themes),
    );

    IdeationPrompt::new(system, user)
}

/// Returns creativity guidance based on the temperature hint.
fn get_creativity_guidance(temperature_hint: f64) -> &'static str {
    if temperature_hint >= 1.1 {
        "CREATIVITY MODE: Push boundaries! Generate NOVEL scenarios that haven't been seen before. Combine unexpected domains. Create unique constraints."
    } else if temperature_hint >= 0.9 {
        "BALANCED MODE: Create challenging tasks that are creative but grounded. Novel approaches to realistic problems."
    } else {
        "PRECISION MODE: Focus on technically rigorous tasks with clear specifications. Prioritize depth over novelty."
    }
}

/// Formats skills into a bullet list.
fn format_skills(skills: &[&str]) -> String {
    skills
        .iter()
        .map(|s| format!("- {}", s))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Formats anti-patterns into a bullet list.
fn format_anti_patterns(anti_patterns: &[&str]) -> String {
    anti_patterns
        .iter()
        .map(|p| format!("- ❌ {}", p))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Formats example themes into a numbered list.
fn format_themes(themes: &[&str]) -> String {
    themes
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{}. {}", i + 1, t))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompts::get_category_prompt;

    #[test]
    fn test_build_ideation_prompt_basic() {
        let category =
            get_category_prompt("AlgorithmDesign").expect("AlgorithmDesign should exist");
        let prompt = build_ideation_prompt(category, 1.0, None);

        assert!(!prompt.system.is_empty());
        assert!(!prompt.user.is_empty());
        assert!(prompt.system.contains("AlgorithmDesign"));
        assert!(prompt.user.contains("AlgorithmDesign"));
    }

    #[test]
    fn test_build_ideation_prompt_with_diversity_seed() {
        let category =
            get_category_prompt("SecurityAnalysis").expect("SecurityAnalysis should exist");
        let prompt = build_ideation_prompt(category, 1.0, Some("blockchain smart contracts"));

        assert!(prompt.user.contains("blockchain smart contracts"));
        assert!(prompt.user.contains("DIVERSITY REQUIREMENT"));
    }

    #[test]
    fn test_build_ideation_prompt_high_temperature() {
        let category =
            get_category_prompt("DataEngineering").expect("DataEngineering should exist");
        let prompt = build_ideation_prompt(category, 1.2, None);

        assert!(prompt.user.contains("CREATIVITY MODE"));
    }

    #[test]
    fn test_build_ideation_prompt_low_temperature() {
        let category = get_category_prompt("Infrastructure").expect("Infrastructure should exist");
        let prompt = build_ideation_prompt(category, 0.7, None);

        assert!(prompt.user.contains("PRECISION MODE"));
    }

    #[test]
    fn test_build_ideation_prompt_balanced_temperature() {
        let category =
            get_category_prompt("SystemDebugging").expect("SystemDebugging should exist");
        let prompt = build_ideation_prompt(category, 0.95, None);

        assert!(prompt.user.contains("BALANCED MODE"));
    }

    #[test]
    fn test_system_prompt_contains_required_sections() {
        let category =
            get_category_prompt("ReverseEngineering").expect("ReverseEngineering should exist");
        let prompt = build_ideation_prompt(category, 1.0, None);

        assert!(prompt.system.contains("CORE PRINCIPLES"));
        assert!(prompt.system.contains("ANTI-PATTERNS TO AVOID"));
        assert!(prompt.system.contains("OUTPUT FORMAT"));
        assert!(prompt.system.contains("CATEGORY-SPECIFIC GUIDANCE"));
        assert!(prompt.system.contains("DIFFICULTY GUIDELINES"));
        assert!(prompt.system.contains("REQUIRED SKILLS"));
    }

    #[test]
    fn test_user_prompt_contains_required_elements() {
        let category = get_category_prompt("PerformanceOptimization")
            .expect("PerformanceOptimization should exist");
        let prompt = build_ideation_prompt(category, 1.0, None);

        assert!(prompt.user.contains("EXAMPLE THEMES"));
        assert!(prompt.user.contains("CONSTRAINTS"));
        assert!(prompt.user.contains("JSON object"));
    }

    #[test]
    fn test_all_categories_produce_valid_prompts() {
        let categories = [
            "AlgorithmDesign",
            "SystemDebugging",
            "SecurityAnalysis",
            "Infrastructure",
            "DataEngineering",
            "ReverseEngineering",
            "PerformanceOptimization",
            "IntegrationTasks",
        ];

        for cat_name in categories {
            let category = get_category_prompt(cat_name)
                .unwrap_or_else(|| panic!("{} should exist", cat_name));
            let prompt = build_ideation_prompt(category, 1.0, None);

            assert!(
                prompt.system.len() >= 500,
                "System prompt for {} should be substantial",
                cat_name
            );
            assert!(
                prompt.user.len() >= 200,
                "User prompt for {} should be substantial",
                cat_name
            );
        }
    }

    #[test]
    fn test_format_skills() {
        let skills = &["skill1", "skill2"];
        let formatted = format_skills(skills);
        assert_eq!(formatted, "- skill1\n- skill2");
    }

    #[test]
    fn test_format_anti_patterns() {
        let patterns = &["pattern1", "pattern2"];
        let formatted = format_anti_patterns(patterns);
        assert_eq!(formatted, "- ❌ pattern1\n- ❌ pattern2");
    }

    #[test]
    fn test_format_themes() {
        let themes = &["theme1", "theme2", "theme3"];
        let formatted = format_themes(themes);
        assert_eq!(formatted, "1. theme1\n2. theme2\n3. theme3");
    }

    #[test]
    fn test_ideation_prompt_new() {
        let prompt = IdeationPrompt::new("system", "user");
        assert_eq!(prompt.system, "system");
        assert_eq!(prompt.user, "user");
    }
}
