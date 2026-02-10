# Debate Topic Templates

Templates for structuring debates on different aspects of workspace generation.

---

## Topic Categories

1. [Project Type Selection](#1-project-type-selection)
2. [Difficulty Calibration](#2-difficulty-calibration)
3. [Improvement Suggestions](#3-improvement-suggestions)
4. [Feasibility Validation](#4-feasibility-validation)

---

## 1. Project Type Selection

### Purpose
Decide whether a proposed project idea should be accepted, modified, or rejected.

### Input Format

```yaml
topic: project_selection
proposal:
  name: "expense-tracker"
  description: |
    Internal expense report submission system for small companies.
    Employees submit expenses with receipts, managers approve/reject.
  domain: web
  language: python
  framework: flask
  target_difficulty: medium
  target_vulnerabilities:
    - type: sql_injection
      location_hint: "report filtering"
    - type: idor
      location_hint: "viewing other users' reports"
  estimated_files: 18
```

### Debate Questions

Each persona should address:

1. **Realism**: Does this project represent something real developers build?
2. **Fit**: Is this project type appropriate for the target vulnerabilities?
3. **Difficulty Match**: Does the complexity support the target difficulty?
4. **Diversity**: Does this add unique value to the benchmark suite?
5. **Implementation**: Can this be generated with the proposed tech stack?

### Decision Criteria

| Criterion | Weight | Description |
|-----------|--------|-------------|
| Realism | 25% | Project would exist in real world |
| Vulnerability Fit | 25% | Vulnerabilities naturally fit the project |
| Difficulty Match | 20% | Complexity matches target difficulty |
| Diversity | 15% | Adds variety to benchmark |
| Implementability | 15% | Can be generated without issues |

### Decision Options

- **ACCEPT**: Proceed with generation
- **ACCEPT WITH MODIFICATIONS**: Proceed with specified changes
- **REVISE AND RESUBMIT**: Significant changes needed
- **REJECT**: Do not proceed (explain why)

### Template: Innovator Opening

```markdown
## INNOVATOR - Project Selection: {{project_name}}

### Initial Assessment
{{description_summary}}

### Enhancement Opportunities
1. **Feature Addition**: {{proposed_feature}}
   - Justification: {{why_this_adds_value}}
   
2. **Vulnerability Enhancement**: {{vulnerability_idea}}
   - Makes it more interesting because: {{reasoning}}

3. **Differentiation**: {{what_makes_unique}}
   - Compared to existing benchmarks: {{comparison}}

### Concerns to Address
- {{potential_issue_1}}
- {{potential_issue_2}}

### Position
[SUPPORT/SUPPORT WITH ENHANCEMENTS/NEEDS WORK]
```

### Template: Validator Synthesis

```markdown
## VALIDATOR - Final Decision: {{project_name}}

### Summary of Debate
| Persona | Position | Key Points |
|---------|----------|------------|
| Innovator | {{position}} | {{key_points}} |
| Pragmatist | {{position}} | {{key_points}} |
| Critic | {{position}} | {{key_points}} |
| Advocate | {{position}} | {{key_points}} |

### Evaluation Against Criteria
| Criterion | Score (1-5) | Notes |
|-----------|-------------|-------|
| Realism | {{score}} | {{notes}} |
| Vulnerability Fit | {{score}} | {{notes}} |
| Difficulty Match | {{score}} | {{notes}} |
| Diversity | {{score}} | {{notes}} |
| Implementability | {{score}} | {{notes}} |

**Weighted Score**: {{total}}/5.0

### Decision
**{{DECISION}}**

### Required Modifications (if applicable)
1. {{modification_1}}
2. {{modification_2}}

### Reasoning
{{detailed_reasoning}}
```

---

## 2. Difficulty Calibration

### Purpose
Reach consensus on the appropriate difficulty level for a project.

### Input Format

```yaml
topic: difficulty_calibration
project:
  name: "api-gateway"
  description: "API gateway with rate limiting and auth"
  language: go
  vulnerability_type: race_condition
  vulnerability_location: "rate limiter token bucket"
  proposed_difficulty: hard
  
context:
  file_count: 25
  code_complexity: moderate
  vulnerability_depth: 3 functions deep
  detection_by_static_analysis: unlikely
```

### Debate Questions

1. **Detection Time**: How long would a skilled reviewer take to find this?
2. **Fix Complexity**: How difficult is it to implement a correct fix?
3. **Prerequisites**: What knowledge is required to understand the vulnerability?
4. **Pattern Recognition**: Is this a commonly known vulnerability pattern?
5. **Static Analysis**: Would automated tools flag this?

### Difficulty Definitions

| Level | Detection Time | Fix Time | Prerequisites |
|-------|---------------|----------|---------------|
| Easy | <5 min | <15 min | Basic security knowledge |
| Medium | 5-20 min | 15-45 min | Moderate security experience |
| Hard | 20-60 min | 45+ min | Advanced security expertise |

### Template: Critic Analysis

```markdown
## CRITIC - Difficulty Assessment: {{project_name}}

### Proposed: {{proposed_difficulty}}
### My Assessment: {{my_assessment}}

### Detection Analysis
- **Visibility**: {{how_visible_is_vulnerability}}
- **Location**: {{where_in_codebase}}
- **Pattern**: {{is_it_common_pattern}}
- **Static Analysis**: {{would_tools_catch_it}}

### Fix Complexity
- **Understanding Required**: {{what_must_be_understood}}
- **Code Changes**: {{scope_of_fix}}
- **Testing**: {{how_to_verify_fix}}

### Benchmark Comparison
Similar vulnerabilities in existing benchmarks:
- {{similar_1}}: {{difficulty_1}}
- {{similar_2}}: {{difficulty_2}}

### Evidence for My Assessment
1. {{evidence_point_1}}
2. {{evidence_point_2}}
3. {{evidence_point_3}}

### Recommendation
{{specific_difficulty_recommendation}}
```

### Template: Validator Decision

```markdown
## VALIDATOR - Difficulty Ruling: {{project_name}}

### Positions Summary
| Persona | Proposed Difficulty | Confidence |
|---------|---------------------|------------|
| Innovator | {{difficulty}} | {{confidence}} |
| Pragmatist | {{difficulty}} | {{confidence}} |
| Critic | {{difficulty}} | {{confidence}} |
| Advocate | {{difficulty}} | {{confidence}} |

### Calibration Factors
| Factor | Assessment |
|--------|------------|
| Detection Time | {{estimate}} |
| Fix Complexity | {{assessment}} |
| Required Knowledge | {{level}} |
| Static Analysis Evasion | {{yes_no}} |

### Final Difficulty: **{{DIFFICULTY}}**

### Justification
{{detailed_reasoning}}

### Adjustments Made
- Original proposal: {{original}}
- Final decision: {{final}}
- Reason for change: {{reason}}
```

---

## 3. Improvement Suggestions

### Purpose
Brainstorm and evaluate improvements to an accepted project idea.

### Input Format

```yaml
topic: improvement_suggestions
project:
  name: "user-management-api"
  description: "RESTful API for user CRUD operations"
  language: javascript
  framework: express
  accepted_vulnerabilities:
    - sql_injection
    - broken_auth
  current_difficulty: medium
  current_file_count: 15

constraints:
  max_additional_files: 5
  max_difficulty_increase: 1 level
  preserve_core_functionality: true
```

### Debate Questions

1. **Value Addition**: What improvements would make this more valuable?
2. **Complexity Balance**: How do we improve without over-complicating?
3. **Vulnerability Enhancement**: Can we make the vulnerabilities more subtle?
4. **Realism Boost**: What would make this feel more like a real project?
5. **Educational Value**: What would make this better for learning?

### Improvement Categories

| Category | Description | Example |
|----------|-------------|---------|
| Feature Addition | Add realistic functionality | Pagination, search |
| Vulnerability Depth | Make vuln harder to find | Move to callback |
| Code Quality Variance | Mix good/bad code | Some files well-tested |
| Documentation | Add realistic docs | API docs, README |
| Testing | Add test coverage | Unit tests with gaps |

### Template: Innovator Proposals

```markdown
## INNOVATOR - Improvement Proposals: {{project_name}}

### Proposed Improvements

#### 1. {{improvement_name_1}}
- **Type**: {{feature/vulnerability/realism}}
- **Description**: {{what_it_adds}}
- **Implementation**: {{how_to_do_it}}
- **Value**: {{why_it_helps}}
- **Cost**: {{additional_complexity}}

#### 2. {{improvement_name_2}}
- **Type**: {{feature/vulnerability/realism}}
- **Description**: {{what_it_adds}}
- **Implementation**: {{how_to_do_it}}
- **Value**: {{why_it_helps}}
- **Cost**: {{additional_complexity}}

#### 3. {{improvement_name_3}}
- **Type**: {{feature/vulnerability/realism}}
- **Description**: {{what_it_adds}}
- **Implementation**: {{how_to_do_it}}
- **Value**: {{why_it_helps}}
- **Cost**: {{additional_complexity}}

### Priority Ranking
1. {{highest_priority}}
2. {{second_priority}}
3. {{third_priority}}

### Constraints Check
- Additional files needed: {{count}}
- Difficulty impact: {{assessment}}
- Core functionality preserved: {{yes_no}}
```

### Template: Advocate Synthesis

```markdown
## ADVOCATE - Improvement Consensus: {{project_name}}

### Improvements with Support
| Improvement | Innovator | Pragmatist | Critic | Consensus |
|-------------|-----------|------------|--------|-----------|
| {{imp_1}} | ✓/✗ | ✓/✗ | ✓/✗ | {{verdict}} |
| {{imp_2}} | ✓/✗ | ✓/✗ | ✓/✗ | {{verdict}} |
| {{imp_3}} | ✓/✗ | ✓/✗ | ✓/✗ | {{verdict}} |

### Addressing Concerns
| Concern (from) | Proposed Solution |
|----------------|-------------------|
| {{concern}} ({{persona}}) | {{solution}} |

### Recommended Final Set
1. **{{improvement_1}}**: {{brief_description}}
2. **{{improvement_2}}**: {{brief_description}}

### Implementation Notes
{{any_special_considerations}}
```

---

## 4. Feasibility Validation

### Purpose
Assess whether a project idea can actually be implemented as specified.

### Input Format

```yaml
topic: feasibility_validation
project:
  name: "blockchain-wallet"
  description: "Cryptocurrency wallet with transaction history"
  language: rust
  target_vulnerability: integer_overflow
  
technical_requirements:
  - "Cryptographic key generation"
  - "Transaction signing"
  - "Balance calculation"
  - "Network communication simulation"

concerns:
  - "Cryptocurrency projects may require specialized libraries"
  - "Transaction logic complexity"
  - "Realistic key management"
```

### Debate Questions

1. **Technical Feasibility**: Can this be built with standard tools?
2. **Time Investment**: Is the generation effort justified?
3. **Dependency Availability**: Are required libraries available/appropriate?
4. **Realism Achievability**: Can we make this look authentic?
5. **Scope Control**: Can we limit scope without losing value?

### Feasibility Checklist

| Aspect | Questions to Answer |
|--------|---------------------|
| Language/Framework | Does the tech stack support the required features? |
| Dependencies | Are all dependencies real, maintained, and appropriate? |
| Complexity | Is the implementation scope reasonable? |
| Expertise | Do we have the domain knowledge needed? |
| Time | Can this be generated in reasonable time? |

### Template: Pragmatist Assessment

```markdown
## PRAGMATIST - Feasibility Assessment: {{project_name}}

### Technical Evaluation

#### Language/Framework Fit
- **Chosen**: {{language}} with {{framework}}
- **Suitability**: {{assessment}}
- **Alternatives**: {{if_any}}

#### Dependency Analysis
| Dependency | Purpose | Available | Maintained | Appropriate |
|------------|---------|-----------|------------|-------------|
| {{dep_1}} | {{purpose}} | ✓/✗ | ✓/✗ | ✓/✗ |
| {{dep_2}} | {{purpose}} | ✓/✗ | ✓/✗ | ✓/✗ |

#### Implementation Complexity
- **Estimated LOC**: {{lines_of_code}}
- **Core Components**: {{count}}
- **Generation Time**: {{estimate}}
- **Risk Areas**: {{list}}

### Feasibility Verdict
| Aspect | Score (1-5) | Notes |
|--------|-------------|-------|
| Technical | {{score}} | {{notes}} |
| Dependencies | {{score}} | {{notes}} |
| Complexity | {{score}} | {{notes}} |
| Realism | {{score}} | {{notes}} |

**Overall Feasibility**: {{FEASIBLE/NEEDS_CHANGES/INFEASIBLE}}

### Required Changes (if applicable)
1. {{change_1}}
2. {{change_2}}

### Alternative Approach (if needed)
{{simpler_alternative_that_achieves_similar_goals}}
```

### Template: Validator Final Assessment

```markdown
## VALIDATOR - Feasibility Ruling: {{project_name}}

### Debate Summary
| Persona | Verdict | Key Concern |
|---------|---------|-------------|
| Innovator | {{verdict}} | {{concern}} |
| Pragmatist | {{verdict}} | {{concern}} |
| Critic | {{verdict}} | {{concern}} |
| Advocate | {{verdict}} | {{concern}} |

### Critical Issues Identified
1. {{issue_1}}: {{severity}}
2. {{issue_2}}: {{severity}}

### Mitigations Proposed
| Issue | Mitigation | Viable |
|-------|------------|--------|
| {{issue_1}} | {{mitigation}} | ✓/✗ |
| {{issue_2}} | {{mitigation}} | ✓/✗ |

### Final Ruling

**Decision**: {{FEASIBLE/FEASIBLE_WITH_CHANGES/INFEASIBLE}}

### Action Items
{{if_feasible}}
1. Proceed with modifications: {{list}}

{{if_infeasible}}
1. Alternative project type: {{suggestion}}
2. Reason this version won't work: {{explanation}}

### Risk Assessment
- **Technical Risk**: {{low/medium/high}}
- **Schedule Risk**: {{low/medium/high}}
- **Quality Risk**: {{low/medium/high}}
```

---

## Debate Orchestration

### Round Structure

```yaml
round_1:
  - innovator: "Initial position and proposals"
  - pragmatist: "Feasibility response"
  - critic: "Issues and concerns"
  - advocate: "Addresses concerns, finds common ground"

round_2:
  - innovator: "Responds to criticism"
  - pragmatist: "Updates assessment"
  - critic: "Evaluates responses"
  - advocate: "Proposes compromise"

round_3:
  - all: "Final positions"
  - validator: "Synthesizes and decides"
```

### Termination Conditions

Debate ends when:
1. Validator makes a final ruling
2. All personas reach consensus
3. Maximum rounds exceeded
4. Topic is withdrawn by orchestrator

### Output Format

```yaml
debate_result:
  topic: "{{topic_type}}"
  project: "{{project_name}}"
  rounds_taken: {{count}}
  
  decision: "{{DECISION}}"
  confidence: {{0.0-1.0}}
  
  key_points:
    - "{{point_1}}"
    - "{{point_2}}"
  
  modifications_required:
    - "{{mod_1}}"
    - "{{mod_2}}"
  
  dissenting_opinions:
    - persona: "{{name}}"
      opinion: "{{summary}}"
  
  next_steps:
    - "{{step_1}}"
    - "{{step_2}}"
```
