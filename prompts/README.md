# LLM Agent Testing Prompts

A comprehensive hierarchical collection of prompt templates for testing LLM agents on computer science tasks. These prompts are designed to be challenging, incorporating traps and edge cases that test the agent's reasoning capabilities.

## Overview

This prompt library follows a format inspired by SWE-bench_Pro and terminal-bench, combining structured problem statements with difficulty estimation and LLM-specific trap configurations.

## Directory Structure

```
prompts/
├── algorithms/           # Algorithm design and implementation
│   ├── sorting/          # Sorting algorithms and comparators
│   ├── graph/            # Graph algorithms (MST, shortest path, traversal)
│   ├── dynamic-programming/  # DP and memoization
│   ├── string/           # String processing and encoding
│   └── search/           # Search algorithms
├── systems/              # Systems programming
│   ├── operating-systems/    # Process management, file systems
│   ├── distributed-systems/  # Consensus, partitioning, clocks
│   ├── memory-management/    # Memory allocation, fragmentation
│   └── concurrency/          # Threading, locking, synchronization
├── security/             # Security vulnerabilities and hardening
│   ├── cryptography/     # Encryption, timing attacks, randomness
│   ├── vulnerabilities/  # SQL injection, XSS, deserialization
│   ├── authentication/   # JWT, sessions, credentials
│   └── exploitation/     # Buffer overflows, ROP
├── databases/            # Database systems
│   ├── sql/              # SQL queries, transactions, migrations
│   ├── nosql/            # Consistency, sharding
│   ├── optimization/     # Indexing, query plans
│   └── transactions/     # Isolation levels, deadlocks
├── networking/           # Network programming
│   ├── protocols/        # Protocol implementation
│   ├── sockets/          # Socket programming
│   └── troubleshooting/  # Network debugging
├── devops/               # DevOps and infrastructure
│   ├── containers/       # Docker, Kubernetes security
│   ├── ci-cd/            # Pipeline security, dependencies
│   ├── monitoring/       # Logging, observability
│   └── infrastructure/   # Terraform, DNS
├── web/                  # Web development
│   ├── frontend/         # Client-side development
│   ├── backend/          # Server-side development
│   └── apis/             # API design and security
├── machine-learning/     # ML/AI tasks
│   ├── neural-networks/  # Deep learning
│   ├── nlp/              # Natural language processing
│   └── computer-vision/  # Image processing
└── traps/                # LLM-specific traps
    ├── data-corruption/  # Files that corrupt on wrong access
    ├── state-dependent/  # Order-dependent operations
    ├── timing-attacks/   # Race conditions, TOCTOU
    └── deceptive-structures/  # Symlinks, Unicode tricks
```

## Prompt Template Format

Each prompt is a YAML file with the following structure:

```yaml
id: "unique-task-id"
version: "1.0.0"
category: "main-category"
subcategory: "sub-category"

# SWE-bench_Pro style fields
problem_statement: |
  Detailed description of the problem...

requirements: |
  - Requirement 1
  - Requirement 2

interface: |
  Input/output specification...

# terminal-bench style fields
difficulty:
  estimated: "hard"  # easy, medium, hard
  time_range: [300, 1800]  # seconds
  command_steps: [5, 20]

# LLM trap configurations
traps:
  - type: "trap_type"
    description: "What the trap does"
    trigger: "What causes it to activate"

# Task generation template
instruction_template: |
  You are given a {{ scenario_type }} that needs to be {{ action }}.
  ...

# Reference solution (hidden from agent)
reference_solution: |
  # Solution code...

# Test cases
fail_to_pass:
  - "test_main_functionality"

pass_to_pass:
  - "test_basic_setup"

# Variables for task generation
variables:
  - name: variable_name
    type: string
    options: ["option1", "option2"]

# Anti-hardcoding measures
anti_hardcoding:
  canary_tokens: true
  randomize_paths: true
  dynamic_content: true
```

## Key Features

### 1. Trap Mechanisms

Each prompt includes specific traps designed to test LLM weaknesses:

- **Data Corruption Traps**: Files that corrupt when opened incorrectly
- **State-Dependent Traps**: Operations that must be performed in specific order
- **Timing Traps**: Race conditions and TOCTOU vulnerabilities
- **Deceptive Structures**: Symlinks, Unicode tricks, homoglyphs

### 2. Difficulty Levels

- **Easy**: Single-step tasks with clear solutions
- **Medium**: Multi-step tasks requiring careful reasoning
- **Hard**: Complex tasks with hidden pitfalls and edge cases

### 3. Anti-Hardcoding Measures

To prevent LLMs from memorizing solutions:
- Canary tokens in file content
- Randomized file paths and names
- Dynamic variable injection
- Multiple valid solution paths

## Usage

### Generating a Task

```python
import yaml
import random

def load_prompt(path):
    with open(path) as f:
        return yaml.safe_load(f)

def generate_task(prompt):
    # Substitute variables
    instruction = prompt['instruction_template']
    for var in prompt['variables']:
        if var['type'] == 'string' and 'options' in var:
            value = random.choice(var['options'])
        elif var['type'] == 'int':
            value = random.randint(var['min'], var['max'])
        instruction = instruction.replace(f"{{{{ {var['name']} }}}}", str(value))
    return instruction
```

### Evaluating Solutions

Solutions should be evaluated against:
1. `fail_to_pass` tests - must pass after fix
2. `pass_to_pass` tests - must not break existing functionality
3. No trap activation - solution handles all edge cases

## Categories

See [CATEGORIES.md](CATEGORIES.md) for a detailed description of each category.

## Contributing

When adding new prompts:

1. Follow the YAML template format exactly
2. Include meaningful trap mechanisms
3. Provide complete reference solutions (no placeholders)
4. Add comprehensive test cases
5. Document all variables and their valid ranges

## License

These prompts are provided for research and evaluation purposes.
