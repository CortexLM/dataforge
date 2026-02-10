//! Multi-Agent Debate Orchestrator for Consensus Building.
//!
//! This module provides an orchestrator that coordinates multi-agent debates
//! to reach consensus on various topics like project type, difficulty, and feasibility.
//! Multiple "agents" (same LLM with different system prompts) debate and vote to
//! determine the best approach.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::debate_agents::{
    AgentPosition, DebateAgentRole, DebateResponse, DebateTopic,
};
use super::error::{AgentError, AgentResult};
use crate::llm::{GenerationRequest, LlmProvider, Message};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the debate orchestrator.
#[derive(Debug, Clone)]
pub struct DebateOrchestratorConfig {
    /// Number of debate rounds to conduct.
    pub debate_rounds: u32,
    /// Consensus threshold (fraction of agents that must agree).
    pub consensus_threshold: f64,
    /// Which agents participate in debates.
    pub participating_roles: Vec<DebateAgentRole>,
    /// LLM temperature for debate responses.
    pub temperature: f64,
    /// Maximum tokens per debate response.
    pub max_tokens: u32,
    /// Consensus mechanism to use.
    pub consensus_mechanism: ConsensusMechanism,
    /// Whether to use weighted voting based on expertise.
    pub use_weighted_voting: bool,
}

impl Default for DebateOrchestratorConfig {
    fn default() -> Self {
        Self {
            debate_rounds: 3,
            consensus_threshold: 0.6,
            participating_roles: DebateAgentRole::all(),
            temperature: 0.7,
            max_tokens: 2000,
            consensus_mechanism: ConsensusMechanism::SimpleMajority,
            use_weighted_voting: true,
        }
    }
}

impl DebateOrchestratorConfig {
    /// Creates a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the number of debate rounds.
    pub fn with_debate_rounds(mut self, rounds: u32) -> Self {
        self.debate_rounds = rounds.max(1);
        self
    }

    /// Sets the consensus threshold.
    pub fn with_consensus_threshold(mut self, threshold: f64) -> Self {
        self.consensus_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Sets which agents participate in debates.
    pub fn with_participating_roles(mut self, roles: Vec<DebateAgentRole>) -> Self {
        self.participating_roles = if roles.is_empty() {
            DebateAgentRole::all()
        } else {
            roles
        };
        self
    }

    /// Sets the LLM temperature.
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// Sets the consensus mechanism.
    pub fn with_consensus_mechanism(mut self, mechanism: ConsensusMechanism) -> Self {
        self.consensus_mechanism = mechanism;
        self
    }

    /// Sets whether to use weighted voting.
    pub fn with_weighted_voting(mut self, enabled: bool) -> Self {
        self.use_weighted_voting = enabled;
        self
    }
}

// ============================================================================
// Consensus Mechanisms
// ============================================================================

/// Mechanisms for determining consensus from agent votes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsensusMechanism {
    /// Simple majority (>50% must agree).
    SimpleMajority,
    /// Supermajority (>66% must agree).
    Supermajority,
    /// All agents must agree.
    Unanimous,
    /// Votes are weighted by agent expertise scores.
    WeightedVoting,
}

impl ConsensusMechanism {
    /// Returns the required threshold for this mechanism.
    pub fn required_threshold(&self) -> f64 {
        match self {
            Self::SimpleMajority => 0.5,
            Self::Supermajority => 0.67,
            Self::Unanimous => 1.0,
            Self::WeightedVoting => 0.5, // Base threshold, actual uses weights
        }
    }

    /// Returns the display name for this mechanism.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::SimpleMajority => "Simple Majority",
            Self::Supermajority => "Supermajority (2/3)",
            Self::Unanimous => "Unanimous",
            Self::WeightedVoting => "Weighted Voting",
        }
    }
}

impl std::fmt::Display for ConsensusMechanism {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

// ============================================================================
// Debate Round
// ============================================================================

/// A single round of debate containing all agent positions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateRound {
    /// Round number (1-indexed).
    pub round_number: u32,
    /// Positions taken by each agent in this round.
    pub positions: Vec<AgentPosition>,
    /// When this round was completed.
    pub completed_at: DateTime<Utc>,
}

impl DebateRound {
    /// Creates a new debate round.
    pub fn new(round_number: u32, positions: Vec<AgentPosition>) -> Self {
        Self {
            round_number,
            positions,
            completed_at: Utc::now(),
        }
    }

    /// Returns a summary of positions for display.
    pub fn summarize_positions(&self) -> String {
        self.positions
            .iter()
            .map(|p| format!("[{}]: {}", p.role.display_name(), p.claim))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ============================================================================
// Consensus Result
// ============================================================================

/// The outcome of a debate including the consensus (or lack thereof).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusResult {
    /// Unique identifier for this debate.
    pub debate_id: String,
    /// The topic that was debated.
    pub topic: DebateTopic,
    /// Whether consensus was reached.
    pub consensus_reached: bool,
    /// The winning position (if consensus reached).
    pub winning_position: Option<String>,
    /// The consensus score (percentage of agreement).
    pub consensus_score: f64,
    /// All rounds of the debate.
    pub rounds: Vec<DebateRound>,
    /// Final votes from each agent.
    pub final_votes: HashMap<String, String>,
    /// Total duration of the debate in milliseconds.
    pub duration_ms: u64,
    /// Mechanism used to determine consensus.
    pub mechanism_used: ConsensusMechanism,
    /// Dissenting opinions (agents who disagreed with consensus).
    pub dissenting_opinions: Vec<DissentingOpinion>,
}

impl ConsensusResult {
    /// Creates a new consensus result.
    pub fn new(
        topic: DebateTopic,
        consensus_reached: bool,
        winning_position: Option<String>,
        consensus_score: f64,
        rounds: Vec<DebateRound>,
        mechanism_used: ConsensusMechanism,
        duration_ms: u64,
    ) -> Self {
        Self {
            debate_id: Uuid::new_v4().to_string(),
            topic,
            consensus_reached,
            winning_position,
            consensus_score,
            rounds,
            final_votes: HashMap::new(),
            duration_ms,
            mechanism_used,
            dissenting_opinions: Vec::new(),
        }
    }

    /// Adds final votes to the result.
    pub fn with_votes(mut self, votes: HashMap<String, String>) -> Self {
        self.final_votes = votes;
        self
    }

    /// Adds dissenting opinions.
    pub fn with_dissent(mut self, dissent: Vec<DissentingOpinion>) -> Self {
        self.dissenting_opinions = dissent;
        self
    }
}

/// A dissenting opinion from an agent who disagreed with consensus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissentingOpinion {
    /// The role that dissented.
    pub role: DebateAgentRole,
    /// Their alternative position.
    pub position: String,
    /// Their reasoning for dissent.
    pub reasoning: String,
}

// ============================================================================
// Debate Events
// ============================================================================

/// Events emitted during the debate process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DebateEvent {
    /// Debate has started.
    DebateStarted {
        /// Topic being debated.
        topic: DebateTopic,
        /// Number of participating agents.
        participant_count: usize,
        /// Planned number of rounds.
        planned_rounds: u32,
        /// When debate started.
        timestamp: DateTime<Utc>,
    },
    /// A debate round has started.
    RoundStarted {
        /// Round number.
        round: u32,
        /// When round started.
        timestamp: DateTime<Utc>,
    },
    /// An agent has provided their position.
    AgentPositionReceived {
        /// The agent's role.
        role: DebateAgentRole,
        /// Summary of their position.
        claim_summary: String,
        /// Their confidence level.
        confidence: f64,
        /// When position was received.
        timestamp: DateTime<Utc>,
    },
    /// A debate round has completed.
    RoundCompleted {
        /// Round number.
        round: u32,
        /// Number of positions received.
        positions_count: usize,
        /// When round completed.
        timestamp: DateTime<Utc>,
    },
    /// Consensus check result.
    ConsensusCheck {
        /// Current consensus score.
        score: f64,
        /// Whether threshold is met.
        threshold_met: bool,
        /// When check occurred.
        timestamp: DateTime<Utc>,
    },
    /// Debate has completed.
    DebateCompleted {
        /// Whether consensus was reached.
        consensus_reached: bool,
        /// The winning position (if any).
        winning_position: Option<String>,
        /// Final consensus score.
        consensus_score: f64,
        /// Total duration in milliseconds.
        duration_ms: u64,
        /// When debate completed.
        timestamp: DateTime<Utc>,
    },
    /// An error occurred during debate.
    DebateError {
        /// Error description.
        error: String,
        /// When error occurred.
        timestamp: DateTime<Utc>,
    },
}

impl DebateEvent {
    /// Creates a DebateStarted event.
    pub fn debate_started(
        topic: DebateTopic,
        participant_count: usize,
        planned_rounds: u32,
    ) -> Self {
        Self::DebateStarted {
            topic,
            participant_count,
            planned_rounds,
            timestamp: Utc::now(),
        }
    }

    /// Creates a RoundStarted event.
    pub fn round_started(round: u32) -> Self {
        Self::RoundStarted {
            round,
            timestamp: Utc::now(),
        }
    }

    /// Creates an AgentPositionReceived event.
    pub fn agent_position_received(
        role: DebateAgentRole,
        claim_summary: impl Into<String>,
        confidence: f64,
    ) -> Self {
        Self::AgentPositionReceived {
            role,
            claim_summary: claim_summary.into(),
            confidence,
            timestamp: Utc::now(),
        }
    }

    /// Creates a RoundCompleted event.
    pub fn round_completed(round: u32, positions_count: usize) -> Self {
        Self::RoundCompleted {
            round,
            positions_count,
            timestamp: Utc::now(),
        }
    }

    /// Creates a ConsensusCheck event.
    pub fn consensus_check(score: f64, threshold_met: bool) -> Self {
        Self::ConsensusCheck {
            score,
            threshold_met,
            timestamp: Utc::now(),
        }
    }

    /// Creates a DebateCompleted event.
    pub fn debate_completed(
        consensus_reached: bool,
        winning_position: Option<String>,
        consensus_score: f64,
        duration_ms: u64,
    ) -> Self {
        Self::DebateCompleted {
            consensus_reached,
            winning_position,
            consensus_score,
            duration_ms,
            timestamp: Utc::now(),
        }
    }

    /// Creates a DebateError event.
    pub fn debate_error(error: impl Into<String>) -> Self {
        Self::DebateError {
            error: error.into(),
            timestamp: Utc::now(),
        }
    }
}

// ============================================================================
// Debate Context
// ============================================================================

/// Context provided to agents for a debate.
#[derive(Debug, Clone)]
pub struct DebateContext {
    /// The topic being debated.
    pub topic: DebateTopic,
    /// Background context (e.g., project requirements).
    pub context: String,
    /// Additional parameters specific to the topic.
    pub parameters: HashMap<String, String>,
}

impl DebateContext {
    /// Creates a new debate context.
    pub fn new(topic: DebateTopic, context: impl Into<String>) -> Self {
        Self {
            topic,
            context: context.into(),
            parameters: HashMap::new(),
        }
    }

    /// Adds a parameter to the context.
    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.parameters.insert(key.into(), value.into());
        self
    }

    /// Builds the user prompt for this context.
    pub fn build_prompt(&self, previous_positions: &str) -> String {
        let template = self.topic.debate_prompt_template();
        let mut prompt = template.replace("{context}", &self.context);
        prompt = prompt.replace("{previous_positions}", previous_positions);

        // Replace any topic-specific parameters
        for (key, value) in &self.parameters {
            prompt = prompt.replace(&format!("{{{}}}", key), value);
        }

        // Replace any remaining placeholders with empty string
        prompt = prompt.replace("{language}", "");
        prompt = prompt.replace("{category}", "");
        prompt = prompt.replace("{task_description}", "");
        prompt = prompt.replace("{implementation}", "");
        prompt = prompt.replace("{approach}", "");

        prompt
    }
}

// ============================================================================
// Debate Orchestrator
// ============================================================================

/// Orchestrator for multi-agent debates.
///
/// This orchestrator coordinates debates between multiple agent personas (same LLM
/// with different system prompts) to reach consensus on various topics.
pub struct DebateOrchestrator {
    /// LLM client for agent interactions.
    llm_client: Arc<dyn LlmProvider>,
    /// Orchestrator configuration.
    config: DebateOrchestratorConfig,
}

impl DebateOrchestrator {
    /// Agent name constant for identification.
    pub const AGENT_NAME: &'static str = "debate_orchestrator";

    /// Creates a new debate orchestrator.
    pub fn new(llm_client: Arc<dyn LlmProvider>, config: DebateOrchestratorConfig) -> Self {
        Self { llm_client, config }
    }

    /// Creates a new orchestrator with default configuration.
    pub fn with_defaults(llm_client: Arc<dyn LlmProvider>) -> Self {
        Self::new(llm_client, DebateOrchestratorConfig::default())
    }

    /// Creates a new builder for configuring the orchestrator.
    pub fn builder() -> DebateOrchestratorBuilder {
        DebateOrchestratorBuilder::new()
    }

    /// Returns the configuration.
    pub fn config(&self) -> &DebateOrchestratorConfig {
        &self.config
    }

    /// Conducts a full debate on the given topic.
    ///
    /// # Arguments
    ///
    /// * `context` - The debate context including topic and background.
    /// * `event_tx` - Channel for emitting debate events.
    ///
    /// # Returns
    ///
    /// The consensus result from the debate.
    pub async fn conduct_debate(
        &self,
        context: DebateContext,
        event_tx: mpsc::Sender<DebateEvent>,
    ) -> AgentResult<ConsensusResult> {
        let start_time = Instant::now();
        let topic = context.topic;

        // Emit debate started event
        self.send_event(
            &event_tx,
            DebateEvent::debate_started(
                topic,
                self.config.participating_roles.len(),
                self.config.debate_rounds,
            ),
        )
        .await;

        let mut rounds: Vec<DebateRound> = Vec::new();
        let mut previous_positions_summary = String::new();

        // Conduct debate rounds
        for round_num in 1..=self.config.debate_rounds {
            self.send_event(&event_tx, DebateEvent::round_started(round_num))
                .await;

            let round = self
                .conduct_round(round_num, &context, &previous_positions_summary, &event_tx)
                .await?;

            self.send_event(
                &event_tx,
                DebateEvent::round_completed(round_num, round.positions.len()),
            )
            .await;

            // Check for early consensus
            let consensus_score = self.calculate_consensus_score(&round.positions, &topic);
            let threshold_met = consensus_score >= self.config.consensus_threshold;

            self.send_event(
                &event_tx,
                DebateEvent::consensus_check(consensus_score, threshold_met),
            )
            .await;

            previous_positions_summary = round.summarize_positions();
            rounds.push(round);

            // Early exit if strong consensus reached
            if threshold_met && round_num < self.config.debate_rounds {
                tracing::info!(
                    "Early consensus reached in round {} with score {:.2}",
                    round_num,
                    consensus_score
                );
                break;
            }
        }

        // Determine final consensus
        let final_round = rounds.last().expect("at least one round completed");
        let (consensus_reached, winning_position, consensus_score, final_votes, dissent) =
            self.determine_consensus(&final_round.positions, &topic);

        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Emit completion event
        self.send_event(
            &event_tx,
            DebateEvent::debate_completed(
                consensus_reached,
                winning_position.clone(),
                consensus_score,
                duration_ms,
            ),
        )
        .await;

        let result = ConsensusResult::new(
            topic,
            consensus_reached,
            winning_position,
            consensus_score,
            rounds,
            self.config.consensus_mechanism,
            duration_ms,
        )
        .with_votes(final_votes)
        .with_dissent(dissent);

        Ok(result)
    }

    /// Conducts a single round of debate.
    async fn conduct_round(
        &self,
        round_number: u32,
        context: &DebateContext,
        previous_positions: &str,
        event_tx: &mpsc::Sender<DebateEvent>,
    ) -> AgentResult<DebateRound> {
        let mut positions = Vec::new();
        let user_prompt = context.build_prompt(previous_positions);

        for role in &self.config.participating_roles {
            let position = self
                .get_agent_position(*role, &user_prompt, context.topic)
                .await?;

            // Emit position received event
            let claim_summary = if position.claim.len() > 100 {
                format!("{}...", &position.claim[..100])
            } else {
                position.claim.clone()
            };

            self.send_event(
                event_tx,
                DebateEvent::agent_position_received(*role, claim_summary, position.confidence),
            )
            .await;

            positions.push(position);
        }

        Ok(DebateRound::new(round_number, positions))
    }

    /// Gets a position from a single agent.
    async fn get_agent_position(
        &self,
        role: DebateAgentRole,
        user_prompt: &str,
        _topic: DebateTopic,
    ) -> AgentResult<AgentPosition> {
        let system_prompt = role.system_prompt();

        let request = GenerationRequest::new(
            "default",
            vec![
                Message::system(system_prompt),
                Message::user(user_prompt),
            ],
        )
        .with_temperature(self.config.temperature)
        .with_max_tokens(self.config.max_tokens);

        let response = self
            .llm_client
            .generate(request)
            .await
            .map_err(AgentError::from)?;

        let content = response
            .first_content()
            .ok_or_else(|| AgentError::ResponseParseError("Empty response from LLM".to_string()))?;

        self.parse_debate_response(role, content)
    }

    /// Parses a debate response from the LLM.
    fn parse_debate_response(
        &self,
        role: DebateAgentRole,
        content: &str,
    ) -> AgentResult<AgentPosition> {
        // Try to extract JSON from the response
        let json_content = Self::extract_json(content);

        let debate_response: DebateResponse =
            serde_json::from_str(&json_content).map_err(|e| {
                AgentError::ResponseParseError(format!(
                    "Failed to parse debate response: {}. Content: {}",
                    e,
                    &json_content[..json_content.len().min(200)]
                ))
            })?;

        Ok(AgentPosition::new(
            role,
            debate_response.claim,
            debate_response.conclusion,
            debate_response.confidence,
        )
        .with_evidence(debate_response.evidence)
        .with_weaknesses(debate_response.acknowledged_weaknesses))
    }

    /// Extracts JSON from a potentially markdown-wrapped response.
    fn extract_json(content: &str) -> String {
        // Try to find JSON in markdown code blocks
        if let Some(start) = content.find("```json") {
            if let Some(end) = content[start..].find("```\n") {
                let json_start = start + 7; // Skip "```json"
                return content[json_start..start + end].trim().to_string();
            }
            if let Some(end) = content[start..].rfind("```") {
                let json_start = start + 7;
                if end > 7 {
                    return content[json_start..start + end].trim().to_string();
                }
            }
        }

        // Try to find raw JSON (starts with {)
        if let Some(start) = content.find('{') {
            if let Some(end) = content.rfind('}') {
                if end >= start {
                    return content[start..=end].to_string();
                }
            }
        }

        // Return as-is and let JSON parser handle errors
        content.to_string()
    }

    /// Calculates consensus score for a set of positions.
    fn calculate_consensus_score(&self, positions: &[AgentPosition], topic: &DebateTopic) -> f64 {
        if positions.is_empty() {
            return 0.0;
        }

        // Group positions by their conclusion (simplified clustering)
        let mut conclusion_weights: HashMap<String, f64> = HashMap::new();
        let mut total_weight = 0.0;

        for pos in positions {
            let key = Self::normalize_conclusion(&pos.conclusion);
            let weight = if self.config.use_weighted_voting {
                pos.weighted_score(topic)
            } else {
                1.0
            };
            *conclusion_weights.entry(key).or_default() += weight;
            total_weight += weight;
        }

        // Find the maximum agreement
        let max_weight = conclusion_weights.values().cloned().fold(0.0, f64::max);

        if total_weight > 0.0 {
            max_weight / total_weight
        } else {
            0.0
        }
    }

    /// Normalizes a conclusion for comparison.
    fn normalize_conclusion(conclusion: &str) -> String {
        // Simple normalization: lowercase, trim, take first 50 chars
        let normalized = conclusion.to_lowercase().trim().to_string();
        if normalized.len() > 50 {
            normalized[..50].to_string()
        } else {
            normalized
        }
    }

    /// Determines the final consensus from positions.
    #[allow(clippy::type_complexity)]
    fn determine_consensus(
        &self,
        positions: &[AgentPosition],
        topic: &DebateTopic,
    ) -> (
        bool,
        Option<String>,
        f64,
        HashMap<String, String>,
        Vec<DissentingOpinion>,
    ) {
        if positions.is_empty() {
            return (false, None, 0.0, HashMap::new(), Vec::new());
        }

        // Calculate weighted votes for each unique position
        let mut position_votes: HashMap<String, (f64, String)> = HashMap::new();
        let mut total_weight = 0.0;

        for pos in positions {
            let key = Self::normalize_conclusion(&pos.conclusion);
            let weight = if self.config.use_weighted_voting {
                pos.weighted_score(topic)
            } else {
                1.0
            };

            let entry = position_votes
                .entry(key)
                .or_insert((0.0, pos.conclusion.clone()));
            entry.0 += weight;
            total_weight += weight;
        }

        // Find winning position
        let (winning_key, (winning_weight, winning_conclusion)) = position_votes
            .iter()
            .max_by(|a, b| a.1 .0.partial_cmp(&b.1 .0).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(k, v)| (k.clone(), (v.0, v.1.clone())))
            .unwrap_or_default();

        let consensus_score = if total_weight > 0.0 {
            winning_weight / total_weight
        } else {
            0.0
        };

        // Check if threshold is met
        let threshold = match self.config.consensus_mechanism {
            ConsensusMechanism::SimpleMajority => 0.5,
            ConsensusMechanism::Supermajority => 0.67,
            ConsensusMechanism::Unanimous => 1.0,
            ConsensusMechanism::WeightedVoting => self.config.consensus_threshold,
        };

        let consensus_reached = consensus_score >= threshold;

        // Build final votes map
        let final_votes: HashMap<String, String> = positions
            .iter()
            .map(|p| (p.role.display_name().to_string(), p.conclusion.clone()))
            .collect();

        // Identify dissenting opinions
        let dissent: Vec<DissentingOpinion> = positions
            .iter()
            .filter(|p| Self::normalize_conclusion(&p.conclusion) != winning_key)
            .map(|p| DissentingOpinion {
                role: p.role,
                position: p.conclusion.clone(),
                reasoning: p.evidence.join("; "),
            })
            .collect();

        (
            consensus_reached,
            if consensus_reached {
                Some(winning_conclusion)
            } else {
                None
            },
            consensus_score,
            final_votes,
            dissent,
        )
    }

    /// Sends an event through the channel.
    async fn send_event(&self, event_tx: &mpsc::Sender<DebateEvent>, event: DebateEvent) {
        let _ = event_tx.send(event).await;
    }
}

// ============================================================================
// Builder Pattern
// ============================================================================

/// Builder for creating a DebateOrchestrator with fluent API.
pub struct DebateOrchestratorBuilder {
    llm_client: Option<Arc<dyn LlmProvider>>,
    config: DebateOrchestratorConfig,
}

impl DebateOrchestratorBuilder {
    /// Creates a new builder with default configuration.
    pub fn new() -> Self {
        Self {
            llm_client: None,
            config: DebateOrchestratorConfig::default(),
        }
    }

    /// Sets the LLM client.
    pub fn llm_client(mut self, client: Arc<dyn LlmProvider>) -> Self {
        self.llm_client = Some(client);
        self
    }

    /// Sets the number of debate rounds.
    pub fn debate_rounds(mut self, rounds: u32) -> Self {
        self.config.debate_rounds = rounds.max(1);
        self
    }

    /// Sets the consensus threshold.
    pub fn consensus_threshold(mut self, threshold: f64) -> Self {
        self.config.consensus_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Sets participating roles.
    pub fn participating_roles(mut self, roles: Vec<DebateAgentRole>) -> Self {
        self.config.participating_roles = roles;
        self
    }

    /// Sets the consensus mechanism.
    pub fn consensus_mechanism(mut self, mechanism: ConsensusMechanism) -> Self {
        self.config.consensus_mechanism = mechanism;
        self
    }

    /// Enables or disables weighted voting.
    pub fn weighted_voting(mut self, enabled: bool) -> Self {
        self.config.use_weighted_voting = enabled;
        self
    }

    /// Sets the LLM temperature.
    pub fn temperature(mut self, temperature: f64) -> Self {
        self.config.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// Builds the DebateOrchestrator.
    pub fn build(self) -> AgentResult<DebateOrchestrator> {
        let llm_client = self
            .llm_client
            .ok_or_else(|| AgentError::ConfigurationError("LLM client is required".to_string()))?;

        Ok(DebateOrchestrator::new(llm_client, self.config))
    }
}

impl Default for DebateOrchestratorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{Choice, GenerationResponse, Usage};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// Mock LLM provider for testing.
    struct MockLlmProvider {
        responses: Mutex<Vec<String>>,
        call_count: AtomicUsize,
    }

    impl MockLlmProvider {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: Mutex::new(responses),
                call_count: AtomicUsize::new(0),
            }
        }

        fn single_response(response: String) -> Self {
            Self::new(vec![response])
        }
    }

    #[async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn generate(
            &self,
            _request: GenerationRequest,
        ) -> Result<GenerationResponse, crate::error::LlmError> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            let responses = self.responses.lock().expect("lock not poisoned");
            let content = responses
                .get(idx)
                .cloned()
                .unwrap_or_else(|| responses.last().cloned().unwrap_or_default());

            Ok(GenerationResponse {
                id: format!("mock-{}", idx),
                model: "mock-model".to_string(),
                choices: vec![Choice {
                    index: 0,
                    message: Message::assistant(content),
                    finish_reason: "stop".to_string(),
                }],
                usage: Usage {
                    prompt_tokens: 100,
                    completion_tokens: 200,
                    total_tokens: 300,
                },
            })
        }
    }

    fn mock_debate_response(claim: &str, confidence: f64) -> String {
        format!(
            r#"{{
            "claim": "{}",
            "evidence": ["Evidence 1", "Evidence 2"],
            "conclusion": "We should proceed with this approach",
            "confidence": {},
            "acknowledged_weaknesses": ["Potential risk"],
            "responses_to_others": []
        }}"#,
            claim, confidence
        )
    }

    #[test]
    fn test_config_defaults() {
        let config = DebateOrchestratorConfig::default();
        assert_eq!(config.debate_rounds, 3);
        assert!((config.consensus_threshold - 0.6).abs() < 0.01);
        assert_eq!(config.participating_roles.len(), 5);
    }

    #[test]
    fn test_config_builder() {
        let config = DebateOrchestratorConfig::new()
            .with_debate_rounds(5)
            .with_consensus_threshold(0.8)
            .with_consensus_mechanism(ConsensusMechanism::Supermajority);

        assert_eq!(config.debate_rounds, 5);
        assert!((config.consensus_threshold - 0.8).abs() < 0.01);
        assert_eq!(config.consensus_mechanism, ConsensusMechanism::Supermajority);
    }

    #[test]
    fn test_consensus_mechanism_thresholds() {
        assert!((ConsensusMechanism::SimpleMajority.required_threshold() - 0.5).abs() < 0.01);
        assert!((ConsensusMechanism::Supermajority.required_threshold() - 0.67).abs() < 0.01);
        assert!((ConsensusMechanism::Unanimous.required_threshold() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_debate_round_creation() {
        let positions = vec![
            AgentPosition::new(DebateAgentRole::Innovator, "Claim 1", "Conclusion 1", 0.8),
            AgentPosition::new(DebateAgentRole::Pragmatist, "Claim 2", "Conclusion 2", 0.7),
        ];

        let round = DebateRound::new(1, positions);
        assert_eq!(round.round_number, 1);
        assert_eq!(round.positions.len(), 2);
    }

    #[test]
    fn test_debate_context_build_prompt() {
        let context = DebateContext::new(DebateTopic::ProjectType, "Build a web API")
            .with_param("language", "Rust")
            .with_param("category", "web-api");

        let prompt = context.build_prompt("Previous: Agent A said X");
        assert!(prompt.contains("Build a web API"));
        assert!(prompt.contains("Rust"));
        assert!(prompt.contains("Previous: Agent A said X"));
    }

    #[test]
    fn test_orchestrator_builder_missing_llm() {
        let result = DebateOrchestratorBuilder::new()
            .debate_rounds(3)
            .build();

        assert!(result.is_err());
        match result {
            Err(AgentError::ConfigurationError(msg)) => {
                assert!(msg.contains("LLM client"));
            }
            _ => panic!("Expected ConfigurationError"),
        }
    }

    #[test]
    fn test_extract_json() {
        // Raw JSON
        let raw = r#"{"claim": "test"}"#;
        assert_eq!(DebateOrchestrator::extract_json(raw), r#"{"claim": "test"}"#);

        // Markdown wrapped
        let markdown = "```json\n{\"claim\": \"test\"}\n```";
        let extracted = DebateOrchestrator::extract_json(markdown);
        assert!(extracted.contains("claim"));

        // With surrounding text
        let with_text = "Here is my response: {\"claim\": \"test\"} end";
        assert_eq!(
            DebateOrchestrator::extract_json(with_text),
            r#"{"claim": "test"}"#
        );
    }

    #[tokio::test]
    async fn test_parse_debate_response() {
        let mock_llm = Arc::new(MockLlmProvider::single_response("".to_string()));
        let orchestrator = DebateOrchestrator::with_defaults(mock_llm);

        let json = r#"{
            "claim": "Test claim",
            "evidence": ["E1", "E2"],
            "conclusion": "Test conclusion",
            "confidence": 0.85,
            "acknowledged_weaknesses": ["W1"],
            "responses_to_others": []
        }"#;

        let position = orchestrator
            .parse_debate_response(DebateAgentRole::Innovator, json)
            .expect("should parse successfully");

        assert_eq!(position.role, DebateAgentRole::Innovator);
        assert_eq!(position.claim, "Test claim");
        assert_eq!(position.conclusion, "Test conclusion");
        assert!((position.confidence - 0.85).abs() < 0.01);
        assert_eq!(position.evidence.len(), 2);
    }

    #[test]
    fn test_consensus_score_calculation() {
        let mock_llm = Arc::new(MockLlmProvider::single_response("".to_string()));
        let config = DebateOrchestratorConfig::default().with_weighted_voting(false);
        let orchestrator = DebateOrchestrator::new(mock_llm, config);

        // All agree
        let positions = vec![
            AgentPosition::new(DebateAgentRole::Innovator, "C", "Same conclusion", 0.8),
            AgentPosition::new(DebateAgentRole::Pragmatist, "C", "Same conclusion", 0.8),
            AgentPosition::new(DebateAgentRole::Critic, "C", "Same conclusion", 0.8),
        ];
        let score = orchestrator.calculate_consensus_score(&positions, &DebateTopic::ProjectType);
        assert!((score - 1.0).abs() < 0.01);

        // Split 2-1
        let positions_split = vec![
            AgentPosition::new(DebateAgentRole::Innovator, "C", "Position A", 0.8),
            AgentPosition::new(DebateAgentRole::Pragmatist, "C", "Position A", 0.8),
            AgentPosition::new(DebateAgentRole::Critic, "C", "Position B", 0.8),
        ];
        let score_split =
            orchestrator.calculate_consensus_score(&positions_split, &DebateTopic::ProjectType);
        assert!((score_split - 0.67).abs() < 0.1);
    }

    #[tokio::test]
    async fn test_full_debate() {
        // Create responses for 5 agents x 3 rounds = 15 calls
        let mut responses = Vec::new();
        for _ in 0..15 {
            responses.push(mock_debate_response("Innovative approach", 0.85));
        }

        let mock_llm = Arc::new(MockLlmProvider::new(responses));
        let orchestrator = DebateOrchestrator::builder()
            .llm_client(mock_llm)
            .debate_rounds(1) // Just 1 round for speed
            .consensus_threshold(0.6)
            .build()
            .expect("should build");

        let context = DebateContext::new(DebateTopic::ProjectType, "Build a CLI tool");
        let (event_tx, mut event_rx) = mpsc::channel(100);

        let result = orchestrator
            .conduct_debate(context, event_tx)
            .await
            .expect("debate should complete");

        assert_eq!(result.topic, DebateTopic::ProjectType);
        assert!(!result.rounds.is_empty());
        assert!(result.consensus_score > 0.0);

        // Verify events were emitted
        event_rx.close();
        let mut events = Vec::new();
        while let Some(event) = event_rx.recv().await {
            events.push(event);
        }
        assert!(!events.is_empty());
    }

    #[test]
    fn test_debate_event_constructors() {
        let started = DebateEvent::debate_started(DebateTopic::Difficulty, 5, 3);
        match started {
            DebateEvent::DebateStarted {
                topic,
                participant_count,
                planned_rounds,
                ..
            } => {
                assert_eq!(topic, DebateTopic::Difficulty);
                assert_eq!(participant_count, 5);
                assert_eq!(planned_rounds, 3);
            }
            _ => panic!("Expected DebateStarted event"),
        }

        let completed = DebateEvent::debate_completed(true, Some("Winner".to_string()), 0.8, 1000);
        match completed {
            DebateEvent::DebateCompleted {
                consensus_reached,
                winning_position,
                consensus_score,
                duration_ms,
                ..
            } => {
                assert!(consensus_reached);
                assert_eq!(winning_position, Some("Winner".to_string()));
                assert!((consensus_score - 0.8).abs() < 0.01);
                assert_eq!(duration_ms, 1000);
            }
            _ => panic!("Expected DebateCompleted event"),
        }
    }
}
