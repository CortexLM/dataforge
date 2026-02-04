//! Category-specific prompts for synthetic benchmark task generation.
//!
//! This module defines detailed prompt templates for each benchmark category,
//! providing domain-specific guidance for generating challenging, non-memorizable tasks.
//!
//! Each category prompt includes:
//! - A tailored system prompt for the domain
//! - Difficulty guidelines explaining what makes a task HARD
//! - Anti-patterns to avoid (memorizable solutions, common mistakes)
//! - Required skills that should be tested
//! - Example themes for task inspiration

use std::collections::HashMap;
use std::sync::LazyLock;

/// Category-specific prompt configuration for task generation.
///
/// This struct contains all the domain-specific guidance needed to generate
/// high-quality benchmark tasks for a particular category.
#[derive(Debug, Clone)]
pub struct CategoryPrompt {
    /// The category identifier (e.g., "AlgorithmDesign").
    pub category: &'static str,
    /// System prompt tailored to this domain.
    pub system_prompt: &'static str,
    /// Guidelines for what constitutes a HARD task in this category.
    pub difficulty_guidelines: &'static str,
    /// Patterns and approaches that should be avoided.
    pub anti_patterns: &'static [&'static str],
    /// Skills that tasks in this category should test.
    pub required_skills: &'static [&'static str],
    /// Example themes for task inspiration.
    pub example_themes: &'static [&'static str],
}

/// Static array of all category prompts.
pub static CATEGORY_PROMPTS: &[CategoryPrompt] = &[
    // ==========================================================================
    // ALGORITHM DESIGN
    // ==========================================================================
    CategoryPrompt {
        category: "AlgorithmDesign",
        system_prompt: r#"You are an expert algorithm designer creating NOVEL algorithmic challenges.

Your tasks must require CREATIVE PROBLEM-SOLVING, not textbook algorithm application. Focus on:
- Problems with UNUSUAL CONSTRAINTS that break standard approaches
- Scenarios requiring HYBRID SOLUTIONS combining multiple techniques
- Real-world optimization problems with competing objectives
- Problems where the NAIVE SOLUTION has unacceptable complexity

CRITICAL: Standard algorithms (sorting, searching, basic graph traversal) are TOO EASY.
Tasks must require ADAPTATION or INVENTION of algorithms, not mere implementation.

Domain expertise required:
- Complexity analysis and amortized analysis
- Approximation algorithms for NP-hard problems
- Online algorithms and competitive analysis
- Randomized algorithms and probabilistic guarantees
- Cache-oblivious algorithms and memory hierarchy awareness"#,

        difficulty_guidelines: r#"A HARD algorithm design task requires:
1. NO direct textbook solution exists - standard algorithms fail or are suboptimal
2. Multiple competing constraints (time vs space, accuracy vs speed)
3. Hidden structure in the problem that must be discovered
4. At least 3 algorithmic techniques must be combined
5. Correctness proof or complexity analysis is non-trivial
6. Edge cases that break obvious approaches

The solver must INVENT or ADAPT algorithms, not recall them."#,

        anti_patterns: &[
            "Standard sorting or searching problems",
            "Basic dynamic programming with obvious recurrence",
            "Graph algorithms solvable by BFS/DFS/Dijkstra directly",
            "Problems with solutions in common algorithm textbooks",
            "Tasks where complexity analysis is straightforward",
            "Problems solvable by single well-known technique",
        ],

        required_skills: &[
            "Algorithm design and analysis",
            "Complexity theory (time and space)",
            "Data structure selection and design",
            "Optimization under constraints",
            "Problem decomposition",
            "Correctness argumentation",
        ],

        example_themes: &[
            "Scheduling with interdependent tasks and resource contention",
            "Graph problems with dynamic edge weights and topology changes",
            "Streaming algorithms with memory constraints and accuracy guarantees",
            "Geometric problems in high dimensions with approximation requirements",
            "Online decision problems with competitive ratio analysis",
            "String algorithms with wildcards and approximate matching",
        ],
    },
    // ==========================================================================
    // SYSTEM DEBUGGING
    // ==========================================================================
    CategoryPrompt {
        category: "SystemDebugging",
        system_prompt: r#"You are an expert systems debugger creating COMPLEX debugging scenarios.

Your tasks must involve MULTI-SERVICE, DISTRIBUTED SYSTEM issues that require:
- Correlation of evidence across multiple components
- Understanding of non-obvious failure modes
- Race conditions, deadlocks, or timing-dependent bugs
- Issues that manifest differently under different conditions

CRITICAL: Simple single-service bugs are TOO EASY. Tasks must involve:
- Cascading failures across service boundaries
- Intermittent issues that depend on timing or load
- Bugs in the interaction BETWEEN components, not within them
- Issues requiring understanding of system architecture

Domain expertise required:
- Distributed systems failure modes
- Observability (logs, metrics, traces)
- Network protocols and failure scenarios
- Operating system internals
- Concurrency and parallelism primitives"#,

        difficulty_guidelines: r#"A HARD system debugging task requires:
1. Multiple services or components involved in the failure
2. Root cause is NOT in the most obvious suspect component
3. Reproduction requires specific timing or load conditions
4. Symptoms are misleading or affect unexpected areas
5. Debugging requires correlating data from 3+ sources
6. The fix requires understanding cross-component interactions

The solver must be a DETECTIVE, not just read error messages."#,

        anti_patterns: &[
            "Single-service bugs with clear stack traces",
            "Syntax errors or obvious type mismatches",
            "Bugs reproducible with single request",
            "Issues with clear error messages pointing to root cause",
            "Problems solvable by reading one log file",
            "Bugs fixed by simple configuration changes",
        ],

        required_skills: &[
            "Distributed systems architecture",
            "Log analysis and correlation",
            "Performance profiling",
            "Network debugging",
            "Concurrency bug identification",
            "Root cause analysis methodology",
        ],

        example_themes: &[
            "Cascading timeout failures across microservices",
            "Memory leaks caused by cross-service callback chains",
            "Race conditions in distributed lock implementations",
            "Network partition scenarios with split-brain behavior",
            "Deadlocks involving multiple services and databases",
            "Performance degradation under specific load patterns",
        ],
    },
    // ==========================================================================
    // SECURITY ANALYSIS
    // ==========================================================================
    CategoryPrompt {
        category: "SecurityAnalysis",
        system_prompt: r#"You are an expert security researcher creating SUBTLE vulnerability challenges.

Your tasks must involve LOGIC VULNERABILITIES and complex attack scenarios, not just:
- Simple buffer overflows
- Basic SQL injection
- Obvious authentication bypasses

Focus on vulnerabilities requiring DEEP UNDERSTANDING of:
- Application logic and business rules
- Authentication/authorization state machines
- Cryptographic protocol weaknesses
- Side-channel information leakage
- Race conditions with security implications

CRITICAL: CVE-style memory corruption is well-documented. Tasks must require:
- Understanding the INTENDED security model
- Identifying subtle violations of security assumptions
- Chaining multiple issues for exploitation
- Bypassing defense-in-depth mechanisms"#,

        difficulty_guidelines: r#"A HARD security analysis task requires:
1. Vulnerability is in LOGIC, not just implementation
2. Exploitation requires chaining 2+ issues together
3. Standard security scanners would NOT find it
4. Understanding the business logic is essential
5. The vulnerability involves subtle state or timing
6. Defense bypass requires creative thinking

The solver must THINK LIKE AN ATTACKER with architectural knowledge."#,

        anti_patterns: &[
            "Simple input validation failures",
            "Basic XSS or CSRF without complexity",
            "Default credentials or missing authentication",
            "Known CVEs or documented vulnerabilities",
            "Issues found by automated scanners",
            "Single-step exploitation without chaining",
        ],

        required_skills: &[
            "Threat modeling",
            "Authentication/authorization systems",
            "Cryptographic protocol analysis",
            "Web security (advanced)",
            "Binary analysis",
            "Side-channel analysis",
        ],

        example_themes: &[
            "OAuth flow vulnerabilities with state manipulation",
            "JWT algorithm confusion attacks",
            "Race conditions in privilege escalation",
            "Timing attacks against authentication",
            "Business logic bypasses in multi-step workflows",
            "Cache poisoning with security implications",
        ],
    },
    // ==========================================================================
    // INFRASTRUCTURE
    // ==========================================================================
    CategoryPrompt {
        category: "Infrastructure",
        system_prompt: r#"You are an expert infrastructure engineer creating COMPLEX deployment scenarios.

Your tasks must involve PRODUCTION-GRADE challenges requiring:
- Zero-downtime migrations with data consistency
- Multi-region deployments with failover
- Resource optimization under real constraints
- Complex networking and security requirements

CRITICAL: Simple deployments are TOO EASY. Tasks must involve:
- Multiple interdependent services
- State management during transitions
- Rollback scenarios and failure recovery
- Performance requirements during operations

Domain expertise required:
- Container orchestration (Kubernetes, etc.)
- Infrastructure as Code
- Cloud provider services and limits
- Networking (load balancing, service mesh)
- Database operations at scale"#,

        difficulty_guidelines: r#"A HARD infrastructure task requires:
1. Zero-downtime requirement during significant changes
2. State or data migration with consistency requirements
3. Multiple failure scenarios that must be handled
4. Resource constraints that affect approach
5. Security requirements that complicate solutions
6. Rollback plan that actually works

The solver must balance MULTIPLE CONCERNS simultaneously."#,

        anti_patterns: &[
            "Simple single-service deployments",
            "Greenfield setups without migration",
            "Tasks without availability requirements",
            "Scenarios ignoring security constraints",
            "Deployments without rollback considerations",
            "Problems with unlimited resource budgets",
        ],

        required_skills: &[
            "Container orchestration",
            "Infrastructure as Code",
            "Database operations",
            "Networking and service mesh",
            "Monitoring and alerting",
            "Disaster recovery planning",
        ],

        example_themes: &[
            "Zero-downtime database schema migration with data backfill",
            "Cross-region failover with stateful services",
            "Kubernetes cluster upgrade with workload continuity",
            "Service mesh migration without traffic disruption",
            "Cost optimization with performance SLA requirements",
            "Multi-tenant isolation in shared infrastructure",
        ],
    },
    // ==========================================================================
    // DATA ENGINEERING
    // ==========================================================================
    CategoryPrompt {
        category: "DataEngineering",
        system_prompt: r#"You are an expert data engineer creating COMPLEX data pipeline challenges.

Your tasks must involve REAL-WORLD data complexity:
- Schema evolution and backward compatibility
- Exactly-once semantics in distributed processing
- Data quality issues requiring detection and handling
- Performance optimization at scale

CRITICAL: Simple ETL tasks are TOO EASY. Tasks must involve:
- Complex transformation logic with edge cases
- Incremental processing with state management
- Data from multiple sources requiring reconciliation
- Quality guarantees with validation requirements

Domain expertise required:
- Stream and batch processing frameworks
- Data modeling and schema design
- Data quality and validation
- Distributed systems consistency
- Performance tuning for data workloads"#,

        difficulty_guidelines: r#"A HARD data engineering task requires:
1. Multiple data sources with different schemas/formats
2. Edge cases that break naive transformations
3. Incremental processing with exactly-once semantics
4. Data quality issues that must be detected and handled
5. Schema evolution with backward compatibility
6. Performance requirements at realistic scale

The solver must handle MESSY REAL-WORLD data."#,

        anti_patterns: &[
            "Simple one-to-one mappings",
            "Static schemas without evolution",
            "Full refresh instead of incremental",
            "Ignoring data quality issues",
            "Tasks without scale considerations",
            "Problems with perfectly clean input data",
        ],

        required_skills: &[
            "Data modeling",
            "ETL/ELT design",
            "Stream processing",
            "Data quality frameworks",
            "Schema evolution strategies",
            "Performance optimization",
        ],

        example_themes: &[
            "CDC pipeline with schema evolution and late arrivals",
            "Multi-source reconciliation with conflict resolution",
            "Incremental aggregation with restatement support",
            "Data quality framework with anomaly detection",
            "Real-time feature engineering with consistency guarantees",
            "Data lakehouse optimization for mixed workloads",
        ],
    },
    // ==========================================================================
    // REVERSE ENGINEERING
    // ==========================================================================
    CategoryPrompt {
        category: "ReverseEngineering",
        system_prompt: r#"You are an expert reverse engineer creating CHALLENGING analysis tasks.

Your tasks must require DEEP ANALYSIS, not just tool usage:
- Custom binary formats with undocumented structures
- Protocols with proprietary encodings
- State machines that must be extracted from behavior
- Obfuscation that requires understanding to defeat

CRITICAL: Running strings/binwalk is TOO EASY. Tasks must involve:
- Understanding of compiler/language patterns
- Recognition of algorithmic structures in assembly
- Protocol state machine reconstruction
- Multi-layer obfuscation or encoding

Domain expertise required:
- Assembly language (multiple architectures)
- Compiler internals and optimizations
- Binary file formats
- Protocol analysis techniques
- Deobfuscation strategies"#,

        difficulty_guidelines: r#"A HARD reverse engineering task requires:
1. No documentation or specifications available
2. Multiple layers of encoding or obfuscation
3. Understanding of algorithmic intent, not just bytes
4. State machines with non-obvious transitions
5. Custom data structures that must be discovered
6. Cross-references and dependencies to track

The solver must UNDERSTAND INTENT, not just decode bytes."#,

        anti_patterns: &[
            "Well-documented file formats",
            "Standard protocols with RFCs",
            "Simple XOR or substitution encoding",
            "Binaries with debug symbols",
            "Tasks solvable by single tool",
            "Problems without state or structure",
        ],

        required_skills: &[
            "Assembly language reading",
            "Binary format analysis",
            "Protocol reverse engineering",
            "Deobfuscation techniques",
            "Pattern recognition",
            "Tool development for analysis",
        ],

        example_themes: &[
            "Custom binary protocol with versioning and compression",
            "Proprietary file format with nested structures",
            "State machine extraction from compiled code",
            "Firmware analysis with hardware interactions",
            "Network protocol with challenge-response authentication",
            "Obfuscated algorithm identification and reconstruction",
        ],
    },
    // ==========================================================================
    // PERFORMANCE OPTIMIZATION
    // ==========================================================================
    CategoryPrompt {
        category: "PerformanceOptimization",
        system_prompt: r#"You are an expert performance engineer creating SUBTLE optimization challenges.

Your tasks must involve NON-OBVIOUS bottlenecks requiring:
- Profiling and measurement to identify issues
- Understanding of hardware (CPU, memory, I/O)
- Trade-offs between different resources
- System-level optimizations beyond code changes

CRITICAL: "Use a hash map instead of list" is TOO EASY. Tasks must involve:
- Bottlenecks not visible from code review alone
- Cache effects and memory access patterns
- Contention and synchronization costs
- Trade-offs with correctness or maintainability

Domain expertise required:
- CPU architecture and caching
- Memory hierarchy and allocation
- Profiling tools and techniques
- Concurrency and lock contention
- I/O patterns and optimization"#,

        difficulty_guidelines: r#"A HARD performance optimization task requires:
1. Bottleneck is NOT obvious from reading code
2. Profiling data requires interpretation
3. Multiple potential optimizations with trade-offs
4. Hardware understanding affects solution
5. Optimization has risks (correctness, maintainability)
6. Measurement methodology matters

The solver must MEASURE, UNDERSTAND, then OPTIMIZE."#,

        anti_patterns: &[
            "O(n²) to O(n log n) algorithm swaps",
            "Obvious cache misses from random access",
            "Single clear bottleneck",
            "Optimizations without trade-offs",
            "Problems where profiler directly shows the fix",
            "Micro-optimizations without system context",
        ],

        required_skills: &[
            "Performance profiling",
            "CPU architecture understanding",
            "Memory optimization",
            "Concurrency tuning",
            "I/O optimization",
            "Benchmarking methodology",
        ],

        example_themes: &[
            "False sharing in multi-threaded data structures",
            "Lock contention with non-obvious holder",
            "Memory allocation patterns causing fragmentation",
            "Cache pollution from background operations",
            "I/O scheduling affecting latency distribution",
            "JIT compilation behavior impacting performance",
        ],
    },
    // ==========================================================================
    // INTEGRATION TASKS
    // ==========================================================================
    CategoryPrompt {
        category: "IntegrationTasks",
        system_prompt: r#"You are an expert systems integrator creating COMPLEX orchestration challenges.

Your tasks must involve MULTI-SYSTEM coordination requiring:
- Transaction boundaries across services
- Eventually consistent systems with reconciliation
- Error handling across multiple failure domains
- API versioning and compatibility concerns

CRITICAL: Single API integration is TOO EASY. Tasks must involve:
- Multiple systems with different consistency models
- Saga patterns or distributed transactions
- Handling partial failures gracefully
- State synchronization across boundaries

Domain expertise required:
- Distributed systems patterns
- API design and versioning
- Message queues and event systems
- Consistency models and trade-offs
- Error handling and recovery patterns"#,

        difficulty_guidelines: r#"A HARD integration task requires:
1. 3+ systems with different characteristics
2. Consistency requirements across boundaries
3. Partial failure scenarios that must be handled
4. Ordering or timing dependencies
5. Recovery and retry with idempotency
6. Version compatibility considerations

The solver must ORCHESTRATE chaos into order."#,

        anti_patterns: &[
            "Single API call and response",
            "Systems with same consistency model",
            "Ignoring partial failure scenarios",
            "Tasks without timing considerations",
            "Problems with simple rollback solutions",
            "Integrations without version concerns",
        ],

        required_skills: &[
            "Distributed systems design",
            "API design patterns",
            "Message queue systems",
            "Saga pattern implementation",
            "Error handling strategies",
            "Monitoring and observability",
        ],

        example_themes: &[
            "Saga orchestration with compensation actions",
            "Event sourcing with multiple projections",
            "API gateway with complex routing and transformation",
            "Multi-database transaction coordination",
            "Real-time sync between systems with different models",
            "Workflow engine with external service dependencies",
        ],
    },
];

/// Lookup map for category prompts by name.
static CATEGORY_LOOKUP: LazyLock<HashMap<&'static str, &'static CategoryPrompt>> =
    LazyLock::new(|| {
        let mut map = HashMap::new();
        for prompt in CATEGORY_PROMPTS {
            map.insert(prompt.category, prompt);
        }
        map
    });

/// Retrieves the category prompt for a given category name.
///
/// # Arguments
///
/// * `category` - The category name (e.g., "AlgorithmDesign", "SystemDebugging")
///
/// # Returns
///
/// The `CategoryPrompt` if found, or `None` for unknown categories.
///
/// # Examples
///
/// ```
/// use dataforge::prompts::get_category_prompt;
///
/// let prompt = get_category_prompt("AlgorithmDesign").unwrap();
/// assert_eq!(prompt.category, "AlgorithmDesign");
/// ```
pub fn get_category_prompt(category: &str) -> Option<&'static CategoryPrompt> {
    CATEGORY_LOOKUP.get(category).copied()
}

/// Returns all available category names.
///
/// # Examples
///
/// ```
/// use dataforge::prompts::categories::all_category_names;
///
/// let names = all_category_names();
/// assert!(names.contains(&"AlgorithmDesign"));
/// assert!(names.contains(&"SecurityAnalysis"));
/// ```
pub fn all_category_names() -> Vec<&'static str> {
    CATEGORY_PROMPTS.iter().map(|p| p.category).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_category_prompts_count() {
        assert_eq!(CATEGORY_PROMPTS.len(), 8, "Should have 8 category prompts");
    }

    #[test]
    fn test_get_category_prompt_found() {
        let prompt = get_category_prompt("AlgorithmDesign");
        assert!(prompt.is_some());
        let prompt = prompt.expect("AlgorithmDesign should exist");
        assert_eq!(prompt.category, "AlgorithmDesign");
        assert!(!prompt.system_prompt.is_empty());
        assert!(!prompt.difficulty_guidelines.is_empty());
        assert!(!prompt.anti_patterns.is_empty());
        assert!(!prompt.required_skills.is_empty());
        assert!(!prompt.example_themes.is_empty());
    }

    #[test]
    fn test_get_category_prompt_not_found() {
        let prompt = get_category_prompt("NonExistentCategory");
        assert!(prompt.is_none());
    }

    #[test]
    fn test_all_categories_have_content() {
        for category_prompt in CATEGORY_PROMPTS {
            assert!(
                !category_prompt.category.is_empty(),
                "Category name should not be empty"
            );
            assert!(
                category_prompt.system_prompt.len() >= 200,
                "System prompt for {} should be at least 200 chars, got {}",
                category_prompt.category,
                category_prompt.system_prompt.len()
            );
            assert!(
                category_prompt.difficulty_guidelines.len() >= 100,
                "Difficulty guidelines for {} should be at least 100 chars",
                category_prompt.category
            );
            assert!(
                category_prompt.anti_patterns.len() >= 4,
                "Anti-patterns for {} should have at least 4 entries",
                category_prompt.category
            );
            assert!(
                category_prompt.required_skills.len() >= 4,
                "Required skills for {} should have at least 4 entries",
                category_prompt.category
            );
            assert!(
                category_prompt.example_themes.len() >= 4,
                "Example themes for {} should have at least 4 entries",
                category_prompt.category
            );
        }
    }

    #[test]
    fn test_all_category_names() {
        let names = all_category_names();
        assert_eq!(names.len(), 8);
        assert!(names.contains(&"AlgorithmDesign"));
        assert!(names.contains(&"SystemDebugging"));
        assert!(names.contains(&"SecurityAnalysis"));
        assert!(names.contains(&"Infrastructure"));
        assert!(names.contains(&"DataEngineering"));
        assert!(names.contains(&"ReverseEngineering"));
        assert!(names.contains(&"PerformanceOptimization"));
        assert!(names.contains(&"IntegrationTasks"));
    }

    #[test]
    fn test_category_lookup_consistency() {
        for category_prompt in CATEGORY_PROMPTS {
            let looked_up = get_category_prompt(category_prompt.category);
            assert!(
                looked_up.is_some(),
                "Category {} should be in lookup",
                category_prompt.category
            );
            assert_eq!(
                looked_up.expect("checked above").category,
                category_prompt.category
            );
        }
    }

    #[test]
    fn test_anti_patterns_are_specific() {
        for category_prompt in CATEGORY_PROMPTS {
            for anti_pattern in category_prompt.anti_patterns {
                assert!(
                    anti_pattern.len() >= 10,
                    "Anti-pattern '{}' in {} should be descriptive",
                    anti_pattern,
                    category_prompt.category
                );
            }
        }
    }

    #[test]
    fn test_example_themes_are_specific() {
        for category_prompt in CATEGORY_PROMPTS {
            for theme in category_prompt.example_themes {
                assert!(
                    theme.len() >= 20,
                    "Example theme '{}' in {} should be descriptive",
                    theme,
                    category_prompt.category
                );
            }
        }
    }
}
