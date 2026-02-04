# Dataforge Usage Guide

This guide provides detailed instructions for using Dataforge, including TUI controls, JSON output format, template creation, and configuration options.

## Table of Contents

- [TUI Mode](#tui-mode)
- [JSON Output Format](#json-output-format)
- [Template Creation](#template-creation)
- [Difficulty Levels](#difficulty-levels)
- [Configuration Options](#configuration-options)

## TUI Mode

The interactive TUI provides real-time visualization of the multi-agent validation pipeline.

### Launching the TUI

```bash
dataforge tui
```

### Controls

| Key | Action |
|-----|--------|
| `Up` / `k` | Move selection up |
| `Down` / `j` | Move selection down |
| `Space` | Start task generation/validation |
| `Tab` | Switch focus between panels |
| `Shift+Tab` | Switch focus (reverse) |
| `Page Up` | Scroll reasoning panel up |
| `Page Down` | Scroll reasoning panel down |
| `q` / `Esc` | Quit application |

### Interface Layout

The TUI is divided into three main panels:

```
+------------------------------------------------------------------+
|                           dataforge                              |
+---------------------+--------------------------------------------+
|                     |                                            |
|    Difficulty       |           Pipeline Progress                |
|    -----------      |           -----------------                |
|    > Easy           |  [ ] Task Generation                       |
|      Medium         |  [ ] Difficulty Validation                 |
|      Hard           |  [ ] Feasibility Validation                |
|                     |  [ ] Final Approval                        |
|                     |                                            |
+---------------------+--------------------------------------------+
|                        Agent Reasoning                           |
|  ---------------------------------------------------------------  |
|  [12:34:56] Starting Task Generation...                          |
|  [12:34:57] Task generator: Created task-abc123                  |
|  [12:34:58] Starting Difficulty Validation...                    |
|                                                                  |
+------------------------------------------------------------------+
| Status: Ready - Press <Space> to start generation                |
+------------------------------------------------------------------+
```

**Panels:**

1. **Difficulty Selection** (Left): Select task difficulty level
2. **Pipeline Progress** (Right): Shows validation stages with status indicators
   - `[ ]` Pending
   - `[~]` Running
   - `[x]` Completed
   - `[!]` Failed
3. **Agent Reasoning** (Bottom): Real-time log of agent activity and reasoning

### Status Indicators

The status bar at the bottom shows:
- Current pipeline stage when running
- Final result (approved/rejected) when complete
- Instructions when idle

## JSON Output Format

Use `--json` flag to output structured JSON instead of the TUI:

```bash
dataforge tui --json --difficulty medium --seed 42
```

### Output Schema

```json
{
  "status": "approved",
  "difficulty": "medium",
  "seed": 12345,
  "task_id": "task-abc12345",
  "validations": [
    {
      "agent": "task_generator",
      "status": "completed",
      "timestamp": "2024-01-15T10:30:00Z",
      "score": 1.0,
      "reasoning": "Successfully generated task from template"
    },
    {
      "agent": "difficulty_validator",
      "status": "completed",
      "timestamp": "2024-01-15T10:30:02Z",
      "score": 0.85,
      "reasoning": "Task matches medium difficulty expectations"
    },
    {
      "agent": "feasibility_validator",
      "status": "completed",
      "timestamp": "2024-01-15T10:30:04Z",
      "score": 0.9,
      "reasoning": "Task is solvable and appropriately challenging"
    }
  ],
  "final_score": 0.92,
  "reasoning": "Task approved with high confidence"
}
```

### Field Descriptions

| Field | Type | Description |
|-------|------|-------------|
| `status` | string | "approved" or "rejected" |
| `difficulty` | string | Difficulty level: easy, medium, hard |
| `seed` | integer | Random seed used for generation |
| `task_id` | string | Unique identifier for the generated task |
| `validations` | array | List of agent validation results |
| `validations[].agent` | string | Name of the validating agent |
| `validations[].status` | string | "completed" or "failed" |
| `validations[].timestamp` | string | ISO 8601 timestamp |
| `validations[].score` | float | Score from 0.0 to 1.0 (optional) |
| `validations[].reasoning` | string | Agent's reasoning (optional) |
| `final_score` | float | Aggregated score from all validators |
| `reasoning` | string | Summary reasoning for the decision |

### Using JSON Output in Scripts

```bash
#!/bin/bash

# Generate task and parse result
result=$(dataforge tui --json --difficulty hard --seed $RANDOM)

# Check if approved
status=$(echo "$result" | jq -r '.status')
if [ "$status" = "approved" ]; then
    task_id=$(echo "$result" | jq -r '.task_id')
    echo "Task approved: $task_id"
else
    echo "Task rejected"
    echo "$result" | jq '.reasoning'
fi
```

## Template Creation

Templates define the structure and parameters for task generation.

### Template Structure

```yaml
# Template metadata
id: "log-analysis-001"
version: "1.0.0"
category: "debugging"
subcategory: "log-analysis"

# Difficulty configuration
difficulty:
  estimated: "medium"           # Expected difficulty: easy, medium, hard
  time_range: [120, 600]        # Expected completion time in seconds [min, max]
  command_steps: [3, 8]         # Expected number of commands [min, max]
  target_success_rate: 0.70     # Expected pass rate (0.0 to 1.0)

# Task instruction using Tera templating
instruction_template: |
  A log file is located at {{ log_path }}.
  
  Your task is to:
  1. Find all entries with log level "ERROR"
  2. Count the total number of errors
  3. Identify the most frequent error code
  
  Expected output format:
  Total errors: <count>
  Most common: <error_code>

# Reference solution (for validation and scoring)
reference_solution: |
  #!/bin/bash
  total=$(grep -c 'ERROR' {{ log_path }})
  common=$(grep 'ERROR' {{ log_path }} | awk '{print $NF}' | sort | uniq -c | sort -rn | head -1 | awk '{print $2}')
  echo "Total errors: $total"
  echo "Most common: $common"

# Variable definitions
variables:
  - name: log_path
    type: string
    generator: file_path
    options:
      directory: "/var/log"
      pattern: "app-*.log"
      
  - name: error_count
    type: integer
    generator: range
    options:
      min: 10
      max: 1000
      
  - name: error_codes
    type: array
    generator: sample
    options:
      choices: ["E001", "E002", "E003", "E004", "E005"]
      count: 3

# Files to generate for the task environment
files:
  - path: "{{ log_path }}"
    template: |
      {% for i in range(end=error_count) %}
      {{ timestamp }} [{{ log_level }}] {{ error_codes | random }}: {{ message }}
      {% endfor %}

# Expected outputs for verification
expected_outputs:
  - type: stdout
    pattern: "Total errors: \\d+"
  - type: file
    path: "/tmp/result.txt"
    contains: ["error", "count"]
```

### Variable Types

| Type | Description | Generator Options |
|------|-------------|-------------------|
| `string` | Text value | `file_path`, `username`, `hostname`, `uuid` |
| `integer` | Numeric value | `range` with min/max |
| `float` | Decimal value | `range` with min/max, precision |
| `boolean` | True/false | `probability` |
| `array` | List of values | `sample` with choices and count |
| `timestamp` | Date/time value | `range` with start/end dates |

### Generator Examples

**File Path Generator:**
```yaml
- name: log_file
  type: string
  generator: file_path
  options:
    directory: "/var/log"
    pattern: "*.log"
    create: true
```

**Range Generator:**
```yaml
- name: port
  type: integer
  generator: range
  options:
    min: 1024
    max: 65535
```

**Sample Generator:**
```yaml
- name: services
  type: array
  generator: sample
  options:
    choices: ["nginx", "apache", "mysql", "postgres", "redis"]
    count: 3
    unique: true
```

### Creating a New Template

1. **Initialize scaffold:**
   ```bash
   dataforge init --id my-task --category debugging --output ./templates
   ```

2. **Edit the generated YAML file** to define your task

3. **Validate the template:**
   ```bash
   dataforge validate --path ./templates/my-task.yaml --validate-type template
   ```

4. **Test task generation:**
   ```bash
   dataforge generate --template ./templates/my-task.yaml --seed 42 --output ./test-output
   ```

## Difficulty Levels

Dataforge uses three calibrated difficulty levels:

### Easy

- **Command Steps:** 1-3
- **Expected Time:** 30 seconds to 2 minutes
- **Target Success Rate:** 90%
- **Base Points:** 10
- **Characteristics:**
  - Single-step tasks requiring basic commands
  - Straightforward instructions
  - Minimal domain knowledge required
  - Examples: list files, search for patterns, basic file operations

**Resource Limits:**
| Resource | Limit |
|----------|-------|
| CPU | 1 core |
| Memory | 256 MB |
| Storage | 1 GB |
| PIDs | 100 |
| Network | Internal only |

### Medium

- **Command Steps:** 3-8
- **Expected Time:** 2 to 10 minutes
- **Target Success Rate:** 70%
- **Base Points:** 25
- **Characteristics:**
  - Multi-step tasks requiring command chaining
  - Moderate complexity and domain knowledge
  - May require some troubleshooting
  - Examples: log analysis, configuration changes, data processing

**Resource Limits:**
| Resource | Limit |
|----------|-------|
| CPU | 2 cores |
| Memory | 512 MB |
| Storage | 5 GB |
| PIDs | 256 |
| Network | Internal only |

### Hard

- **Command Steps:** 8-20
- **Expected Time:** 10 to 30 minutes
- **Target Success Rate:** 40%
- **Base Points:** 50
- **Characteristics:**
  - Complex tasks requiring advanced knowledge
  - Multi-step problem solving and debugging
  - May require iteration and analysis
  - Examples: performance debugging, security analysis, system recovery

**Resource Limits:**
| Resource | Limit |
|----------|-------|
| CPU | 4 cores |
| Memory | 1 GB |
| Storage | 10 GB |
| PIDs | 512 |
| Network | External allowed |

### Difficulty Score Calculation

Task difficulty is calculated using weighted factors:

```
difficulty_score = 0.4 * time_factor + 0.4 * success_factor + 0.2 * hints_factor
```

Where:
- `time_factor` = normalized completion time (0-1)
- `success_factor` = 1 - success_rate (lower success = higher difficulty)
- `hints_factor` = normalized hints used (0-1)

Score ranges:
- **Easy:** 0.0 - 0.33
- **Medium:** 0.34 - 0.66
- **Hard:** 0.67 - 1.0

## Configuration Options

### Orchestrator Configuration

The orchestrator controls the validation pipeline:

```rust
OrchestratorConfig {
    // Minimum overall score to approve a task
    final_approval_threshold: 0.7,
    
    // Continue pipeline even if a stage fails
    continue_on_failure: false,
}
```

### Difficulty Validator Configuration

```rust
DifficultyValidatorConfig {
    // Minimum score to pass validation
    pass_threshold: 0.7,
    
    // LLM temperature (lower = more deterministic)
    temperature: 0.3,
    
    // Maximum tokens for LLM response
    max_tokens: 1000,
}
```

### Feasibility Validator Configuration

```rust
FeasibilityValidatorConfig {
    // Minimum score to pass validation
    pass_threshold: 0.7,
    
    // Require task to be marked solvable
    require_solvable: true,
    
    // Require task to be non-trivial
    require_non_trivial: true,
    
    // LLM temperature
    temperature: 0.3,
    
    // Maximum tokens for LLM response
    max_tokens: 1200,
}
```

### Environment Configuration

Configure Dataforge behavior via environment variables:

```bash
# Required: LiteLLM endpoint
export LITELLM_API_BASE="http://localhost:4000"

# Optional: API authentication
export LITELLM_API_KEY="your-api-key"

# Optional: Logging level
export RUST_LOG="info"              # Default
export RUST_LOG="debug"             # Verbose
export RUST_LOG="dataforge=debug"   # Module-specific

# Optional: Template directory
export DATAFORGE_TEMPLATES="./my-templates"

# Optional: Output directory
export DATAFORGE_OUTPUT="./output"
```

### LiteLLM Integration

Dataforge uses LiteLLM as the LLM backend. Configure your LiteLLM proxy with supported models:

```yaml
# litellm_config.yaml
model_list:
  - model_name: gpt-4
    litellm_params:
      model: openai/gpt-4
      api_key: ${OPENAI_API_KEY}
      
  - model_name: claude-3
    litellm_params:
      model: anthropic/claude-3-sonnet
      api_key: ${ANTHROPIC_API_KEY}
```

Start the proxy:
```bash
litellm --config litellm_config.yaml --port 4000
```

Then configure Dataforge:
```bash
export LITELLM_API_BASE="http://localhost:4000"
dataforge tui
```
