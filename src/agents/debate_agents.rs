//! Debate Agent Personas for Multi-Agent Debate System.
//!
//! This module defines different debate agent roles that bring unique perspectives
//! to discussions about project creation, difficulty levels, and improvements.
//! Each agent has a distinct system prompt that shapes their argumentation style.

use serde::{Deserialize, Serialize};

// ============================================================================
// Debate Agent Roles
// ============================================================================

/// Roles that agents can take in a debate, each with a distinct perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebateAgentRole {
    /// Pushes for creative, novel, and innovative solutions.
    Innovator,
    /// Focuses on practicality, feasibility, and real-world constraints.
    Pragmatist,
    /// Points out flaws, challenges assumptions, and identifies risks.
    Critic,
    /// Supports and enhances ideas, finds strengths and builds on them.
    Advocate,
    /// Checks for correctness, completeness, and consistency.
    Validator,
}

impl DebateAgentRole {
    /// Returns all available debate roles.
    pub fn all() -> Vec<Self> {
        vec![
            Self::Innovator,
            Self::Pragmatist,
            Self::Critic,
            Self::Advocate,
            Self::Validator,
        ]
    }

    /// Returns the display name for this role.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Innovator => "Innovator",
            Self::Pragmatist => "Pragmatist",
            Self::Critic => "Critic",
            Self::Advocate => "Advocate",
            Self::Validator => "Validator",
        }
    }

    /// Returns a brief description of this role's perspective.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Innovator => "Pushes for creative and novel approaches",
            Self::Pragmatist => "Focuses on practical feasibility",
            Self::Critic => "Identifies flaws and challenges",
            Self::Advocate => "Supports and enhances ideas",
            Self::Validator => "Ensures correctness and completeness",
        }
    }

    /// Returns the system prompt that defines this agent's debate behavior.
    pub fn system_prompt(&self) -> &'static str {
        match self {
            Self::Innovator => INNOVATOR_SYSTEM_PROMPT,
            Self::Pragmatist => PRAGMATIST_SYSTEM_PROMPT,
            Self::Critic => CRITIC_SYSTEM_PROMPT,
            Self::Advocate => ADVOCATE_SYSTEM_PROMPT,
            Self::Validator => VALIDATOR_SYSTEM_PROMPT,
        }
    }

    /// Returns the default expertise score for this role on different topics.
    pub fn expertise_score(&self, topic: &DebateTopic) -> f64 {
        match (self, topic) {
            // Innovator excels at project type ideation
            (Self::Innovator, DebateTopic::ProjectType) => 0.9,
            (Self::Innovator, DebateTopic::Improvements) => 0.85,
            (Self::Innovator, DebateTopic::Difficulty) => 0.6,
            (Self::Innovator, DebateTopic::Feasibility) => 0.5,

            // Pragmatist excels at feasibility assessment
            (Self::Pragmatist, DebateTopic::Feasibility) => 0.95,
            (Self::Pragmatist, DebateTopic::Difficulty) => 0.8,
            (Self::Pragmatist, DebateTopic::ProjectType) => 0.7,
            (Self::Pragmatist, DebateTopic::Improvements) => 0.7,

            // Critic excels at difficulty assessment
            (Self::Critic, DebateTopic::Difficulty) => 0.9,
            (Self::Critic, DebateTopic::Feasibility) => 0.85,
            (Self::Critic, DebateTopic::Improvements) => 0.75,
            (Self::Critic, DebateTopic::ProjectType) => 0.65,

            // Advocate supports improvement discussions
            (Self::Advocate, DebateTopic::Improvements) => 0.9,
            (Self::Advocate, DebateTopic::ProjectType) => 0.8,
            (Self::Advocate, DebateTopic::Feasibility) => 0.7,
            (Self::Advocate, DebateTopic::Difficulty) => 0.65,

            // Validator ensures correctness across all topics
            (Self::Validator, DebateTopic::Feasibility) => 0.9,
            (Self::Validator, DebateTopic::Improvements) => 0.85,
            (Self::Validator, DebateTopic::Difficulty) => 0.85,
            (Self::Validator, DebateTopic::ProjectType) => 0.75,
        }
    }
}

impl std::fmt::Display for DebateAgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

// ============================================================================
// Debate Topics
// ============================================================================

/// Topics that can be debated by agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebateTopic {
    /// What type of project to create (web API, CLI tool, library, etc.).
    ProjectType,
    /// How difficult the task should be.
    Difficulty,
    /// What improvements to make to a proposed solution.
    Improvements,
    /// Whether a proposed task is feasible and solvable.
    Feasibility,
}

impl DebateTopic {
    /// Returns the display name for this topic.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::ProjectType => "Project Type Selection",
            Self::Difficulty => "Difficulty Assessment",
            Self::Improvements => "Improvement Proposals",
            Self::Feasibility => "Feasibility Analysis",
        }
    }

    /// Returns a description of what this topic covers.
    pub fn description(&self) -> &'static str {
        match self {
            Self::ProjectType => "Discussing what type of project structure and technology to use",
            Self::Difficulty => "Determining the appropriate complexity level for the task",
            Self::Improvements => "Identifying and prioritizing potential improvements",
            Self::Feasibility => "Assessing whether the proposed approach is achievable",
        }
    }

    /// Returns the prompt template for initiating debate on this topic.
    pub fn debate_prompt_template(&self) -> &'static str {
        match self {
            Self::ProjectType => PROJECT_TYPE_DEBATE_TEMPLATE,
            Self::Difficulty => DIFFICULTY_DEBATE_TEMPLATE,
            Self::Improvements => IMPROVEMENTS_DEBATE_TEMPLATE,
            Self::Feasibility => FEASIBILITY_DEBATE_TEMPLATE,
        }
    }
}

impl std::fmt::Display for DebateTopic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

// ============================================================================
// Agent Position
// ============================================================================

/// An agent's position or stance on a debate topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPosition {
    /// The role of the agent taking this position.
    pub role: DebateAgentRole,
    /// The agent's main claim or stance.
    pub claim: String,
    /// Evidence and reasoning supporting the claim.
    pub evidence: Vec<String>,
    /// The conclusion or recommendation.
    pub conclusion: String,
    /// Confidence level in this position (0.0 - 1.0).
    pub confidence: f64,
    /// Optional counter-arguments the agent acknowledges.
    pub acknowledged_weaknesses: Vec<String>,
}

impl AgentPosition {
    /// Creates a new agent position.
    pub fn new(
        role: DebateAgentRole,
        claim: impl Into<String>,
        conclusion: impl Into<String>,
        confidence: f64,
    ) -> Self {
        Self {
            role,
            claim: claim.into(),
            evidence: Vec::new(),
            conclusion: conclusion.into(),
            confidence: confidence.clamp(0.0, 1.0),
            acknowledged_weaknesses: Vec::new(),
        }
    }

    /// Adds evidence supporting the position.
    pub fn with_evidence(mut self, evidence: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.evidence = evidence.into_iter().map(|e| e.into()).collect();
        self
    }

    /// Adds acknowledged weaknesses.
    pub fn with_weaknesses(
        mut self,
        weaknesses: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.acknowledged_weaknesses = weaknesses.into_iter().map(|w| w.into()).collect();
        self
    }

    /// Calculates a weighted score based on confidence and expertise.
    pub fn weighted_score(&self, topic: &DebateTopic) -> f64 {
        let expertise = self.role.expertise_score(topic);
        self.confidence * expertise
    }
}

// ============================================================================
// Debate Response Parsing
// ============================================================================

/// Raw response structure from LLM for debate positions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateResponse {
    /// The main claim or stance.
    pub claim: String,
    /// Evidence points supporting the claim.
    pub evidence: Vec<String>,
    /// The conclusion or recommendation.
    pub conclusion: String,
    /// Confidence in the position (0.0 - 1.0).
    pub confidence: f64,
    /// Counter-arguments the agent acknowledges.
    #[serde(default)]
    pub acknowledged_weaknesses: Vec<String>,
    /// Response to other agents' positions (if in a later round).
    #[serde(default)]
    pub responses_to_others: Vec<ResponseToOther>,
}

/// A response to another agent's position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseToOther {
    /// The role being responded to.
    pub responding_to: String,
    /// Whether the agent agrees, disagrees, or partially agrees.
    pub stance: String,
    /// The response content.
    pub response: String,
}

// ============================================================================
// System Prompts for Each Role
// ============================================================================

const INNOVATOR_SYSTEM_PROMPT: &str = r#"You are the INNOVATOR in a multi-agent debate. Your role is to push boundaries and propose creative, novel solutions.

YOUR PERSPECTIVE:
- Challenge conventional approaches
- Propose innovative alternatives that others might overlook
- Think beyond immediate constraints to find breakthrough solutions
- Embrace calculated risks for potentially higher rewards
- Connect ideas from different domains

DEBATE STYLE:
- Be bold in your proposals but ground them in reasoning
- Acknowledge practical constraints but explore ways around them
- Build on others' ideas to make them more innovative
- Challenge "this is how it's always done" thinking

When responding, provide a JSON object with:
{
  "claim": "Your main innovative proposal",
  "evidence": ["Reasoning point 1", "Example or precedent 2", "Technical justification 3"],
  "conclusion": "Your recommended approach",
  "confidence": 0.0-1.0,
  "acknowledged_weaknesses": ["Potential issue 1", "Risk 2"],
  "responses_to_others": [{"responding_to": "role", "stance": "agree/disagree/partial", "response": "Your response"}]
}

Be creative but substantive. Innovation without foundation is mere speculation."#;

const PRAGMATIST_SYSTEM_PROMPT: &str = r#"You are the PRAGMATIST in a multi-agent debate. Your role is to ensure proposals are practical and achievable.

YOUR PERSPECTIVE:
- Focus on real-world constraints and limitations
- Consider resource requirements (time, complexity, dependencies)
- Evaluate implementation feasibility
- Identify what can realistically be delivered
- Balance ambition with achievability

DEBATE STYLE:
- Ground discussions in practical reality
- Ask "how will this actually work?"
- Identify hidden complexities and dependencies
- Propose realistic timelines and scope
- Don't reject innovation, but ensure it's feasible

When responding, provide a JSON object with:
{
  "claim": "Your assessment of practicality",
  "evidence": ["Practical consideration 1", "Resource analysis 2", "Feasibility factor 3"],
  "conclusion": "Your recommendation on viability",
  "confidence": 0.0-1.0,
  "acknowledged_weaknesses": ["What you might be overly cautious about"],
  "responses_to_others": [{"responding_to": "role", "stance": "agree/disagree/partial", "response": "Your response"}]
}

Be realistic but not defeatist. Practicality enables progress."#;

const CRITIC_SYSTEM_PROMPT: &str = r#"You are the CRITIC in a multi-agent debate. Your role is to identify flaws, risks, and potential problems.

YOUR PERSPECTIVE:
- Find weaknesses in proposals before they become real problems
- Challenge assumptions that others take for granted
- Identify edge cases and failure modes
- Consider security, reliability, and maintenance concerns
- Ask difficult questions others might avoid

DEBATE STYLE:
- Be constructively critical, not destructive
- Identify specific problems, not vague concerns
- Suggest what would need to change to address issues
- Acknowledge strengths while highlighting weaknesses
- Focus on substance, not style

When responding, provide a JSON object with:
{
  "claim": "The main issue or concern you identify",
  "evidence": ["Specific flaw 1", "Risk factor 2", "Overlooked problem 3"],
  "conclusion": "What needs to be addressed",
  "confidence": 0.0-1.0,
  "acknowledged_weaknesses": ["Where you might be overly critical"],
  "responses_to_others": [{"responding_to": "role", "stance": "agree/disagree/partial", "response": "Your response"}]
}

Critical analysis improves outcomes. Find problems before they find you."#;

const ADVOCATE_SYSTEM_PROMPT: &str = r#"You are the ADVOCATE in a multi-agent debate. Your role is to support promising ideas and help them succeed.

YOUR PERSPECTIVE:
- Find strengths and potential in proposals
- Help refine and improve ideas constructively
- Build bridges between conflicting positions
- Identify what's working and amplify it
- Champion good ideas that might otherwise be dismissed

DEBATE STYLE:
- Support with specific reasoning, not blind enthusiasm
- Help others see the value in proposals
- Find ways to address criticisms while preserving core ideas
- Synthesize different viewpoints into stronger proposals
- Advocate for the user's needs and goals

When responding, provide a JSON object with:
{
  "claim": "The key strength or value you identify",
  "evidence": ["Supporting point 1", "Benefit 2", "Opportunity 3"],
  "conclusion": "How to build on this foundation",
  "confidence": 0.0-1.0,
  "acknowledged_weaknesses": ["Valid concerns from others"],
  "responses_to_others": [{"responding_to": "role", "stance": "agree/disagree/partial", "response": "Your response"}]
}

Good ideas need champions. Help the best proposals succeed."#;

const VALIDATOR_SYSTEM_PROMPT: &str = r#"You are the VALIDATOR in a multi-agent debate. Your role is to ensure correctness, completeness, and consistency.

YOUR PERSPECTIVE:
- Check that proposals meet all requirements
- Verify logical consistency across the discussion
- Ensure nothing important has been overlooked
- Validate that conclusions follow from evidence
- Confirm alignment with stated goals and constraints

DEBATE STYLE:
- Be thorough and systematic
- Cross-reference claims against requirements
- Identify gaps in reasoning or coverage
- Ensure the final decision is well-supported
- Synthesize the debate into actionable conclusions

When responding, provide a JSON object with:
{
  "claim": "Your validation assessment",
  "evidence": ["Verification point 1", "Completeness check 2", "Consistency analysis 3"],
  "conclusion": "Whether the proposal meets requirements",
  "confidence": 0.0-1.0,
  "acknowledged_weaknesses": ["Areas of uncertainty"],
  "responses_to_others": [{"responding_to": "role", "stance": "agree/disagree/partial", "response": "Your response"}]
}

Validation ensures quality. Nothing leaves without verification."#;

// ============================================================================
// Debate Topic Prompt Templates
// ============================================================================

const PROJECT_TYPE_DEBATE_TEMPLATE: &str = r#"DEBATE TOPIC: Project Type Selection

Context:
{context}

Language: {language}
Category Hint: {category}

Your task is to argue for the most appropriate project type. Consider:
- What type of project best fits this context?
- What technologies and frameworks should be used?
- What structure would make the task challenging but achievable?
- How does the project type affect difficulty and learning value?

Previous positions (if any):
{previous_positions}

Provide your position as a JSON object."#;

const DIFFICULTY_DEBATE_TEMPLATE: &str = r#"DEBATE TOPIC: Difficulty Assessment

Context:
{context}

Proposed Task:
{task_description}

Your task is to argue for the appropriate difficulty level. Consider:
- How many steps are required to solve this?
- What skills and knowledge are needed?
- Are there hidden complexities or gotchas?
- Is this too easy (memorizable) or too hard (impossible)?

Previous positions (if any):
{previous_positions}

Provide your position as a JSON object."#;

const IMPROVEMENTS_DEBATE_TEMPLATE: &str = r#"DEBATE TOPIC: Improvement Proposals

Context:
{context}

Current Implementation:
{implementation}

Your task is to argue for the most important improvements. Consider:
- What would make this more challenging?
- What would make this more realistic?
- What edge cases should be added?
- What would increase learning value?

Previous positions (if any):
{previous_positions}

Provide your position as a JSON object."#;

const FEASIBILITY_DEBATE_TEMPLATE: &str = r#"DEBATE TOPIC: Feasibility Analysis

Context:
{context}

Proposed Approach:
{approach}

Your task is to argue about feasibility. Consider:
- Can this be implemented in the given constraints?
- Are there hidden dependencies or requirements?
- Is the solution path clear enough to be solvable?
- Are there blockers that make this impossible?

Previous positions (if any):
{previous_positions}

Provide your position as a JSON object."#;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debate_role_all() {
        let roles = DebateAgentRole::all();
        assert_eq!(roles.len(), 5);
        assert!(roles.contains(&DebateAgentRole::Innovator));
        assert!(roles.contains(&DebateAgentRole::Pragmatist));
        assert!(roles.contains(&DebateAgentRole::Critic));
        assert!(roles.contains(&DebateAgentRole::Advocate));
        assert!(roles.contains(&DebateAgentRole::Validator));
    }

    #[test]
    fn test_debate_role_display() {
        assert_eq!(DebateAgentRole::Innovator.display_name(), "Innovator");
        assert_eq!(DebateAgentRole::Pragmatist.display_name(), "Pragmatist");
        assert_eq!(DebateAgentRole::Critic.display_name(), "Critic");
        assert_eq!(DebateAgentRole::Advocate.display_name(), "Advocate");
        assert_eq!(DebateAgentRole::Validator.display_name(), "Validator");
    }

    #[test]
    fn test_debate_role_system_prompts_not_empty() {
        for role in DebateAgentRole::all() {
            let prompt = role.system_prompt();
            assert!(!prompt.is_empty(), "{:?} should have a system prompt", role);
            assert!(
                prompt.len() > 100,
                "{:?} system prompt should be substantial",
                role
            );
        }
    }

    #[test]
    fn test_debate_topic_display() {
        assert_eq!(
            DebateTopic::ProjectType.display_name(),
            "Project Type Selection"
        );
        assert_eq!(
            DebateTopic::Difficulty.display_name(),
            "Difficulty Assessment"
        );
        assert_eq!(
            DebateTopic::Improvements.display_name(),
            "Improvement Proposals"
        );
        assert_eq!(
            DebateTopic::Feasibility.display_name(),
            "Feasibility Analysis"
        );
    }

    #[test]
    fn test_expertise_scores() {
        // Innovator should excel at project type
        let innovator_score = DebateAgentRole::Innovator.expertise_score(&DebateTopic::ProjectType);
        assert!(innovator_score > 0.8);

        // Pragmatist should excel at feasibility
        let pragmatist_score =
            DebateAgentRole::Pragmatist.expertise_score(&DebateTopic::Feasibility);
        assert!(pragmatist_score > 0.9);

        // Critic should excel at difficulty
        let critic_score = DebateAgentRole::Critic.expertise_score(&DebateTopic::Difficulty);
        assert!(critic_score > 0.8);
    }

    #[test]
    fn test_agent_position_creation() {
        let position = AgentPosition::new(
            DebateAgentRole::Innovator,
            "We should use a microservices architecture",
            "This enables better scalability",
            0.8,
        )
        .with_evidence(["Industry trend", "Better isolation", "Easier testing"])
        .with_weaknesses(["Higher initial complexity"]);

        assert_eq!(position.role, DebateAgentRole::Innovator);
        assert_eq!(position.evidence.len(), 3);
        assert_eq!(position.acknowledged_weaknesses.len(), 1);
        assert!((position.confidence - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_weighted_score() {
        let position = AgentPosition::new(
            DebateAgentRole::Pragmatist,
            "This is feasible",
            "We can do it",
            0.9,
        );

        // Pragmatist on feasibility: expertise 0.95, confidence 0.9 = 0.855
        let score = position.weighted_score(&DebateTopic::Feasibility);
        assert!((score - 0.855).abs() < 0.01);
    }

    #[test]
    fn test_confidence_clamping() {
        let high = AgentPosition::new(DebateAgentRole::Critic, "claim", "conclusion", 1.5);
        assert!((high.confidence - 1.0).abs() < 0.01);

        let low = AgentPosition::new(DebateAgentRole::Critic, "claim", "conclusion", -0.5);
        assert!((low.confidence - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_debate_topic_templates_not_empty() {
        for topic in [
            DebateTopic::ProjectType,
            DebateTopic::Difficulty,
            DebateTopic::Improvements,
            DebateTopic::Feasibility,
        ] {
            let template = topic.debate_prompt_template();
            assert!(!template.is_empty(), "{:?} should have a template", topic);
            assert!(
                template.contains("{context}"),
                "{:?} template should have context placeholder",
                topic
            );
        }
    }
}
