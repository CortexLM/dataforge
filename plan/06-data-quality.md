# 📊 Data Quality Framework - Ensuring High-Quality Synthetic Datasets

## 1. The Quality Challenge

### 1.1 What Makes a Good Trajectory?

A high-quality trajectory for training should be:

| Property | Description | Why It Matters |
|----------|-------------|----------------|
| **Correct** | Solves the task correctly | Training on failures is harmful |
| **Complete** | Contains all necessary steps | Partial trajectories confuse models |
| **Coherent** | Logical step progression | Random actions don't teach planning |
| **Diverse** | Different from other trajectories | Redundancy wastes compute |
| **Generalizable** | Teaches transferable skills | Overfitting to specific patterns |

### 1.2 Quality Failure Modes

```
❌ INCORRECT                    ❌ INCOMPLETE
Task: Fix the bug              Task: Implement auth
Steps: [edit, edit, edit]      Steps: [research, plan...]
Result: Bug still exists       Result: Implementation missing

❌ INCOHERENT                   ❌ REDUNDANT
Task: Optimize function        Task: Parse JSON
Steps: [random edits, undo]    Steps: [same as 100 others]
Result: No clear strategy      Result: No diversity

❌ NON-GENERALIZABLE
Task: Fix typo in line 42
Steps: [delete char at pos 5]
Result: Works only for this exact case
```

---

## 2. Quality Dimensions

### 2.1 Correctness Verification

```rust
pub struct CorrectnessChecker {
    test_runner: TestRunner,
    static_analyzer: StaticAnalyzer,
    reference_validator: ReferenceValidator,
}

pub enum CorrectnessResult {
    Correct { confidence: f32 },
    PartiallyCorrect { 
        passed_checks: Vec<String>,
        failed_checks: Vec<String>,
        confidence: f32,
    },
    Incorrect { 
        reason: String,
        evidence: String,
    },
    Unknown { 
        reason: String,
    },
}

impl CorrectnessChecker {
    pub async fn verify(&self, trajectory: &Trajectory, task: &Task) -> CorrectnessResult {
        // 1. Run provided tests
        let test_results = self.test_runner.run(&task.tests).await;
        
        // 2. Static analysis (syntax, types, linting)
        let static_results = self.static_analyzer.analyze(&trajectory.final_state).await;
        
        // 3. Compare with reference solution (if available)
        let reference_match = self.reference_validator.compare(
            &trajectory.final_state,
            &task.reference_solution,
        ).await;
        
        // 4. Combine results
        self.aggregate(test_results, static_results, reference_match)
    }
}
```

### 2.2 Completeness Verification

```rust
pub struct CompletenessChecker {
    min_steps_per_difficulty: HashMap<Difficulty, u32>,
    required_action_types: HashMap<TaskCategory, Vec<ActionType>>,
}

impl CompletenessChecker {
    pub fn verify(&self, trajectory: &Trajectory, task: &Task) -> CompletenessResult {
        let issues = vec![];
        
        // 1. Check minimum steps
        let min_steps = self.min_steps_per_difficulty.get(&task.difficulty);
        if trajectory.steps.len() < *min_steps {
            issues.push("Too few steps - may be incomplete");
        }
        
        // 2. Check for required actions
        let required = self.required_action_types.get(&task.category);
        for action_type in required {
            if !trajectory.contains_action(action_type) {
                issues.push(format!("Missing required action: {}", action_type));
            }
        }
        
        // 3. Check for premature termination
        if trajectory.last_action() == ActionType::GiveUp {
            issues.push("Agent gave up - incomplete solution");
        }
        
        // 4. Check for unfinished state
        if self.has_uncommitted_changes(&trajectory.final_state) {
            issues.push("Uncommitted changes detected");
        }
        
        CompletenessResult {
            is_complete: issues.is_empty(),
            issues,
        }
    }
}
```

### 2.3 Coherence Verification

```rust
pub struct CoherenceChecker {
    llm: Box<dyn LlmProvider>,
}

impl CoherenceChecker {
    pub async fn verify(&self, trajectory: &Trajectory) -> CoherenceResult {
        // 1. Check for action-observation consistency
        let consistency = self.check_action_observation_pairs(trajectory);
        
        // 2. Check for logical progression
        let progression = self.check_logical_progression(trajectory);
        
        // 3. Check for backtracking ratio
        let backtrack_ratio = self.calculate_backtrack_ratio(trajectory);
        
        // 4. Use LLM for semantic coherence check
        let semantic_coherence = self.llm_coherence_check(trajectory).await;
        
        CoherenceResult {
            action_observation_consistency: consistency,
            logical_progression_score: progression,
            backtrack_ratio,
            semantic_coherence_score: semantic_coherence,
            overall_score: self.aggregate_scores(/*...*/),
        }
    }
    
    fn check_logical_progression(&self, trajectory: &Trajectory) -> f32 {
        // Actions should build on previous observations
        // Score based on:
        // - Are file reads followed by relevant edits?
        // - Does the agent follow through on its stated plan?
        // - Are there unexplained context switches?
        // ...
    }
}
```

### 2.4 Diversity Measurement

```rust
pub struct DiversityAnalyzer {
    embedding_model: EmbeddingModel,
    existing_trajectories: TrajectoryIndex,
}

impl DiversityAnalyzer {
    pub fn measure_diversity(&self, trajectory: &Trajectory) -> DiversityScore {
        // 1. Action sequence diversity
        let action_sequence = trajectory.action_sequence();
        let action_diversity = self.action_sequence_novelty(&action_sequence);
        
        // 2. Solution approach diversity
        let approach_embedding = self.embedding_model.embed(&trajectory.solution_summary());
        let approach_diversity = self.nearest_neighbor_distance(approach_embedding);
        
        // 3. Tool usage diversity
        let tool_distribution = trajectory.tool_usage_distribution();
        let tool_diversity = self.compare_tool_distribution(tool_distribution);
        
        DiversityScore {
            action_novelty: action_diversity,
            approach_novelty: approach_diversity,
            tool_novelty: tool_diversity,
            is_duplicate: approach_diversity < DUPLICATE_THRESHOLD,
        }
    }
}
```

---

## 3. Quality Pipeline

### 3.1 Multi-Stage Filtering

```
           ┌─────────────────────────────────────────────────────────────┐
           │                   RAW TRAJECTORIES                          │
           │                   (All generated)                           │
           └────────────────────────┬────────────────────────────────────┘
                                    │
                                    ▼
           ┌─────────────────────────────────────────────────────────────┐
           │                 STAGE 1: BASIC FILTERING                    │
           │  - Remove crashed/errored trajectories                      │
           │  - Remove timeouts                                          │
           │  - Remove empty/trivial trajectories                        │
           │                                                             │
           │  Pass Rate: ~70%                                            │
           └────────────────────────┬────────────────────────────────────┘
                                    │
                                    ▼
           ┌─────────────────────────────────────────────────────────────┐
           │                 STAGE 2: CORRECTNESS FILTER                 │
           │  - Run verification tests                                   │
           │  - Static analysis                                          │
           │  - Reference solution comparison                            │
           │                                                             │
           │  Pass Rate: ~60%                                            │
           └────────────────────────┬────────────────────────────────────┘
                                    │
                                    ▼
           ┌─────────────────────────────────────────────────────────────┐
           │                 STAGE 3: QUALITY SCORING                    │
           │  - Coherence check                                          │
           │  - Completeness check                                       │
           │  - Efficiency score                                         │
           │                                                             │
           │  Keep top 80% by score                                      │
           └────────────────────────┬────────────────────────────────────┘
                                    │
                                    ▼
           ┌─────────────────────────────────────────────────────────────┐
           │                 STAGE 4: DIVERSITY FILTER                   │
           │  - Near-duplicate detection                                 │
           │  - Approach clustering                                      │
           │  - Stratified sampling                                      │
           │                                                             │
           │  Keep diverse subset                                        │
           └────────────────────────┬────────────────────────────────────┘
                                    │
                                    ▼
           ┌─────────────────────────────────────────────────────────────┐
           │                 STAGE 5: HUMAN REVIEW                       │
           │  - Sample review (5-10%)                                    │
           │  - Edge case review                                         │
           │  - Category balance check                                   │
           │                                                             │
           │  Final dataset                                              │
           └─────────────────────────────────────────────────────────────┘
```

### 3.2 Quality Metrics Dashboard

```rust
pub struct QualityMetrics {
    // Volume metrics
    pub total_generated: u64,
    pub passed_basic_filter: u64,
    pub passed_correctness: u64,
    pub passed_quality: u64,
    pub final_count: u64,
    
    // Rate metrics
    pub generation_rate: f32,       // trajectories/hour
    pub correctness_rate: f32,      // % correct
    pub quality_pass_rate: f32,     // % high quality
    pub diversity_coverage: f32,    // % of task space covered
    
    // Distribution metrics
    pub category_distribution: HashMap<Category, u64>,
    pub difficulty_distribution: HashMap<Difficulty, u64>,
    pub model_distribution: HashMap<String, u64>,
    pub scaffold_distribution: HashMap<String, u64>,
    
    // Quality scores
    pub avg_coherence_score: f32,
    pub avg_efficiency_score: f32,
    pub avg_diversity_score: f32,
}
```

---

## 4. Ensuring Data Diversity

### 4.1 Diversity Strategies

#### Strategy 1: Task Diversity

```rust
pub struct TaskDiversifier {
    categories: Vec<Category>,
    difficulty_levels: Vec<Difficulty>,
    target_distribution: HashMap<(Category, Difficulty), f32>,
}

impl TaskDiversifier {
    pub fn sample_next_task(&self, current_counts: &HashMap<(Category, Difficulty), u64>) -> TaskSelector {
        // Calculate underrepresented categories
        let gaps = self.calculate_distribution_gaps(current_counts);
        
        // Prioritize underrepresented areas
        let weights = gaps.iter()
            .map(|(key, gap)| (key, gap.max(0.0)))
            .collect();
        
        TaskSelector::weighted(weights)
    }
}
```

#### Strategy 2: Model Diversity

```rust
// Force different models to solve same tasks
pub fn diversify_by_model(tasks: &[Task], models: &[String]) -> Vec<(Task, String)> {
    tasks.iter()
        .flat_map(|task| {
            models.iter().map(|model| (task.clone(), model.clone()))
        })
        .collect()
}
```

#### Strategy 3: Prompt Diversity

```rust
// Generate multiple phrasings of same task
pub fn diversify_prompts(task: &Task, count: usize) -> Vec<Task> {
    let rephrasings = vec![
        rephrase_formal(&task.instruction),
        rephrase_casual(&task.instruction),
        rephrase_detailed(&task.instruction),
        rephrase_concise(&task.instruction),
        add_context(&task.instruction),
        add_constraints(&task.instruction),
    ];
    
    rephrasings.into_iter()
        .take(count)
        .map(|instruction| Task { instruction, ..task.clone() })
        .collect()
}
```

#### Strategy 4: Temperature Diversity

```rust
// Use different LLM temperatures
pub fn diversify_by_temperature(task: &Task) -> Vec<(Task, f32)> {
    vec![
        (task.clone(), 0.0),   // Deterministic
        (task.clone(), 0.3),   // Low creativity
        (task.clone(), 0.7),   // Medium creativity
        (task.clone(), 1.0),   // High creativity
    ]
}
```

### 4.2 Anti-Duplication Measures

```rust
pub struct DuplicateDetector {
    trajectory_hashes: HashSet<u64>,
    solution_embeddings: NearestNeighborIndex,
    action_sequence_trie: TrieIndex,
}

impl DuplicateDetector {
    pub fn is_duplicate(&self, trajectory: &Trajectory) -> bool {
        // Exact match check
        let hash = self.hash_trajectory(trajectory);
        if self.trajectory_hashes.contains(&hash) {
            return true;
        }
        
        // Near-duplicate check via embeddings
        let embedding = self.embed_trajectory(trajectory);
        let nearest = self.solution_embeddings.nearest(&embedding);
        if nearest.distance < NEAR_DUPLICATE_THRESHOLD {
            return true;
        }
        
        // Action sequence similarity
        let action_seq = trajectory.action_sequence();
        if self.action_sequence_trie.longest_match(&action_seq).len() > action_seq.len() * 0.8 {
            return true;
        }
        
        false
    }
}
```

---

## 5. Critical Questions for Data Quality

### 5.1 Task Design Questions

| Question | Why It Matters | How to Address |
|----------|----------------|----------------|
| Are tasks representative of real-world problems? | Transfer to actual use cases | Collect tasks from real developers |
| Are tasks too easy? | Model won't learn complex reasoning | Add difficulty amplification |
| Are tasks too hard? | Low success rate = wasted compute | Validate solvability before generation |
| Do tasks have clear success criteria? | Ambiguous tasks → inconsistent rewards | Define explicit verification tests |

### 5.2 Trajectory Quality Questions

| Question | Why It Matters | How to Address |
|----------|----------------|----------------|
| How do we verify correctness without ground truth? | Can't always know the "right" answer | Multiple verification approaches |
| What if the model finds unexpected valid solutions? | May be filtered as incorrect | Accept novel correct solutions |
| How do we handle partial successes? | Binary labels lose nuance | Continuous reward signals |
| When is a trajectory "good enough"? | Perfectionism vs throughput | Define minimum quality thresholds |

### 5.3 Diversity Questions

| Question | Why It Matters | How to Address |
|----------|----------------|----------------|
| How diverse is "diverse enough"? | Unknown optimal diversity level | Measure and experiment |
| Can we measure solution approach diversity? | Surface metrics may miss semantics | Use embedding-based comparison |
| How do we avoid mode collapse? | Models may repeat same strategy | Force diversity constraints |
| What's the right model mix? | Different models have different strengths | Experiment with ratios |

### 5.4 Scale vs Quality Trade-offs

| Question | Options | Trade-off |
|----------|---------|-----------|
| More trajectories or better trajectories? | Volume vs Quality | Quality usually wins |
| Fast filtering or thorough filtering? | Speed vs Precision | Depends on budget |
| Human review percentage? | 0% / 5% / 20% | Cost vs confidence |
| When to regenerate vs discard? | Retry failed tasks or move on | Compute cost vs coverage |

---

## 6. Reward Design

### 6.1 Reward Components

```rust
pub struct RewardCalculator {
    weights: RewardWeights,
}

pub struct RewardWeights {
    pub correctness: f32,      // 0.5 - Did it solve the task?
    pub efficiency: f32,       // 0.2 - Did it use few steps?
    pub code_quality: f32,     // 0.1 - Is the code clean?
    pub reasoning: f32,        // 0.1 - Did it explain well?
    pub tool_usage: f32,       // 0.1 - Did it use tools appropriately?
}

impl RewardCalculator {
    pub fn calculate(&self, trajectory: &Trajectory, task: &Task) -> f32 {
        let correctness = self.calculate_correctness(trajectory, task);
        let efficiency = self.calculate_efficiency(trajectory, task);
        let code_quality = self.calculate_code_quality(trajectory);
        let reasoning = self.calculate_reasoning_quality(trajectory);
        let tool_usage = self.calculate_tool_usage(trajectory);
        
        self.weights.correctness * correctness
            + self.weights.efficiency * efficiency
            + self.weights.code_quality * code_quality
            + self.weights.reasoning * reasoning
            + self.weights.tool_usage * tool_usage
    }
    
    fn calculate_efficiency(&self, trajectory: &Trajectory, task: &Task) -> f32 {
        let steps = trajectory.steps.len() as f32;
        let expected = task.expected_steps as f32;
        
        // Reward for being close to expected, penalize way over
        let ratio = steps / expected;
        if ratio <= 1.0 {
            1.0
        } else {
            (2.0 - ratio).max(0.0)
        }
    }
}
```

### 6.2 Intermediate Rewards

```rust
pub fn calculate_step_reward(
    prev_state: &State,
    action: &Action,
    new_state: &State,
    task: &Task,
) -> f32 {
    let mut reward = 0.0;
    
    // Progress toward goal
    let prev_progress = calculate_progress(prev_state, task);
    let new_progress = calculate_progress(new_state, task);
    reward += (new_progress - prev_progress) * 0.5;
    
    // Test pass changes
    let prev_tests = count_passing_tests(prev_state, task);
    let new_tests = count_passing_tests(new_state, task);
    reward += (new_tests - prev_tests) as f32 * 0.1;
    
    // Compilation status
    if !prev_state.compiles && new_state.compiles {
        reward += 0.1;
    }
    
    // Penalty for regressions
    if new_state.has_new_errors_compared_to(prev_state) {
        reward -= 0.1;
    }
    
    reward
}
```

---

## 7. Implementation Phases

### Phase 1: Basic Quality Checks (Week 1)
- [ ] Correctness verification (test execution)
- [ ] Basic filtering (errors, timeouts)
- [ ] Simple deduplication (hash-based)

### Phase 2: Advanced Metrics (Week 2)
- [ ] Coherence scoring
- [ ] Efficiency metrics
- [ ] Completeness checks

### Phase 3: Diversity Analysis (Week 3)
- [ ] Embedding-based similarity
- [ ] Near-duplicate detection
- [ ] Distribution tracking

### Phase 4: Quality Dashboard (Week 4)
- [ ] Real-time metrics
- [ ] Alerts for quality degradation
- [ ] Human review interface
