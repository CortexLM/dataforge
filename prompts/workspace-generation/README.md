# Workspace Generation Prompts

Prompt templates for the synthetic benchmark workspace generation system. This module generates realistic code projects with intentionally injected vulnerabilities for security training and evaluation.

## Overview

The workspace generation system creates complete, realistic code projects through a multi-agent pipeline:

1. **Ideation Phase**: Generate diverse, realistic project concepts
2. **Code Generation Phase**: Create actual working code with proper structure
3. **Vulnerability Injection Phase**: Subtly introduce security flaws
4. **Cleaning Phase**: Remove telltale signs of artificial generation

## Directory Structure

```
workspace-generation/
├── README.md           # This file
├── ideation.md         # Prompt template for project ideation
└── code-generation.md  # Guidelines for code generation
```

## Related Directories

- `../vulnerability-injection/` - Prompts for vulnerability injection patterns
- `../debate/` - Multi-agent debate system for quality refinement

## Pipeline Flow

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────────┐
│  WorkspaceIdea  │────▶│  CodeGenerator   │────▶│ VulnerabilityInject │
│  torAgent       │     │  Agent           │     │ orAgent             │
└─────────────────┘     └──────────────────┘     └─────────────────────┘
        │                                                   │
        │                                                   ▼
        │               ┌──────────────────┐     ┌─────────────────────┐
        └──────────────▶│   DebateOrch     │────▶│  CodeCleanerAgent   │
                        │   estrator       │     │                     │
                        └──────────────────┘     └─────────────────────┘
```

## Key Principles

### Realism
Generated workspaces must appear as authentic projects:
- Realistic commit history patterns
- Common project structures by language
- Appropriate dependencies for the tech stack
- Natural code style variations

### Difficulty Calibration
Each workspace targets a specific difficulty level:
- **Easy**: Obvious vulnerabilities, simple projects
- **Medium**: Subtle issues, moderate complexity
- **Hard**: Deep vulnerabilities, complex codebases

### Diversity
Workspaces span multiple dimensions:
- Programming languages (Python, JavaScript, Rust, Go, Java, etc.)
- Project types (CLI tools, web apps, libraries, services)
- Vulnerability categories (injection, auth, crypto, etc.)
- Code quality levels (clean to legacy)

## Usage

### Generating a New Workspace Idea

```python
from dataforge.agents import WorkspaceIdeatorAgent

ideator = WorkspaceIdeatorAgent()
idea = ideator.generate_idea(
    domain="web",
    language="python",
    difficulty="medium",
    vulnerability_type="authentication"
)
```

### Code Generation Parameters

| Parameter | Description | Example Values |
|-----------|-------------|----------------|
| `language` | Target programming language | python, javascript, rust, go |
| `project_type` | Type of project to generate | cli, web, library, service |
| `complexity` | Code complexity level | simple, moderate, complex |
| `file_count` | Target number of files | 5-50 |
| `has_tests` | Include test files | true, false |
| `has_docs` | Include documentation | true, false |

## Evaluation Criteria

Generated workspaces are evaluated on:

1. **Authenticity**: Does it look like real production code?
2. **Completeness**: Is the project functional and buildable?
3. **Vulnerability Quality**: Are flaws realistic and subtle?
4. **Difficulty Alignment**: Does it match the target difficulty?
5. **Diversity**: Does it add variety to the benchmark suite?

## Anti-Patterns to Avoid

❌ **DON'T** generate obviously fake variable names  
❌ **DON'T** include comments explaining vulnerabilities  
❌ **DON'T** use trivial placeholder implementations  
❌ **DON'T** create unrealistic project structures  
❌ **DON'T** mix incompatible dependencies  

## See Also

- [ideation.md](ideation.md) - Project ideation prompts
- [code-generation.md](code-generation.md) - Code generation guidelines
- [../vulnerability-injection/injection-guide.md](../vulnerability-injection/injection-guide.md) - Vulnerability patterns
- [../debate/README.md](../debate/README.md) - Quality refinement through debate
