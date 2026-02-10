//! Multi-agent debate system for workspace generation decisions.
//!
//! This module implements a structured debate system where multiple agents
//! discuss and reach consensus on various aspects of workspace generation:
//!
//! - Project design and architecture
//! - Vulnerability selection and placement
//! - Difficulty calibration
//! - Code quality validation

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument};
use uuid::Uuid;

use crate::error::LlmError;
use crate::llm::{GenerationRequest, LlmProvider, Message};

/// Topics that can be debated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebateTopic {
    /// Project architecture and design decisions.
    ProjectDesign,
    /// Selection of vulnerability types.
    VulnerabilitySelection,
    /// Placement of vulnerabilities in code.
    VulnerabilityPlacement,
    /// Difficulty level calibration.
    DifficultyCalibration,
    /// Code quality and realism.
    CodeQuality,
    /// Task feasibility (solvable but challenging).
    Feasibility,
    /// Framework and library choices.
    TechnologyChoice,
    /// File structure and organization.
    FileStructure,
}

impl DebateTopic {
    /// Returns a description of the topic.
    pub fn description(&self) -> &'static str {
        match self {
            Self::ProjectDesign => "Discuss and decide on overall project architecture and design",
            Self::VulnerabilitySelection => {
                "Select appropriate vulnerability types for the project"
            }
            Self::VulnerabilityPlacement => {
                "Determine optimal placement of vulnerabilities in code"
            }
            Self::DifficultyCalibration => "Calibrate difficulty level to match requirements",
            Self::CodeQuality => "Evaluate code quality, realism, and production-readiness",
            Self::Feasibility => {
                "Assess whether the task is solvable but appropriately challenging"
            }
            Self::TechnologyChoice => "Select frameworks, libraries, and technology stack",
            Self::FileStructure => "Design file and directory structure for the project",
        }
    }

    /// Returns the system prompt for agents debating this topic.
    pub fn agent_system_prompt(&self, role: &str) -> String {
        match self {
            Self::ProjectDesign => format!(
                r#"You are a {} discussing project design for a synthetic benchmark workspace.

Your goal is to propose and critique project designs that:
1. Are REALISTIC - match real-world production codebases
2. Are COHERENT - have consistent architecture and patterns
3. Support natural vulnerability injection points
4. Are neither too simple nor overly complex

Provide concrete, specific arguments with examples. Be constructive in critiques.
When proposing, include file structure, key components, and architectural decisions."#,
                role
            ),
            Self::VulnerabilitySelection => format!(
                r#"You are a {} specializing in security vulnerability selection.

Your goal is to propose and critique vulnerability choices that:
1. Are APPROPRIATE for the project type and language
2. Are REALISTIC - match real-world security issues
3. Have appropriate DIFFICULTY for the target level
4. Are SUBTLE - not obviously marked or commented

Consider OWASP Top 10, CWE classifications, and language-specific vulnerabilities.
Argue for/against specific vulnerability types with technical reasoning."#,
                role
            ),
            Self::VulnerabilityPlacement => format!(
                r#"You are a {} specializing in code security and vulnerability analysis.

Your goal is to propose and critique vulnerability placement that:
1. Is NATURAL - fits organically into the codebase
2. Is DISCOVERABLE - can be found through careful analysis
3. Is NOT OBVIOUS - avoids telltale patterns or comments
4. Is REALISTIC - matches how real vulnerabilities occur

Provide specific file locations, function names, and code patterns.
Consider the natural flow of data and control in the application."#,
                role
            ),
            Self::DifficultyCalibration => format!(
                r#"You are a {} specializing in task difficulty assessment.

Your goal is to evaluate and calibrate difficulty:
1. COMPLEXITY - number and interconnection of vulnerabilities
2. SUBTLETY - how well vulnerabilities are hidden
3. SCOPE - amount of code to analyze
4. KNOWLEDGE - required security expertise

Score difficulty on multiple dimensions. Propose adjustments to reach target level.
Consider both finding and fixing the vulnerabilities."#,
                role
            ),
            Self::CodeQuality => format!(
                r#"You are a {} specializing in code quality assessment.

Your goal is to evaluate code for:
1. REALISM - does it look like production code?
2. COMPLETENESS - are all necessary components present?
3. STYLE - does it follow language conventions?
4. NO HINTS - are there any comments or patterns that reveal vulnerabilities?

Flag any issues that break immersion or hint at injected vulnerabilities.
Propose specific improvements to increase code quality."#,
                role
            ),
            Self::Feasibility => format!(
                r#"You are a {} assessing task feasibility.

Your goal is to determine if the task is:
1. SOLVABLE - can be completed with sufficient expertise
2. CHALLENGING - requires real skill to complete
3. NOT IMPOSSIBLE - doesn't require information not available
4. WELL-SCOPED - can be completed in reasonable time

Identify potential blockers, missing information, or unclear requirements.
Propose adjustments to improve feasibility while maintaining challenge."#,
                role
            ),
            Self::TechnologyChoice => format!(
                r#"You are a {} specializing in technology stack decisions.

Your goal is to propose and critique technology choices:
1. APPROPRIATE - fit the project requirements
2. REALISTIC - commonly used in real projects
3. COMPATIBLE - work well together
4. SECURITY-RELEVANT - enable natural vulnerability injection

Consider frameworks, libraries, databases, and tools.
Argue for specific versions and configurations."#,
                role
            ),
            Self::FileStructure => format!(
                r#"You are a {} specializing in project organization.

Your goal is to propose and critique file structures that:
1. FOLLOW CONVENTIONS - match language/framework standards
2. ARE REALISTIC - match real project structures
3. SUPPORT SEPARATION - enable modular vulnerability placement
4. ARE NAVIGABLE - logical organization for analysis

Provide specific directory trees and file naming conventions.
Consider test structure, configuration, and documentation."#,
                role
            ),
        }
    }
}

impl std::fmt::Display for DebateTopic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::ProjectDesign => "Project Design",
            Self::VulnerabilitySelection => "Vulnerability Selection",
            Self::VulnerabilityPlacement => "Vulnerability Placement",
            Self::DifficultyCalibration => "Difficulty Calibration",
            Self::CodeQuality => "Code Quality",
            Self::Feasibility => "Feasibility",
            Self::TechnologyChoice => "Technology Choice",
            Self::FileStructure => "File Structure",
        };
        write!(f, "{}", name)
    }
}

/// A participant in a debate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateAgent {
    /// Unique identifier.
    pub id: String,
    /// Agent name/role.
    pub name: String,
    /// Agent's perspective or specialty.
    pub perspective: String,
    /// Bias towards certain positions (for diversity).
    pub bias: Option<String>,
}

impl DebateAgent {
    /// Creates a new debate agent.
    pub fn new(name: impl Into<String>, perspective: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            perspective: perspective.into(),
            bias: None,
        }
    }

    /// Sets a bias for this agent.
    pub fn with_bias(mut self, bias: impl Into<String>) -> Self {
        self.bias = Some(bias.into());
        self
    }

    /// Creates the Architect agent.
    pub fn architect() -> Self {
        Self::new(
            "Architect",
            "Focuses on overall system design, modularity, and maintainability",
        )
    }

    /// Creates the Security Expert agent.
    pub fn security_expert() -> Self {
        Self::new(
            "Security Expert",
            "Specializes in vulnerability patterns and secure coding practices",
        )
    }

    /// Creates the Developer agent.
    pub fn developer() -> Self {
        Self::new(
            "Developer",
            "Represents typical developer perspective, focusing on practicality",
        )
    }

    /// Creates the Quality Analyst agent.
    pub fn quality_analyst() -> Self {
        Self::new(
            "Quality Analyst",
            "Focuses on code quality, testing, and realism",
        )
    }

    /// Creates the Difficulty Assessor agent.
    pub fn difficulty_assessor() -> Self {
        Self::new(
            "Difficulty Assessor",
            "Specializes in calibrating task difficulty and solvability",
        )
    }
}

/// A message in a debate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateMessage {
    /// Message ID.
    pub id: String,
    /// Agent who sent the message.
    pub agent_id: String,
    /// Agent name.
    pub agent_name: String,
    /// Round number.
    pub round: u32,
    /// The position being argued.
    pub position: String,
    /// Supporting arguments.
    pub arguments: Vec<String>,
    /// Counter-arguments to other positions.
    pub counter_arguments: Vec<CounterArgument>,
    /// Confidence in position (0.0-1.0).
    pub confidence: f64,
    /// Whether the agent agrees with emerging consensus.
    pub agrees_with_consensus: Option<bool>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

impl DebateMessage {
    /// Creates a new debate message.
    pub fn new(agent: &DebateAgent, round: u32, position: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            agent_id: agent.id.clone(),
            agent_name: agent.name.clone(),
            round,
            position: position.into(),
            arguments: Vec::new(),
            counter_arguments: Vec::new(),
            confidence: 0.5,
            agrees_with_consensus: None,
            timestamp: Utc::now(),
        }
    }

    /// Adds an argument.
    pub fn with_argument(mut self, argument: impl Into<String>) -> Self {
        self.arguments.push(argument.into());
        self
    }

    /// Adds arguments.
    pub fn with_arguments<I, S>(mut self, arguments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arguments.extend(arguments.into_iter().map(Into::into));
        self
    }

    /// Adds a counter-argument.
    pub fn with_counter(mut self, counter: CounterArgument) -> Self {
        self.counter_arguments.push(counter);
        self
    }

    /// Sets the confidence.
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }
}

/// A counter-argument to another agent's position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterArgument {
    /// ID of the message being countered.
    pub target_message_id: String,
    /// Agent being countered.
    pub target_agent: String,
    /// The counter-argument text.
    pub argument: String,
    /// Strength of the counter (0.0-1.0).
    pub strength: f64,
}

impl CounterArgument {
    /// Creates a new counter-argument.
    pub fn new(
        target_message_id: impl Into<String>,
        target_agent: impl Into<String>,
        argument: impl Into<String>,
    ) -> Self {
        Self {
            target_message_id: target_message_id.into(),
            target_agent: target_agent.into(),
            argument: argument.into(),
            strength: 0.5,
        }
    }

    /// Sets the strength.
    pub fn with_strength(mut self, strength: f64) -> Self {
        self.strength = strength.clamp(0.0, 1.0);
        self
    }
}

/// A single round of debate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateRound {
    /// Round number.
    pub round_number: u32,
    /// Messages in this round.
    pub messages: Vec<DebateMessage>,
    /// Emerging consensus (if any).
    pub emerging_consensus: Option<String>,
    /// Agreement score (0.0-1.0).
    pub agreement_score: f64,
}

impl DebateRound {
    /// Creates a new debate round.
    pub fn new(round_number: u32) -> Self {
        Self {
            round_number,
            messages: Vec::new(),
            emerging_consensus: None,
            agreement_score: 0.0,
        }
    }

    /// Adds a message to the round.
    pub fn add_message(&mut self, message: DebateMessage) {
        self.messages.push(message);
    }

    /// Calculates the agreement score based on positions.
    pub fn calculate_agreement(&mut self) {
        if self.messages.is_empty() {
            self.agreement_score = 0.0;
            return;
        }

        // Group messages by position
        let mut positions: HashMap<String, usize> = HashMap::new();
        for msg in &self.messages {
            *positions.entry(msg.position.clone()).or_insert(0) += 1;
        }

        // Find the most common position
        let max_count = positions.values().max().copied().unwrap_or(0);
        self.agreement_score = max_count as f64 / self.messages.len() as f64;

        // Set emerging consensus if high agreement
        if self.agreement_score >= 0.5 {
            self.emerging_consensus = positions
                .into_iter()
                .max_by_key(|(_, count)| *count)
                .map(|(pos, _)| pos);
        }
    }
}

/// A complete debate session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateSession {
    /// Session ID.
    pub id: String,
    /// Topic being debated.
    pub topic: DebateTopic,
    /// Context for the debate.
    pub context: String,
    /// Participating agents.
    pub agents: Vec<DebateAgent>,
    /// Debate rounds.
    pub rounds: Vec<DebateRound>,
    /// Final consensus (if reached).
    pub consensus: Option<DebateConsensus>,
    /// Session start time.
    pub started_at: DateTime<Utc>,
    /// Session end time.
    pub ended_at: Option<DateTime<Utc>>,
}

impl DebateSession {
    /// Creates a new debate session.
    pub fn new(topic: DebateTopic, context: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            topic,
            context: context.into(),
            agents: Vec::new(),
            rounds: Vec::new(),
            consensus: None,
            started_at: Utc::now(),
            ended_at: None,
        }
    }

    /// Adds an agent to the session.
    pub fn add_agent(&mut self, agent: DebateAgent) {
        self.agents.push(agent);
    }

    /// Adds a round to the session.
    pub fn add_round(&mut self, round: DebateRound) {
        self.rounds.push(round);
    }

    /// Sets the final consensus.
    pub fn set_consensus(&mut self, consensus: DebateConsensus) {
        self.consensus = Some(consensus);
        self.ended_at = Some(Utc::now());
    }

    /// Returns whether consensus was reached.
    pub fn has_consensus(&self) -> bool {
        self.consensus.as_ref().map_or(false, |c| c.reached)
    }

    /// Returns all messages across all rounds.
    pub fn all_messages(&self) -> Vec<&DebateMessage> {
        self.rounds.iter().flat_map(|r| &r.messages).collect()
    }
}

/// The result of a debate - consensus or lack thereof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateConsensus {
    /// Whether consensus was reached.
    pub reached: bool,
    /// The consensus position (if reached).
    pub position: Option<String>,
    /// Confidence in the consensus (0.0-1.0).
    pub confidence: f64,
    /// Summary of the debate outcome.
    pub summary: String,
    /// Key points of agreement.
    pub agreements: Vec<String>,
    /// Remaining disagreements.
    pub disagreements: Vec<String>,
    /// Recommended action based on debate.
    pub recommendation: String,
}

impl DebateConsensus {
    /// Creates a consensus result.
    pub fn reached(position: impl Into<String>, confidence: f64) -> Self {
        Self {
            reached: true,
            position: Some(position.into()),
            confidence: confidence.clamp(0.0, 1.0),
            summary: String::new(),
            agreements: Vec::new(),
            disagreements: Vec::new(),
            recommendation: String::new(),
        }
    }

    /// Creates a no-consensus result.
    pub fn not_reached(summary: impl Into<String>) -> Self {
        Self {
            reached: false,
            position: None,
            confidence: 0.0,
            summary: summary.into(),
            agreements: Vec::new(),
            disagreements: Vec::new(),
            recommendation: String::new(),
        }
    }

    /// Sets the summary.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = summary.into();
        self
    }

    /// Sets the agreements.
    pub fn with_agreements<I, S>(mut self, agreements: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.agreements = agreements.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the disagreements.
    pub fn with_disagreements<I, S>(mut self, disagreements: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.disagreements = disagreements.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the recommendation.
    pub fn with_recommendation(mut self, recommendation: impl Into<String>) -> Self {
        self.recommendation = recommendation.into();
        self
    }
}

/// Orchestrates multi-agent debates.
pub struct DebateOrchestrator {
    /// LLM client for generating agent responses.
    llm: Arc<dyn LlmProvider>,
    /// Model to use for debate.
    model: String,
    /// Temperature for debate responses.
    temperature: f64,
    /// Maximum tokens per response.
    max_tokens: u32,
    /// Consensus threshold (0.0-1.0).
    consensus_threshold: f64,
}

impl DebateOrchestrator {
    /// Creates a new debate orchestrator.
    pub fn new(llm: Arc<dyn LlmProvider>, model: impl Into<String>) -> Self {
        Self {
            llm,
            model: model.into(),
            temperature: 0.7,
            max_tokens: 2000,
            consensus_threshold: 0.6,
        }
    }

    /// Sets the temperature.
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// Sets the max tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Sets the consensus threshold.
    pub fn with_consensus_threshold(mut self, threshold: f64) -> Self {
        self.consensus_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Conducts a debate on a topic.
    #[instrument(skip(self))]
    pub async fn conduct_debate(
        &self,
        topic: DebateTopic,
        context: &str,
        agents: Vec<DebateAgent>,
        max_rounds: u32,
    ) -> Result<DebateSession, LlmError> {
        info!(
            topic = %topic,
            agents = agents.len(),
            max_rounds = max_rounds,
            "Starting debate session"
        );

        let mut session = DebateSession::new(topic, context);
        for agent in agents {
            session.add_agent(agent);
        }

        for round_num in 1..=max_rounds {
            debug!(round = round_num, "Starting debate round");

            let round = self.conduct_round(&session, round_num).await?;
            let agreement = round.agreement_score;

            session.add_round(round);

            // Check for consensus
            if agreement >= self.consensus_threshold {
                info!(
                    round = round_num,
                    agreement = agreement,
                    "Consensus reached"
                );
                break;
            }
        }

        // Determine final consensus
        let consensus = self.determine_consensus(&session).await?;
        session.set_consensus(consensus);

        info!(
            consensus_reached = session.has_consensus(),
            rounds = session.rounds.len(),
            "Debate completed"
        );

        Ok(session)
    }

    /// Conducts a single round of debate.
    async fn conduct_round(
        &self,
        session: &DebateSession,
        round_num: u32,
    ) -> Result<DebateRound, LlmError> {
        let mut round = DebateRound::new(round_num);

        // Build conversation history
        let history = self.build_conversation_history(session);

        // Get response from each agent
        for agent in &session.agents {
            let message = self
                .get_agent_response(session.topic, &session.context, agent, round_num, &history)
                .await?;

            round.add_message(message);
        }

        round.calculate_agreement();
        Ok(round)
    }

    /// Gets a response from a single agent.
    async fn get_agent_response(
        &self,
        topic: DebateTopic,
        context: &str,
        agent: &DebateAgent,
        round: u32,
        history: &str,
    ) -> Result<DebateMessage, LlmError> {
        let system_prompt = topic.agent_system_prompt(&agent.name);

        let user_prompt = format!(
            r#"DEBATE CONTEXT:
{context}

AGENT PERSPECTIVE: {perspective}
{bias}

ROUND: {round}

PREVIOUS DISCUSSION:
{history}

Please provide your position on this topic. Your response MUST be valid JSON:
{{
    "position": "Your main position/proposal in 1-2 sentences",
    "arguments": [
        "Supporting argument 1",
        "Supporting argument 2",
        "Supporting argument 3"
    ],
    "counter_arguments": [
        {{
            "target": "Agent name you're responding to",
            "argument": "Your counter-argument"
        }}
    ],
    "confidence": 0.0-1.0,
    "agrees_with_emerging_consensus": true/false/null
}}

Be specific and constructive. Provide concrete examples where possible."#,
            context = context,
            perspective = agent.perspective,
            bias = agent
                .bias
                .as_ref()
                .map_or("".to_string(), |b| format!("BIAS: {}", b)),
            round = round,
            history = if history.is_empty() {
                "No previous discussion."
            } else {
                history
            }
        );

        let request = GenerationRequest::new(
            &self.model,
            vec![Message::system(&system_prompt), Message::user(&user_prompt)],
        )
        .with_temperature(self.temperature)
        .with_max_tokens(self.max_tokens);

        let response = self.llm.generate(request).await?;
        let content = response.first_content().unwrap_or_default();

        // Parse the response
        self.parse_agent_response(agent, round, content)
    }

    /// Parses an agent's JSON response into a DebateMessage.
    fn parse_agent_response(
        &self,
        agent: &DebateAgent,
        round: u32,
        content: &str,
    ) -> Result<DebateMessage, LlmError> {
        // Try to extract JSON from the response
        let json_str = self.extract_json(content);

        #[derive(Deserialize)]
        struct AgentResponse {
            position: String,
            arguments: Vec<String>,
            #[serde(default)]
            counter_arguments: Vec<CounterResponse>,
            #[serde(default)]
            confidence: f64,
            #[serde(default)]
            agrees_with_emerging_consensus: Option<bool>,
        }

        #[derive(Deserialize)]
        struct CounterResponse {
            target: String,
            argument: String,
        }

        match serde_json::from_str::<AgentResponse>(&json_str) {
            Ok(parsed) => {
                let mut message = DebateMessage::new(agent, round, &parsed.position)
                    .with_arguments(parsed.arguments)
                    .with_confidence(parsed.confidence);

                message.agrees_with_consensus = parsed.agrees_with_emerging_consensus;

                for counter in parsed.counter_arguments {
                    message = message.with_counter(CounterArgument::new(
                        "", // We don't have message IDs in this simple version
                        counter.target,
                        counter.argument,
                    ));
                }

                Ok(message)
            }
            Err(_) => {
                // Fallback: create a message from the raw content
                Ok(DebateMessage::new(agent, round, content.trim()).with_confidence(0.5))
            }
        }
    }

    /// Extracts JSON from a potentially wrapped response.
    fn extract_json(&self, content: &str) -> String {
        let trimmed = content.trim();

        // Already JSON
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            return trimmed.to_string();
        }

        // Try to find JSON in markdown code block
        if let Some(start) = trimmed.find("```json") {
            if let Some(end) = trimmed[start + 7..].find("```") {
                return trimmed[start + 7..start + 7 + end].trim().to_string();
            }
        }

        // Try to find any JSON object
        if let Some(start) = trimmed.find('{') {
            if let Some(end) = trimmed.rfind('}') {
                return trimmed[start..=end].to_string();
            }
        }

        trimmed.to_string()
    }

    /// Builds conversation history for context.
    fn build_conversation_history(&self, session: &DebateSession) -> String {
        let mut history = Vec::new();

        for round in &session.rounds {
            history.push(format!("--- Round {} ---", round.round_number));
            for msg in &round.messages {
                history.push(format!(
                    "[{}]: {} (confidence: {:.2})",
                    msg.agent_name, msg.position, msg.confidence
                ));
                for arg in &msg.arguments {
                    history.push(format!("  - {}", arg));
                }
            }
            if let Some(ref consensus) = round.emerging_consensus {
                history.push(format!("Emerging consensus: {}", consensus));
            }
        }

        history.join("\n")
    }

    /// Determines the final consensus from a completed debate.
    async fn determine_consensus(
        &self,
        session: &DebateSession,
    ) -> Result<DebateConsensus, LlmError> {
        if session.rounds.is_empty() {
            return Ok(DebateConsensus::not_reached("No debate rounds conducted"));
        }

        let last_round = session.rounds.last().unwrap();

        // Check if consensus threshold was met
        if last_round.agreement_score >= self.consensus_threshold {
            if let Some(ref position) = last_round.emerging_consensus {
                return Ok(
                    DebateConsensus::reached(position, last_round.agreement_score).with_summary(
                        format!(
                            "Consensus reached after {} rounds with {:.0}% agreement",
                            session.rounds.len(),
                            last_round.agreement_score * 100.0
                        ),
                    ),
                );
            }
        }

        // No consensus - summarize the outcome
        let positions: Vec<_> = last_round
            .messages
            .iter()
            .map(|m| format!("{}: {}", m.agent_name, m.position))
            .collect();

        Ok(DebateConsensus::not_reached(format!(
            "No consensus after {} rounds. Final positions: {}",
            session.rounds.len(),
            positions.join("; ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debate_topic_description() {
        let topic = DebateTopic::ProjectDesign;
        assert!(!topic.description().is_empty());
        assert!(!topic.agent_system_prompt("Test").is_empty());
    }

    #[test]
    fn test_debate_agent_creation() {
        let agent = DebateAgent::architect();
        assert_eq!(agent.name, "Architect");
        assert!(!agent.perspective.is_empty());

        let biased = agent.with_bias("Prefers microservices");
        assert_eq!(biased.bias, Some("Prefers microservices".to_string()));
    }

    #[test]
    fn test_debate_message() {
        let agent = DebateAgent::security_expert();
        let message = DebateMessage::new(&agent, 1, "Use SQL injection")
            .with_argument("Common vulnerability")
            .with_confidence(0.8);

        assert_eq!(message.round, 1);
        assert_eq!(message.arguments.len(), 1);
        assert_eq!(message.confidence, 0.8);
    }

    #[test]
    fn test_debate_round_agreement() {
        let agent1 = DebateAgent::architect();
        let agent2 = DebateAgent::developer();

        let mut round = DebateRound::new(1);
        round.add_message(DebateMessage::new(&agent1, 1, "Use REST API"));
        round.add_message(DebateMessage::new(&agent2, 1, "Use REST API"));

        round.calculate_agreement();
        assert_eq!(round.agreement_score, 1.0);
        assert!(round.emerging_consensus.is_some());
    }

    #[test]
    fn test_debate_session() {
        let mut session = DebateSession::new(DebateTopic::VulnerabilitySelection, "Test context");
        session.add_agent(DebateAgent::security_expert());
        session.add_agent(DebateAgent::developer());

        assert_eq!(session.agents.len(), 2);
        assert!(!session.has_consensus());

        session.set_consensus(DebateConsensus::reached("SQL injection", 0.9));
        assert!(session.has_consensus());
    }

    #[test]
    fn test_debate_consensus() {
        let consensus = DebateConsensus::reached("Use parameterized queries", 0.85)
            .with_summary("All agents agreed")
            .with_agreements(vec!["Security is important", "Performance acceptable"]);

        assert!(consensus.reached);
        assert_eq!(consensus.confidence, 0.85);
        assert_eq!(consensus.agreements.len(), 2);
    }
}
