//! Prompts for external data collection and processing agents.
//!
//! This module contains prompts for agents that collect, analyze, and process
//! external tasks for benchmark suitability. These prompts guide the evaluation,
//! analysis, and refinement of tasks collected from external sources.

/// Prompt for CollectorAgent to evaluate task priority.
///
/// Used to assess whether a collected task is suitable for inclusion
/// in the benchmark dataset based on complexity, relevance, and testability.
pub const COLLECTOR_EVALUATION_PROMPT: &str = r#"
You are evaluating a collected task for benchmark suitability.
Rate the following aspects on a scale of 0.0 to 1.0:

Task: {task_description}
Source: {source}
Test availability: {has_tests}

Evaluate:
1. COMPLEXITY: Is this task non-trivial? Does it require real problem-solving?
2. RELEVANCE: Is this useful for evaluating AI coding agents?
3. TESTABILITY: Can the solution be automatically verified?

Respond in JSON format:
{
  "complexity": 0.0-1.0,
  "relevance": 0.0-1.0,
  "testability": 0.0-1.0,
  "reasoning": "brief explanation"
}
"#;

/// Prompt for AnalyzerAgent to analyze tasks and extract metadata.
///
/// Used to deeply analyze a collected task, extract required skills,
/// estimate difficulty, and identify category classification.
pub const ANALYZER_PROMPT: &str = r#"
You are a task analyzer for AI coding benchmark development.
Analyze the following task and extract structured metadata.

## Task Information
Problem: {problem_description}
Repository: {repository}
Category Hint: {category_hint}

## Analysis Requirements
1. SKILLS: Identify specific technical skills required (be precise)
2. DIFFICULTY: Estimate difficulty level (easy/medium/hard) based on:
   - Number of distinct reasoning steps required
   - Breadth of knowledge domains involved
   - Complexity of integration between components
3. CATEGORY: Classify into one of: debugging, security, system-administration, software-engineering, file-operations, data-science, networking, containers, algorithm-design, reverse-engineering, performance-optimization, integration-tasks
4. SUBCATEGORY: Identify the most appropriate subcategory
5. DEPENDENCIES: List external dependencies (libraries, tools, services)
6. TIME_ESTIMATE: Estimate completion time for an expert (in minutes)

## Output Format
Respond in JSON format:
{
  "skills": ["skill1", "skill2", ...],
  "difficulty": "easy|medium|hard",
  "estimated_time_minutes": 15-120,
  "category": "category-name",
  "subcategory": "subcategory-name",
  "dependencies": ["dep1", "dep2", ...],
  "complexity_factors": ["factor1", "factor2", ...],
  "anti_patterns_detected": ["pattern1", ...],
  "reasoning": "brief explanation of analysis"
}
"#;

/// Prompt for ProblemCrafterAgent to reformulate problem statements.
///
/// Used to take raw task descriptions and craft clear, well-structured
/// problem statements suitable for benchmark evaluation.
pub const CRAFTER_PROMPT: &str = r#"
You are a benchmark problem crafter. Your job is to reformulate task descriptions
into clear, precise, and self-contained problem statements for AI agent evaluation.

## Original Task
{original_description}

## Context
{context}

## Constraints
- Maximum length: {max_length} characters
- Must be self-contained (no external links or references)
- Must have clear success criteria
- Must not reveal implementation approach

## Crafting Guidelines
1. CLARITY: Remove ambiguity, be specific about requirements
2. COMPLETENESS: Include all necessary context and constraints
3. OBJECTIVITY: Define measurable success criteria
4. NEUTRALITY: Don't bias toward any particular solution approach
5. REALISM: Frame as a realistic problem a developer might encounter

## Output Format
Respond in JSON format:
{
  "problem_statement": "The crafted problem statement...",
  "success_criteria": ["criterion1", "criterion2", ...],
  "hidden_constraints": ["constraint that agent must discover", ...],
  "estimated_steps": 5-15,
  "tags": ["tag1", "tag2", ...]
}
"#;

/// Prompt for TestDesignerAgent to design test cases.
///
/// Used to generate comprehensive test suites that verify
/// task completion without revealing the solution approach.
pub const TEST_DESIGNER_PROMPT: &str = r#"
You are a test designer for coding benchmarks. Design a comprehensive test suite
that verifies task completion without revealing implementation details.

## Problem Statement
{problem_statement}

## Existing Tests
{existing_tests}

## Test Design Principles
1. FAIL-TO-PASS: Tests that should fail initially and pass after correct implementation
2. PASS-TO-PASS: Tests that verify existing functionality isn't broken
3. EDGE_CASES: Tests for boundary conditions and error handling
4. NO_LEAKAGE: Tests must not reveal the solution approach

## Requirements
- Minimum 3 fail-to-pass tests
- Minimum 2 pass-to-pass tests (if applicable)
- All tests must be deterministic
- Tests should run in under 30 seconds total

## Output Format
Respond in JSON format:
{
  "fail_to_pass": [
    {
      "name": "test_name",
      "command": "command to run test",
      "expected_exit_code": 0,
      "description": "what this test verifies"
    },
    ...
  ],
  "pass_to_pass": [
    {
      "name": "test_name",
      "command": "command to run test",
      "expected_exit_code": 0,
      "description": "what this test verifies"
    },
    ...
  ],
  "edge_case_tests": [
    {
      "name": "test_name",
      "command": "command to run test",
      "expected_exit_code": 0,
      "description": "edge case being tested"
    },
    ...
  ],
  "test_script_content": "Full test script content as single line or escaped...",
  "timeout_seconds": 30
}
"#;

/// Prompt for EnvironmentBuilderAgent to determine task dependencies.
///
/// Used to analyze tasks and generate environment configuration
/// including Dockerfiles, dependency lists, and setup scripts.
pub const ENVIRONMENT_PROMPT: &str = r#"
You are an environment builder for coding benchmarks. Analyze the task
and generate the necessary environment configuration.

## Repository Information
Repository: {repository}
Primary Language: {language}
Dependency Hints: {deps_hint}

## Requirements
1. Identify all required system packages
2. Identify language-specific dependencies
3. Determine minimum required runtime versions
4. Generate a Dockerfile for isolated execution
5. Generate any setup/teardown scripts needed

## Environment Constraints
- Base image should be minimal but sufficient
- All dependencies must be pinned to specific versions
- Environment must be reproducible
- Maximum container size: 2GB
- Execution timeout: 5 minutes

## Output Format
Respond in JSON format:
{
  "base_image": "ubuntu:22.04",
  "system_packages": ["package1", "package2", ...],
  "language_runtime": {
    "name": "python",
    "version": "3.11"
  },
  "dependencies": [
    {"name": "package", "version": "1.0.0"},
    ...
  ],
  "dockerfile_content": "FROM ubuntu:22.04...",
  "setup_script": "Setup script content...",
  "teardown_script": "Teardown script content...",
  "environment_variables": {
    "VAR_NAME": "value"
  },
  "working_directory": "/app/workspace"
}
"#;

/// Prompt for SyntheticGeneratorAgent to generate synthetic tasks.
///
/// Used to generate novel benchmark tasks in specific categories,
/// particularly useful for DevOps and infrastructure scenarios.
pub const SYNTHETIC_GENERATION_PROMPT: &str = r#"
You are a synthetic benchmark task generator. Create a novel, challenging task
for the specified category that tests AI agent capabilities.

## Generation Parameters
Category: {category}
Difficulty Level: {difficulty}

## Task Requirements
1. NOVELTY: Task must not be a direct copy of common examples
2. REALISM: Task should reflect real-world scenarios
3. TESTABILITY: Task must be automatically verifiable
4. ISOLATION: Task must be executable in a containerized environment
5. TIME_BOUND: Task should be completable in 15-45 minutes

## Category-Specific Guidance
For DevOps/Infrastructure tasks, consider:
- CI/CD pipeline issues
- Kubernetes/Docker configuration problems
- Infrastructure as Code debugging
- Service mesh configuration
- Monitoring and alerting setup

For Security tasks, consider:
- Vulnerability remediation
- Access control configuration
- Secret management
- Audit log analysis
- Compliance verification

## Output Format
Respond in JSON format:
{
  "title": "Concise task title",
  "problem_statement": "Detailed problem description...",
  "category": "category-name",
  "subcategory": "subcategory-name",
  "difficulty": "easy|medium|hard",
  "required_skills": ["skill1", "skill2", ...],
  "success_criteria": ["criterion1", "criterion2", ...],
  "verification_approach": "How to verify completion",
  "environment": {
    "base_image": "image:tag",
    "required_tools": ["tool1", "tool2", ...],
    "setup_steps": ["step1", "step2", ...]
  },
  "hidden_solution": {
    "approach": "High-level solution approach",
    "key_commands": ["cmd1", "cmd2", ...],
    "estimated_time_minutes": 20
  },
  "anti_memorization_elements": ["element1", "element2", ...]
}
"#;

// ============================================================================
// Builder Functions
// ============================================================================

/// Builds the collector evaluation prompt with task details.
///
/// # Arguments
///
/// * `task` - Description of the task being evaluated
/// * `source` - Source where the task was collected from
/// * `has_tests` - Whether the task has associated tests
///
/// # Returns
///
/// A formatted prompt string ready for LLM submission.
///
/// # Examples
///
/// ```
/// use swe_forge::prompts::external_data::build_collector_prompt;
///
/// let prompt = build_collector_prompt(
///     "Fix the authentication bug in the login handler",
///     "github-issue",
///     true,
/// );
/// assert!(prompt.contains("Fix the authentication bug"));
/// ```
pub fn build_collector_prompt(task: &str, source: &str, has_tests: bool) -> String {
    COLLECTOR_EVALUATION_PROMPT
        .replace("{task_description}", task)
        .replace("{source}", source)
        .replace("{has_tests}", if has_tests { "yes" } else { "no" })
}

/// Builds the analyzer prompt with task and repository details.
///
/// # Arguments
///
/// * `problem` - The problem description to analyze
/// * `repo` - Repository URL or identifier
/// * `category_hint` - Optional category hint to guide classification
///
/// # Returns
///
/// A formatted prompt string ready for LLM submission.
///
/// # Examples
///
/// ```
/// use swe_forge::prompts::external_data::build_analyzer_prompt;
///
/// let prompt = build_analyzer_prompt(
///     "Optimize the database query performance",
///     "org/repo",
///     Some("performance-optimization"),
/// );
/// assert!(prompt.contains("Optimize the database query"));
/// ```
pub fn build_analyzer_prompt(problem: &str, repo: &str, category_hint: Option<&str>) -> String {
    ANALYZER_PROMPT
        .replace("{problem_description}", problem)
        .replace("{repository}", repo)
        .replace("{category_hint}", category_hint.unwrap_or("not specified"))
}

/// Builds the crafter prompt for reformulating problem statements.
///
/// # Arguments
///
/// * `original` - The original task description
/// * `context` - Additional context for the task
/// * `max_length` - Maximum character length for the crafted statement
///
/// # Returns
///
/// A formatted prompt string ready for LLM submission.
///
/// # Examples
///
/// ```
/// use swe_forge::prompts::external_data::build_crafter_prompt;
///
/// let prompt = build_crafter_prompt(
///     "There's a bug in the parser",
///     "The parser handles JSON input",
///     500,
/// );
/// assert!(prompt.contains("There's a bug in the parser"));
/// assert!(prompt.contains("500"));
/// ```
pub fn build_crafter_prompt(original: &str, context: &str, max_length: usize) -> String {
    CRAFTER_PROMPT
        .replace("{original_description}", original)
        .replace("{context}", context)
        .replace("{max_length}", &max_length.to_string())
}

/// Builds the test designer prompt with problem and existing tests.
///
/// # Arguments
///
/// * `problem` - The problem statement requiring tests
/// * `existing_tests` - List of existing test cases
///
/// # Returns
///
/// A formatted prompt string ready for LLM submission.
///
/// # Examples
///
/// ```
/// use swe_forge::prompts::external_data::build_test_designer_prompt;
///
/// let prompt = build_test_designer_prompt(
///     "Implement a rate limiter",
///     &["test_basic_limit".to_string()],
/// );
/// assert!(prompt.contains("Implement a rate limiter"));
/// assert!(prompt.contains("test_basic_limit"));
/// ```
pub fn build_test_designer_prompt(problem: &str, existing_tests: &[String]) -> String {
    let tests_formatted = if existing_tests.is_empty() {
        "None".to_string()
    } else {
        existing_tests
            .iter()
            .map(|t| format!("- {}", t))
            .collect::<Vec<_>>()
            .join("\n")
    };

    TEST_DESIGNER_PROMPT
        .replace("{problem_statement}", problem)
        .replace("{existing_tests}", &tests_formatted)
}

/// Builds the environment prompt with repository and language details.
///
/// # Arguments
///
/// * `repo` - Repository URL or identifier
/// * `language` - Primary programming language
/// * `deps_hint` - Hints about known dependencies
///
/// # Returns
///
/// A formatted prompt string ready for LLM submission.
///
/// # Examples
///
/// ```
/// use swe_forge::prompts::external_data::build_environment_prompt;
///
/// let prompt = build_environment_prompt(
///     "org/repo",
///     "python",
///     "requires numpy, pandas",
/// );
/// assert!(prompt.contains("org/repo"));
/// assert!(prompt.contains("python"));
/// ```
pub fn build_environment_prompt(repo: &str, language: &str, deps_hint: &str) -> String {
    ENVIRONMENT_PROMPT
        .replace("{repository}", repo)
        .replace("{language}", language)
        .replace("{deps_hint}", deps_hint)
}

/// Builds the synthetic generation prompt for creating novel tasks.
///
/// # Arguments
///
/// * `category` - Target category for the generated task
/// * `difficulty` - Target difficulty level (easy/medium/hard)
///
/// # Returns
///
/// A formatted prompt string ready for LLM submission.
///
/// # Examples
///
/// ```
/// use swe_forge::prompts::external_data::build_synthetic_prompt;
///
/// let prompt = build_synthetic_prompt("debugging", "medium");
/// assert!(prompt.contains("debugging"));
/// assert!(prompt.contains("medium"));
/// ```
pub fn build_synthetic_prompt(category: &str, difficulty: &str) -> String {
    SYNTHETIC_GENERATION_PROMPT
        .replace("{category}", category)
        .replace("{difficulty}", difficulty)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_collector_prompt_with_tests() {
        let prompt =
            build_collector_prompt("Fix authentication vulnerability", "github-issue", true);

        assert!(prompt.contains("Fix authentication vulnerability"));
        assert!(prompt.contains("github-issue"));
        assert!(prompt.contains("yes"));
        assert!(prompt.contains("COMPLEXITY"));
        assert!(prompt.contains("RELEVANCE"));
        assert!(prompt.contains("TESTABILITY"));
    }

    #[test]
    fn test_build_collector_prompt_without_tests() {
        let prompt =
            build_collector_prompt("Optimize query performance", "internal-tracker", false);

        assert!(prompt.contains("Optimize query performance"));
        assert!(prompt.contains("internal-tracker"));
        assert!(prompt.contains("no"));
    }

    #[test]
    fn test_build_analyzer_prompt_with_hint() {
        let prompt = build_analyzer_prompt(
            "Memory leak in background service",
            "company/backend-service",
            Some("debugging"),
        );

        assert!(prompt.contains("Memory leak in background service"));
        assert!(prompt.contains("company/backend-service"));
        assert!(prompt.contains("debugging"));
        assert!(prompt.contains("SKILLS"));
        assert!(prompt.contains("DIFFICULTY"));
    }

    #[test]
    fn test_build_analyzer_prompt_without_hint() {
        let prompt = build_analyzer_prompt("Implement caching layer", "org/api-gateway", None);

        assert!(prompt.contains("Implement caching layer"));
        assert!(prompt.contains("org/api-gateway"));
        assert!(prompt.contains("not specified"));
    }

    #[test]
    fn test_build_crafter_prompt() {
        let prompt = build_crafter_prompt(
            "There's a bug when parsing nested JSON arrays",
            "The parser is used for API responses",
            800,
        );

        assert!(prompt.contains("There's a bug when parsing nested JSON arrays"));
        assert!(prompt.contains("The parser is used for API responses"));
        assert!(prompt.contains("800"));
        assert!(prompt.contains("CLARITY"));
        assert!(prompt.contains("COMPLETENESS"));
    }

    #[test]
    fn test_build_test_designer_prompt_with_existing_tests() {
        let existing = vec![
            "test_basic_functionality".to_string(),
            "test_error_handling".to_string(),
        ];
        let prompt =
            build_test_designer_prompt("Implement retry logic with exponential backoff", &existing);

        assert!(prompt.contains("Implement retry logic"));
        assert!(prompt.contains("test_basic_functionality"));
        assert!(prompt.contains("test_error_handling"));
        assert!(prompt.contains("FAIL-TO-PASS"));
        assert!(prompt.contains("PASS-TO-PASS"));
    }

    #[test]
    fn test_build_test_designer_prompt_without_existing_tests() {
        let prompt = build_test_designer_prompt("Create a distributed lock implementation", &[]);

        assert!(prompt.contains("Create a distributed lock"));
        assert!(prompt.contains("None"));
    }

    #[test]
    fn test_build_environment_prompt() {
        let prompt =
            build_environment_prompt("myorg/ml-pipeline", "python", "requires tensorflow, numpy");

        assert!(prompt.contains("myorg/ml-pipeline"));
        assert!(prompt.contains("python"));
        assert!(prompt.contains("requires tensorflow, numpy"));
        assert!(prompt.contains("Dockerfile"));
        assert!(prompt.contains("system_packages"));
    }

    #[test]
    fn test_build_synthetic_prompt() {
        let prompt = build_synthetic_prompt("security", "hard");

        assert!(prompt.contains("security"));
        assert!(prompt.contains("hard"));
        assert!(prompt.contains("NOVELTY"));
        assert!(prompt.contains("TESTABILITY"));
        assert!(prompt.contains("hidden_solution"));
    }

    #[test]
    fn test_collector_evaluation_prompt_structure() {
        assert!(COLLECTOR_EVALUATION_PROMPT.contains("{task_description}"));
        assert!(COLLECTOR_EVALUATION_PROMPT.contains("{source}"));
        assert!(COLLECTOR_EVALUATION_PROMPT.contains("{has_tests}"));
        assert!(COLLECTOR_EVALUATION_PROMPT.contains("JSON format"));
    }

    #[test]
    fn test_analyzer_prompt_structure() {
        assert!(ANALYZER_PROMPT.contains("{problem_description}"));
        assert!(ANALYZER_PROMPT.contains("{repository}"));
        assert!(ANALYZER_PROMPT.contains("{category_hint}"));
        assert!(ANALYZER_PROMPT.contains("skills"));
        assert!(ANALYZER_PROMPT.contains("difficulty"));
    }

    #[test]
    fn test_crafter_prompt_structure() {
        assert!(CRAFTER_PROMPT.contains("{original_description}"));
        assert!(CRAFTER_PROMPT.contains("{context}"));
        assert!(CRAFTER_PROMPT.contains("{max_length}"));
        assert!(CRAFTER_PROMPT.contains("problem_statement"));
    }

    #[test]
    fn test_test_designer_prompt_structure() {
        assert!(TEST_DESIGNER_PROMPT.contains("{problem_statement}"));
        assert!(TEST_DESIGNER_PROMPT.contains("{existing_tests}"));
        assert!(TEST_DESIGNER_PROMPT.contains("fail_to_pass"));
        assert!(TEST_DESIGNER_PROMPT.contains("pass_to_pass"));
    }

    #[test]
    fn test_environment_prompt_structure() {
        assert!(ENVIRONMENT_PROMPT.contains("{repository}"));
        assert!(ENVIRONMENT_PROMPT.contains("{language}"));
        assert!(ENVIRONMENT_PROMPT.contains("{deps_hint}"));
        assert!(ENVIRONMENT_PROMPT.contains("dockerfile_content"));
    }

    #[test]
    fn test_synthetic_generation_prompt_structure() {
        assert!(SYNTHETIC_GENERATION_PROMPT.contains("{category}"));
        assert!(SYNTHETIC_GENERATION_PROMPT.contains("{difficulty}"));
        assert!(SYNTHETIC_GENERATION_PROMPT.contains("problem_statement"));
        assert!(SYNTHETIC_GENERATION_PROMPT.contains("hidden_solution"));
    }

    #[test]
    fn test_all_prompts_are_non_empty() {
        assert!(!COLLECTOR_EVALUATION_PROMPT.is_empty());
        assert!(!ANALYZER_PROMPT.is_empty());
        assert!(!CRAFTER_PROMPT.is_empty());
        assert!(!TEST_DESIGNER_PROMPT.is_empty());
        assert!(!ENVIRONMENT_PROMPT.is_empty());
        assert!(!SYNTHETIC_GENERATION_PROMPT.is_empty());
    }

    #[test]
    fn test_prompts_contain_json_output_instruction() {
        let prompts = [
            COLLECTOR_EVALUATION_PROMPT,
            ANALYZER_PROMPT,
            CRAFTER_PROMPT,
            TEST_DESIGNER_PROMPT,
            ENVIRONMENT_PROMPT,
            SYNTHETIC_GENERATION_PROMPT,
        ];

        for prompt in prompts {
            assert!(
                prompt.contains("JSON format") || prompt.contains("JSON"),
                "Prompt should instruct JSON output"
            );
        }
    }
}
