//! Scaffold system for agent execution.
//!
//! The scaffold provides tools to the LLM and manages the agent loop:
//! 1. Capture state
//! 2. Get LLM action
//! 3. Parse action (tool call)
//! 4. Execute tool
//! 5. Record observation
//! 6. Check termination
//!
//! # Example
//!
//! ```ignore
//! use synth_bench::scaffold::{AgentLoop, AgentConfig, ExecutionContext};
//! use synth_bench::llm::LiteLlmClient;
//!
//! let llm_client = Arc::new(LiteLlmClient::from_env()?);
//! let config = AgentConfig::default();
//! let mut agent = AgentLoop::new(llm_client, config);
//! let result = agent.run(&task, &context).await?;
//! ```

pub mod agent_loop;
pub mod bridge;
pub mod prompts;
pub mod swe_agent;
pub mod tools;

pub use agent_loop::{AgentConfig, AgentLoop, AgentResult, StepResult};
pub use bridge::{BridgeError, ProcessBridge};
pub use prompts::{build_system_prompt, build_tool_prompt, AGENT_SYSTEM_PROMPT};
pub use swe_agent::{
    get_swe_agent_version, is_swe_agent_available, Container, Scaffold, ScaffoldError,
    StepResult as SweAgentStepResult, SweAgentAction, SweAgentConfig, SweAgentError,
    SweAgentScaffold, SweAgentState, TaskSpec,
};
pub use tools::{
    BashTool, EditFileTool, ExecutionContext, ReadFileTool, SearchTool, Tool, ToolError,
    ToolRegistry, ToolResult, WriteFileTool,
};
