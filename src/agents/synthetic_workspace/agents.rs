//! Specialized agents for synthetic workspace generation.
//!
//! This module provides individual agents that participate in the
//! workspace generation pipeline, each with a specific role.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use tracing::info;

use crate::error::LlmError;
use crate::llm::{GenerationRequest, LlmProvider, Message};

use super::config::{DifficultyLevel, LanguageTarget, ProjectCategory};
use super::types::ProjectSpec;

/// An agent role in the generation pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    /// Designs project architecture.
    Architect,
    /// Generates production-quality code.
    CodeGenerator,
    /// Plans vulnerability injection.
    VulnerabilityStrategist,
    /// Implements vulnerabilities.
    Injector,
    /// Reviews code quality.
    QualityAssurance,
    /// Calibrates difficulty.
    DifficultyCalibrator,
    /// Removes hints and markers.
    Cleaner,
    /// Reviews feasibility.
    FeasibilityReviewer,
}

impl AgentRole {
    /// Returns the display name for this role.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Architect => "Architect",
            Self::CodeGenerator => "Code Generator",
            Self::VulnerabilityStrategist => "Vulnerability Strategist",
            Self::Injector => "Injector",
            Self::QualityAssurance => "Quality Assurance",
            Self::DifficultyCalibrator => "Difficulty Calibrator",
            Self::Cleaner => "Cleaner",
            Self::FeasibilityReviewer => "Feasibility Reviewer",
        }
    }

    /// Returns the system prompt for this agent role.
    pub fn system_prompt(&self) -> &'static str {
        match self {
            Self::Architect => {
                r#"You are an expert software architect designing realistic code projects.

Your designs must:
1. Reflect real-world production patterns
2. Use appropriate frameworks and libraries
3. Have proper separation of concerns
4. Support natural vulnerability injection points
5. Be neither too simple nor overly complex

Provide specific, actionable architectural decisions with clear rationale."#
            }

            Self::CodeGenerator => {
                r#"You are an expert developer writing production-quality code.

Your code must:
1. Look like real production code - no placeholders
2. Follow language-specific best practices
3. Include proper error handling
4. Have realistic naming and structure
5. NEVER include TODO, FIXME, or hint comments
6. Be complete and functional

Output only code. No explanations or markdown."#
            }

            Self::VulnerabilityStrategist => {
                r#"You are a security expert planning vulnerability injection for benchmarks.

Your strategies must:
1. Select realistic, discoverable vulnerabilities
2. Choose appropriate placement in the codebase
3. Plan subtle implementation that looks natural
4. Consider difficulty and solvability
5. Avoid obvious patterns or telltale signs

Provide detailed injection plans with specific file locations and techniques."#
            }

            Self::Injector => {
                r#"You are a security expert implementing vulnerabilities in code.

Your injections must:
1. Be SUBTLE - no obvious markers or comments
2. Look like natural developer mistakes
3. Be exploitable but not trivially obvious
4. Maintain code functionality
5. NEVER add comments about the vulnerability

Output only the modified code."#
            }

            Self::QualityAssurance => {
                r#"You are a code quality expert reviewing generated code.

You must verify:
1. Code looks like real production code
2. No placeholder implementations remain
3. No hints about vulnerabilities exist
4. Proper coding standards are followed
5. Code compiles and runs correctly

Report issues and suggest specific fixes."#
            }

            Self::DifficultyCalibrator => {
                r#"You are an expert in task difficulty assessment.

You evaluate:
1. Complexity of vulnerability discovery
2. Required security expertise level
3. Code analysis effort needed
4. Time to complete the task
5. Overall solvability

Score on multiple dimensions and suggest calibrations."#
            }

            Self::Cleaner => {
                r#"You are a code sanitization expert removing hints from generated code.

You must remove:
1. TODO, FIXME, XXX comments
2. Comments mentioning security or vulnerabilities
3. Debug statements revealing issues
4. Obvious marker patterns
5. Suspicious variable names

Preserve all functional code while removing hints."#
            }

            Self::FeasibilityReviewer => {
                r#"You are an expert assessing task feasibility.

You verify:
1. The task is solvable with available information
2. Vulnerabilities can be discovered through analysis
3. The task isn't impossibly hard
4. Required context is provided
5. Time expectations are realistic

Identify blockers and suggest adjustments."#
            }
        }
    }
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Base trait for pipeline agents.
pub trait PipelineAgent: Send + Sync {
    /// Returns the agent's role.
    fn role(&self) -> AgentRole;

    /// Returns the agent's name.
    fn name(&self) -> String {
        self.role().display_name().to_string()
    }
}

/// Architect agent for designing project structure.
pub struct ArchitectAgent {
    llm: Arc<dyn LlmProvider>,
    model: String,
}

impl ArchitectAgent {
    /// Creates a new architect agent.
    pub fn new(llm: Arc<dyn LlmProvider>, model: impl Into<String>) -> Self {
        Self {
            llm,
            model: model.into(),
        }
    }

    /// Designs a project architecture.
    pub async fn design_project(
        &self,
        language: LanguageTarget,
        category: ProjectCategory,
        difficulty: DifficultyLevel,
    ) -> Result<ProjectDesign, LlmError> {
        let system = AgentRole::Architect.system_prompt();

        let user = format!(
            r#"Design a {} project architecture for a {} application with {} difficulty.

Provide your design as JSON:
{{
    "name": "project-name",
    "description": "Brief description",
    "framework": "Framework to use or null",
    "directories": ["src", "tests", "config"],
    "key_files": [
        {{"path": "src/main.py", "purpose": "Entry point"}}
    ],
    "design_rationale": "Why this design",
    "vulnerability_opportunities": [
        {{"type": "sql_injection", "location": "src/db.py", "rationale": "Database layer"}}
    ]
}}"#,
            language.display_name(),
            category,
            difficulty
        );

        let request = GenerationRequest::new(
            &self.model,
            vec![Message::system(system), Message::user(&user)],
        )
        .with_temperature(0.7)
        .with_max_tokens(2000);

        let response = self.llm.generate(request).await?;
        let content = response.first_content().unwrap_or_default();

        // Parse the response
        self.parse_design(content)
    }

    fn parse_design(&self, content: &str) -> Result<ProjectDesign, LlmError> {
        // Try to extract JSON
        let json_str = self.extract_json(content);

        serde_json::from_str(&json_str)
            .map_err(|e| LlmError::ParseError(format!("Failed to parse project design: {}", e)))
    }

    fn extract_json(&self, content: &str) -> String {
        let trimmed = content.trim();

        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            return trimmed.to_string();
        }

        if let Some(start) = trimmed.find("```json") {
            if let Some(end) = trimmed[start + 7..].find("```") {
                return trimmed[start + 7..start + 7 + end].trim().to_string();
            }
        }

        if let Some(start) = trimmed.find('{') {
            if let Some(end) = trimmed.rfind('}') {
                return trimmed[start..=end].to_string();
            }
        }

        trimmed.to_string()
    }
}

impl PipelineAgent for ArchitectAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Architect
    }
}

/// Project design output from architect agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDesign {
    pub name: String,
    pub description: String,
    pub framework: Option<String>,
    pub directories: Vec<String>,
    pub key_files: Vec<KeyFile>,
    pub design_rationale: String,
    pub vulnerability_opportunities: Vec<VulnerabilityOpportunity>,
}

/// A key file in the project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyFile {
    pub path: String,
    pub purpose: String,
}

/// A vulnerability opportunity identified by the architect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityOpportunity {
    #[serde(rename = "type")]
    pub vulnerability_type: String,
    pub location: String,
    pub rationale: String,
}

/// Vulnerability Strategist agent for planning injections.
pub struct VulnerabilityStrategistAgent {
    llm: Arc<dyn LlmProvider>,
    model: String,
}

impl VulnerabilityStrategistAgent {
    /// Creates a new vulnerability strategist agent.
    pub fn new(llm: Arc<dyn LlmProvider>, model: impl Into<String>) -> Self {
        Self {
            llm,
            model: model.into(),
        }
    }

    /// Plans vulnerability injections.
    pub async fn plan_vulnerabilities(
        &self,
        spec: &ProjectSpec,
        count: usize,
    ) -> Result<Vec<VulnerabilityPlan>, LlmError> {
        let system = AgentRole::VulnerabilityStrategist.system_prompt();

        let common_vulns = spec.category.common_vulnerabilities();

        let user = format!(
            r#"Plan {count} vulnerability injections for this project:

Project: {name}
Language: {language}
Category: {category}
Difficulty: {difficulty}

Common vulnerabilities for this category: {vulns}

Provide your plan as JSON array:
[
    {{
        "vulnerability_type": "sql_injection",
        "target_file": "app/db.py",
        "target_function": "get_user",
        "injection_technique": "String interpolation in SQL query",
        "subtlety_level": 7,
        "detection_difficulty": "medium",
        "cwe_id": "CWE-89",
        "remediation": "Use parameterized queries"
    }}
]"#,
            count = count,
            name = spec.name,
            language = spec.language.display_name(),
            category = spec.category,
            difficulty = spec.difficulty,
            vulns = common_vulns.join(", ")
        );

        let request = GenerationRequest::new(
            &self.model,
            vec![Message::system(system), Message::user(&user)],
        )
        .with_temperature(0.6)
        .with_max_tokens(3000);

        let response = self.llm.generate(request).await?;
        let content = response.first_content().unwrap_or_default();

        self.parse_plans(content)
    }

    fn parse_plans(&self, content: &str) -> Result<Vec<VulnerabilityPlan>, LlmError> {
        let json_str = self.extract_json_array(content);

        serde_json::from_str(&json_str).map_err(|e| {
            LlmError::ParseError(format!("Failed to parse vulnerability plans: {}", e))
        })
    }

    fn extract_json_array(&self, content: &str) -> String {
        let trimmed = content.trim();

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            return trimmed.to_string();
        }

        if let Some(start) = trimmed.find("```json") {
            if let Some(end) = trimmed[start + 7..].find("```") {
                return trimmed[start + 7..start + 7 + end].trim().to_string();
            }
        }

        if let Some(start) = trimmed.find('[') {
            if let Some(end) = trimmed.rfind(']') {
                return trimmed[start..=end].to_string();
            }
        }

        trimmed.to_string()
    }
}

impl PipelineAgent for VulnerabilityStrategistAgent {
    fn role(&self) -> AgentRole {
        AgentRole::VulnerabilityStrategist
    }
}

/// A planned vulnerability injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityPlan {
    pub vulnerability_type: String,
    pub target_file: String,
    pub target_function: Option<String>,
    pub injection_technique: String,
    pub subtlety_level: u8,
    pub detection_difficulty: String,
    pub cwe_id: Option<String>,
    pub remediation: String,
}

/// Quality Assurance agent for reviewing code.
pub struct QualityAssuranceAgent {
    llm: Arc<dyn LlmProvider>,
    model: String,
}

impl QualityAssuranceAgent {
    /// Creates a new QA agent.
    pub fn new(llm: Arc<dyn LlmProvider>, model: impl Into<String>) -> Self {
        Self {
            llm,
            model: model.into(),
        }
    }

    /// Reviews code quality.
    pub async fn review_code(
        &self,
        file_path: &str,
        content: &str,
        language: LanguageTarget,
    ) -> Result<QualityReview, LlmError> {
        let system = AgentRole::QualityAssurance.system_prompt();

        let user = format!(
            r#"Review this {} code for quality:

File: {}

```
{}
```

Provide your review as JSON:
{{
    "overall_score": 0.0-1.0,
    "looks_realistic": true/false,
    "has_placeholders": true/false,
    "has_vulnerability_hints": true/false,
    "coding_standards_followed": true/false,
    "issues": [
        {{"severity": "high/medium/low", "description": "Issue description", "line": 42}}
    ],
    "recommendations": ["Fix recommendation"]
}}"#,
            language.display_name(),
            file_path,
            content
        );

        let request = GenerationRequest::new(
            &self.model,
            vec![Message::system(system), Message::user(&user)],
        )
        .with_temperature(0.3)
        .with_max_tokens(2000);

        let response = self.llm.generate(request).await?;
        let content = response.first_content().unwrap_or_default();

        self.parse_review(content)
    }

    fn parse_review(&self, content: &str) -> Result<QualityReview, LlmError> {
        let json_str = self.extract_json(content);

        serde_json::from_str(&json_str)
            .map_err(|e| LlmError::ParseError(format!("Failed to parse quality review: {}", e)))
    }

    fn extract_json(&self, content: &str) -> String {
        let trimmed = content.trim();

        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            return trimmed.to_string();
        }

        if let Some(start) = trimmed.find('{') {
            if let Some(end) = trimmed.rfind('}') {
                return trimmed[start..=end].to_string();
            }
        }

        trimmed.to_string()
    }
}

impl PipelineAgent for QualityAssuranceAgent {
    fn role(&self) -> AgentRole {
        AgentRole::QualityAssurance
    }
}

/// Quality review result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityReview {
    pub overall_score: f64,
    pub looks_realistic: bool,
    pub has_placeholders: bool,
    pub has_vulnerability_hints: bool,
    pub coding_standards_followed: bool,
    pub issues: Vec<QualityIssue>,
    pub recommendations: Vec<String>,
}

/// A quality issue found during review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityIssue {
    pub severity: String,
    pub description: String,
    pub line: Option<usize>,
}

/// Difficulty Calibrator agent.
pub struct DifficultyCalibrator {
    llm: Arc<dyn LlmProvider>,
    model: String,
}

impl DifficultyCalibrator {
    /// Creates a new difficulty calibrator.
    pub fn new(llm: Arc<dyn LlmProvider>, model: impl Into<String>) -> Self {
        Self {
            llm,
            model: model.into(),
        }
    }

    /// Calibrates task difficulty.
    pub async fn calibrate(
        &self,
        spec: &ProjectSpec,
        vulnerabilities: &[VulnerabilityPlan],
        target_difficulty: DifficultyLevel,
    ) -> Result<DifficultyAssessment, LlmError> {
        let system = AgentRole::DifficultyCalibrator.system_prompt();

        let vuln_summary: Vec<String> = vulnerabilities
            .iter()
            .map(|v| {
                format!(
                    "- {}: {} (subtlety: {})",
                    v.vulnerability_type, v.target_file, v.subtlety_level
                )
            })
            .collect();

        let user = format!(
            r#"Assess and calibrate difficulty for this security benchmark:

Project: {name}
Language: {language}
Target Difficulty: {target}

Vulnerabilities:
{vulns}

Provide assessment as JSON:
{{
    "current_difficulty_score": 0.0-10.0,
    "matches_target": true/false,
    "complexity_score": 0.0-10.0,
    "subtlety_score": 0.0-10.0,
    "scope_score": 0.0-10.0,
    "expertise_required": "beginner/intermediate/advanced/expert",
    "estimated_time_minutes": 60,
    "adjustments_needed": [
        {{"type": "increase/decrease", "area": "what to adjust", "suggestion": "how"}}
    ],
    "overall_assessment": "Summary of difficulty"
}}"#,
            name = spec.name,
            language = spec.language.display_name(),
            target = target_difficulty,
            vulns = vuln_summary.join("\n")
        );

        let request = GenerationRequest::new(
            &self.model,
            vec![Message::system(system), Message::user(&user)],
        )
        .with_temperature(0.5)
        .with_max_tokens(2000);

        let response = self.llm.generate(request).await?;
        let content = response.first_content().unwrap_or_default();

        self.parse_assessment(content)
    }

    fn parse_assessment(&self, content: &str) -> Result<DifficultyAssessment, LlmError> {
        let json_str = self.extract_json(content);

        serde_json::from_str(&json_str).map_err(|e| {
            LlmError::ParseError(format!("Failed to parse difficulty assessment: {}", e))
        })
    }

    fn extract_json(&self, content: &str) -> String {
        let trimmed = content.trim();

        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            return trimmed.to_string();
        }

        if let Some(start) = trimmed.find('{') {
            if let Some(end) = trimmed.rfind('}') {
                return trimmed[start..=end].to_string();
            }
        }

        trimmed.to_string()
    }
}

impl PipelineAgent for DifficultyCalibrator {
    fn role(&self) -> AgentRole {
        AgentRole::DifficultyCalibrator
    }
}

/// Difficulty assessment result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifficultyAssessment {
    pub current_difficulty_score: f64,
    pub matches_target: bool,
    pub complexity_score: f64,
    pub subtlety_score: f64,
    pub scope_score: f64,
    pub expertise_required: String,
    pub estimated_time_minutes: u32,
    pub adjustments_needed: Vec<DifficultyAdjustment>,
    pub overall_assessment: String,
}

/// A suggested difficulty adjustment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifficultyAdjustment {
    #[serde(rename = "type")]
    pub adjustment_type: String,
    pub area: String,
    pub suggestion: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_role_display() {
        assert_eq!(AgentRole::Architect.display_name(), "Architect");
        assert_eq!(AgentRole::CodeGenerator.display_name(), "Code Generator");
    }

    #[test]
    fn test_agent_role_system_prompt() {
        let prompt = AgentRole::Architect.system_prompt();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("architect"));
    }

    #[test]
    fn test_project_design_deserialize() {
        let json = r#"{
            "name": "test-project",
            "description": "A test project",
            "framework": "Flask",
            "directories": ["src", "tests"],
            "key_files": [{"path": "src/main.py", "purpose": "Entry"}],
            "design_rationale": "Simple design",
            "vulnerability_opportunities": []
        }"#;

        let design: ProjectDesign = serde_json::from_str(json).unwrap();
        assert_eq!(design.name, "test-project");
        assert_eq!(design.framework, Some("Flask".to_string()));
    }

    #[test]
    fn test_vulnerability_plan_deserialize() {
        let json = r#"{
            "vulnerability_type": "sql_injection",
            "target_file": "db.py",
            "target_function": "query",
            "injection_technique": "String concat",
            "subtlety_level": 7,
            "detection_difficulty": "medium",
            "cwe_id": "CWE-89",
            "remediation": "Use params"
        }"#;

        let plan: VulnerabilityPlan = serde_json::from_str(json).unwrap();
        assert_eq!(plan.vulnerability_type, "sql_injection");
        assert_eq!(plan.subtlety_level, 7);
    }

    #[test]
    fn test_quality_review_deserialize() {
        let json = r#"{
            "overall_score": 0.85,
            "looks_realistic": true,
            "has_placeholders": false,
            "has_vulnerability_hints": false,
            "coding_standards_followed": true,
            "issues": [],
            "recommendations": ["Add more tests"]
        }"#;

        let review: QualityReview = serde_json::from_str(json).unwrap();
        assert_eq!(review.overall_score, 0.85);
        assert!(review.looks_realistic);
    }
}
