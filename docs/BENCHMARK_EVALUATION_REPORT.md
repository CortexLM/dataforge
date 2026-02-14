# Benchmark Evaluation Report

## Executive Summary

This report documents the evaluation of the swe-forge benchmark task generation system against the terminal-bench format standard. Key improvements were made to align generated tasks with practical, executable terminal benchmarks.

## Initial Findings

### Tasks Generated Before Changes

The initial task generation produced highly theoretical tasks requiring:
- Cloud provider infrastructure (AWS Direct Connect, Azure ExpressRoute)
- Network hardware (Palo Alto firewalls, BGP routers)
- Multi-datacenter database replication
- External services not available in Docker containers

**Example of problematic task:**
> "Debug PostgreSQL replication between on-premise and AWS RDS with eBGP peerings, MTU issues, and firewall configuration"

**Agent evaluation result:** **IMPOSSIBLE** - Task cannot be executed in any standard Docker container.

### terminal-bench Task Format

terminal-bench tasks have these characteristics:
1. **Concrete file paths**: `/app/access_log`, `/home/user/result.txt`
2. **Clear, actionable instructions**: "Analyze the log file and count unique IPs"
3. **Verifiable outputs**: Specific files or values to produce
4. **Self-contained**: All needed files exist in the Docker container

## Changes Made

### 1. Ideator Prompts (`src/agents/ideator.rs`)

Updated system prompt to:
- Require tasks be EXECUTABLE in Docker containers
- Forbid external cloud services, network infrastructure
- Provide concrete examples of good vs bad tasks
- Require specific file paths and verifiable outputs

Added new fields to `TaskIdea`:
- `input_files`: Paths where input data exists
- `output_file`: Where result should be written

### 2. Validator Prompts (`src/agents/task_validator.rs`)

Updated validation criteria to:
- Prioritize practical executability over theoretical complexity
- Approve tasks with concrete file paths
- Reject tasks requiring external services
- Lower complexity threshold from 0.6 to 0.4

## Results After Changes

### Tasks Generated After Changes

**Example 1: Security - Reverse Engineering**
> "A proprietary data archive at /app/datastream.bin employs an undocumented binary protocol. Reverse engineer the format, decode the XOR obfuscation, and write results to /home/user/decoded.txt"

**Example 2: Debugging - Performance Optimization**
> "The Python script at /app/calculate_revenue.py processes /app/data/products.csv (100,000+ rows) slowly. Optimize to run under 5 seconds, output to /app/output/revenue_by_category.json"

### Agent Evaluation

| Aspect | Before | After |
|--------|--------|-------|
| Executable in Docker | NO | YES |
| Has concrete file paths | NO | YES |
| Has verifiable output | PARTIAL | YES |
| Similar to terminal-bench | NO | YES |
| Difficulty rating | IMPOSSIBLE | MEDIUM |

## Recommendations

### For Further Improvement

1. **Generate supporting files**: The task generator should also produce:
   - Input data files (CSVs, logs, scripts)
   - Reference solutions
   - Test verification scripts

2. **Add task categories**: Consider adding dedicated terminal-bench categories:
   - `file-analysis`: Log parsing, data extraction
   - `code-debugging`: Fix broken scripts
   - `performance`: Optimize slow code
   - `security-audit`: Find vulnerabilities

3. **Difficulty calibration**: Run generated tasks through agents to:
   - Measure actual solve rates
   - Adjust difficulty based on agent performance
   - Identify tasks that are too easy/hard

## Test Artifacts

Generated tasks are saved in:
- `/workspace/test-outputs/benchmark-test-run/batch-1/` - Initial (theoretical) tasks
- `/workspace/test-outputs/benchmark-test-run/batch-improved2/` - Improved (practical) tasks

## Conclusion

The changes successfully aligned swe-forge's task generation with the terminal-bench format. Generated tasks are now:
- Practical and executable
- Have concrete file paths and outputs
- Can be verified programmatically
- Suitable for AI coding agent evaluation
