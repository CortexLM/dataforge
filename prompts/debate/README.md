# Debate System Prompts

Prompt templates for the multi-agent debate orchestrator that refines workspace generation quality through structured argumentation.

## Overview

The debate system brings multiple AI personas together to evaluate and improve workspace ideas before implementation. Each persona represents a different perspective, ensuring comprehensive coverage of potential issues and improvements.

## Purpose

Debates serve to:
1. **Validate** project ideas for realism and feasibility
2. **Calibrate** difficulty levels accurately
3. **Improve** ideas through diverse perspectives
4. **Catch** issues before expensive code generation
5. **Ensure** diversity across the benchmark suite

## Directory Structure

```
debate/
├── README.md       # This file
├── personas.md     # The 5 debate agent personas
└── topics.md       # Debate topic templates
```

## The Five Personas

| Persona | Role | Focus |
|---------|------|-------|
| **Innovator** | Creative catalyst | Novel ideas, pushing boundaries |
| **Pragmatist** | Feasibility expert | Practical implementation concerns |
| **Critic** | Analytical reviewer | Finding flaws and weaknesses |
| **Advocate** | Idea supporter | Strengthening and enhancing |
| **Validator** | Correctness checker | Verification and accuracy |

## Debate Flow

```
┌─────────────┐
│   Topic     │
│  Presented  │
└──────┬──────┘
       │
       ▼
┌─────────────────────────────────────────────────┐
│                   ROUND 1                        │
│  Innovator → Pragmatist → Critic → Advocate     │
│                                                  │
│  Initial positions and perspectives              │
└─────────────────────┬───────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────┐
│                   ROUND 2                        │
│  All personas respond to Round 1                 │
│                                                  │
│  Rebuttals, clarifications, refinements          │
└─────────────────────┬───────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────┐
│                   ROUND 3                        │
│  Convergence round                               │
│                                                  │
│  Building consensus, final positions             │
└─────────────────────┬───────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────┐
│                 VALIDATOR                        │
│  Synthesizes final decision/recommendation       │
│                                                  │
│  Checks logical consistency, makes ruling        │
└─────────────────────┬───────────────────────────┘
                      │
                      ▼
┌─────────────┐
│   Final     │
│  Decision   │
└─────────────┘
```

## Debate Types

### 1. Project Selection Debate
- **Goal**: Decide if a project idea should proceed
- **Decision**: Accept, Reject, or Modify

### 2. Difficulty Calibration Debate
- **Goal**: Agree on appropriate difficulty level
- **Decision**: Easy, Medium, or Hard with justification

### 3. Improvement Debate
- **Goal**: Enhance an accepted project idea
- **Decision**: List of specific improvements

### 4. Feasibility Debate
- **Goal**: Assess if a project is technically realistic
- **Decision**: Feasible, Needs Changes, or Infeasible

## Integration with Pipeline

```
                    ┌─────────────────┐
                    │ WorkspaceIdea   │
                    │ torAgent        │
                    └────────┬────────┘
                             │
                             ▼
┌───────────────────────────────────────────────────┐
│                 DEBATE ORCHESTRATOR               │
│                                                   │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ │
│  │Innovator│ │Pragmatist│ │ Critic │ │Advocate │ │
│  └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘ │
│       │          │           │           │       │
│       └──────────┴─────┬─────┴───────────┘       │
│                        │                         │
│                        ▼                         │
│                 ┌─────────────┐                  │
│                 │  Validator  │                  │
│                 └──────┬──────┘                  │
└────────────────────────┼────────────────────────┘
                         │
                         ▼
              ┌──────────────────────┐
              │ Decision + Feedback  │
              └──────────┬───────────┘
                         │
            ┌────────────┼────────────┐
            │            │            │
            ▼            ▼            ▼
       ┌────────┐  ┌──────────┐  ┌────────┐
       │ Accept │  │  Modify  │  │ Reject │
       └───┬────┘  └────┬─────┘  └────────┘
           │            │
           │            ▼
           │     ┌─────────────┐
           │     │  Revise &   │
           │     │  Re-debate  │
           │     └──────┬──────┘
           │            │
           └────────────┴─────────────────────┐
                                              │
                                              ▼
                                    ┌─────────────────┐
                                    │ CodeGenerator   │
                                    │ Agent           │
                                    └─────────────────┘
```

## Configuration

### Debate Parameters

```yaml
debate_config:
  max_rounds: 3
  min_agreement_threshold: 0.6  # 60% agreement to proceed
  required_validators: 1        # Validator must agree
  timeout_per_round: 60         # seconds
  
  persona_weights:
    innovator: 1.0
    pragmatist: 1.2  # Slightly more weight on practical concerns
    critic: 1.0
    advocate: 0.8    # Less weight to prevent over-optimism
    validator: 1.5   # Final say matters most
```

### Topic-Specific Settings

Different debate types have different requirements:

| Topic | Required Rounds | Agreement Threshold |
|-------|-----------------|---------------------|
| Project Selection | 2 | 60% |
| Difficulty Calibration | 3 | 80% |
| Improvement | 2 | 50% |
| Feasibility | 2 | 70% |

## Quality Metrics

Debates are evaluated on:

1. **Diversity of Arguments**: All perspectives represented
2. **Constructiveness**: Arguments improve the idea
3. **Convergence**: Reaches clear decision
4. **Accuracy**: Decisions align with ground truth (when available)
5. **Efficiency**: Reasonable number of rounds

## See Also

- [personas.md](personas.md) - Detailed persona descriptions
- [topics.md](topics.md) - Topic templates for debates
- [../workspace-generation/](../workspace-generation/) - Pre-debate idea generation
- [../vulnerability-injection/](../vulnerability-injection/) - Post-debate implementation
