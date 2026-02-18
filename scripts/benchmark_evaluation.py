#!/usr/bin/env python3
"""
Benchmark Evaluation Script for dataforge

This script:
1. Generates 10 benchmark tasks using the dataforge CLI
2. Evaluates each task with an autonomous agent
3. Measures success rates, coherence, and difficulty calibration
4. Outputs results for iterative improvement
"""

import json
import os
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional


@dataclass
class TaskResult:
    """Result of evaluating a single task."""
    task_id: str
    category: str
    difficulty: str
    success: bool
    duration_seconds: float
    steps_taken: int
    agent_output: str
    notes: str
    error: Optional[str] = None


@dataclass
class EvaluationReport:
    """Complete evaluation report."""
    total_tasks: int
    successful_tasks: int
    failed_tasks: int
    success_rate: float
    average_duration: float
    by_difficulty: dict = field(default_factory=dict)
    task_results: list = field(default_factory=list)
    recommendations: list = field(default_factory=list)


def run_command(cmd: list[str], env: Optional[dict] = None) -> tuple[int, str, str]:
    """Run a command and return (returncode, stdout, stderr)."""
    merged_env = os.environ.copy()
    if env:
        merged_env.update(env)
    
    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        env=merged_env,
        timeout=1800  # 30 minute timeout
    )
    return result.returncode, result.stdout, result.stderr


def generate_tasks(
    count: int,
    model: str,
    api_key: str,
    output_dir: str,
    categories: Optional[list[str]] = None
) -> bool:
    """Generate benchmark tasks using dataforge."""
    
    # Ensure output directory exists
    Path(output_dir).mkdir(parents=True, exist_ok=True)
    
    # Build the command
    cmd = [
        "cargo", "run", "--",
        "generate",
        "-n", str(count),
        "-m", model,
        "-o", output_dir,
        "--json",
        "--api-key", api_key,
    ]
    
    if categories:
        # Generate tasks for specific category
        cmd.extend(["-c", categories[0]])
    
    print(f"Generating {count} tasks...")
    print(f"Command: {' '.join(cmd[:6])}...")
    
    returncode, stdout, stderr = run_command(cmd)
    
    if returncode != 0:
        print(f"Error generating tasks: {stderr}")
        return False
    
    try:
        result = json.loads(stdout)
        if result.get("status") == "success":
            print(f"Generated {len(result.get('tasks', []))} tasks")
            return True
        else:
            print(f"Generation failed: {result}")
            return False
    except json.JSONDecodeError:
        print(f"Could not parse output: {stdout[:500]}")
        return False


def evaluate_tasks(
    tasks_dir: str,
    model: str,
    api_key: str,
    max_steps: int = 50,
    timeout: int = 1200
) -> Optional[dict]:
    """Evaluate tasks using the evaluate command."""
    
    cmd = [
        "cargo", "run", "--",
        "evaluate",
        "--tasks-dir", tasks_dir,
        "-m", model,
        "--api-key", api_key,
        "--max-steps", str(max_steps),
        "--timeout", str(timeout),
        "--json",
    ]
    
    print(f"Evaluating tasks in {tasks_dir}...")
    
    returncode, stdout, stderr = run_command(cmd, env={"RUST_LOG": "warn"})
    
    if returncode != 0:
        print(f"Evaluation error: {stderr}")
        # Try to parse partial results
        
    try:
        return json.loads(stdout)
    except json.JSONDecodeError:
        print(f"Could not parse evaluation output: {stdout[:500]}")
        return None


def analyze_results(evaluation_result: dict) -> EvaluationReport:
    """Analyze evaluation results and generate recommendations."""
    
    task_results = evaluation_result.get("task_results", [])
    
    successful = sum(1 for t in task_results if t.get("success", False))
    failed = len(task_results) - successful
    
    # Analyze by difficulty
    by_difficulty = {}
    for result in task_results:
        diff = result.get("difficulty", "unknown")
        if diff not in by_difficulty:
            by_difficulty[diff] = {"total": 0, "success": 0, "durations": []}
        by_difficulty[diff]["total"] += 1
        if result.get("success", False):
            by_difficulty[diff]["success"] += 1
        by_difficulty[diff]["durations"].append(result.get("duration_seconds", 0))
    
    # Calculate success rates per difficulty
    for diff, stats in by_difficulty.items():
        stats["success_rate"] = stats["success"] / stats["total"] if stats["total"] > 0 else 0
        stats["avg_duration"] = sum(stats["durations"]) / len(stats["durations"]) if stats["durations"] else 0
    
    # Generate recommendations
    recommendations = []
    
    overall_success_rate = successful / len(task_results) if task_results else 0
    
    if overall_success_rate > 0.8:
        recommendations.append("Tasks are too easy - increase difficulty factors")
        recommendations.append("Consider adding more traps and edge cases")
    elif overall_success_rate < 0.2:
        recommendations.append("Tasks may be too hard or unclear - review problem statements")
        recommendations.append("Consider simplifying instructions or adding more context")
    else:
        recommendations.append("Difficulty calibration looks reasonable")
    
    # Check difficulty distribution
    for diff, stats in by_difficulty.items():
        if diff.lower() == "easy" and stats["success_rate"] < 0.7:
            recommendations.append(f"Easy tasks are too hard (success rate: {stats['success_rate']:.0%})")
        elif diff.lower() == "hard" and stats["success_rate"] > 0.5:
            recommendations.append(f"Hard tasks are too easy (success rate: {stats['success_rate']:.0%})")
    
    durations = [r.get("duration_seconds", 0) for r in task_results]
    avg_duration = sum(durations) / len(durations) if durations else 0
    
    return EvaluationReport(
        total_tasks=len(task_results),
        successful_tasks=successful,
        failed_tasks=failed,
        success_rate=overall_success_rate,
        average_duration=avg_duration,
        by_difficulty=by_difficulty,
        task_results=task_results,
        recommendations=recommendations,
    )


def print_report(report: EvaluationReport):
    """Print a formatted evaluation report."""
    
    print("\n" + "=" * 60)
    print("BENCHMARK EVALUATION REPORT")
    print("=" * 60)
    
    print(f"\n📊 Overall Results:")
    print(f"  Total Tasks: {report.total_tasks}")
    print(f"  Successful: {report.successful_tasks}")
    print(f"  Failed: {report.failed_tasks}")
    print(f"  Success Rate: {report.success_rate:.1%}")
    print(f"  Average Duration: {report.average_duration:.1f}s")
    
    print(f"\n📈 Results by Difficulty:")
    for diff, stats in report.by_difficulty.items():
        print(f"  {diff}:")
        print(f"    Tasks: {stats['total']}")
        print(f"    Success Rate: {stats['success_rate']:.1%}")
        print(f"    Avg Duration: {stats['avg_duration']:.1f}s")
    
    print(f"\n🔧 Recommendations:")
    for rec in report.recommendations:
        print(f"  • {rec}")
    
    print(f"\n📋 Task Details:")
    for result in report.task_results:
        status = "✅" if result.get("success", False) else "❌"
        print(f"  {status} {result.get('task_id', 'unknown')}")
        print(f"     Category: {result.get('category', 'unknown')}")
        print(f"     Difficulty: {result.get('difficulty', 'unknown')}")
        print(f"     Duration: {result.get('duration_seconds', 0):.1f}s")
        if not result.get("success", False):
            print(f"     Notes: {result.get('notes', 'N/A')[:100]}")
    
    print("\n" + "=" * 60)


def main():
    """Main evaluation workflow."""
    
    # Configuration - API key must be provided via environment variable
    API_KEY = os.environ.get("OPENROUTER_API_KEY")
    if not API_KEY:
        print("ERROR: OPENROUTER_API_KEY environment variable is required")
        print("Please set it before running: export OPENROUTER_API_KEY=your-key-here")
        sys.exit(1)
    MODEL = "openai/gpt-5.2-codex"
    OUTPUT_DIR = "./test-outputs/benchmark-eval"
    TASK_COUNT = 10
    
    print("=" * 60)
    print("DATAFORGE BENCHMARK EVALUATION")
    print("=" * 60)
    print(f"Model: {MODEL}")
    print(f"Tasks to generate: {TASK_COUNT}")
    print(f"Output directory: {OUTPUT_DIR}")
    
    # Phase 1: Generate tasks
    print("\n📝 Phase 1: Task Generation")
    print("-" * 40)
    
    categories = ["debugging", "file-operations", "containers", "networking", "system-administration"]
    tasks_generated = 0
    
    for i, category in enumerate(categories[:2]):  # Generate from 2 categories
        count = TASK_COUNT // 2
        cat_output = f"{OUTPUT_DIR}/batch-{i+1}"
        
        print(f"\nGenerating {count} {category} tasks...")
        if generate_tasks(
            count=count,
            model=MODEL,
            api_key=API_KEY,
            output_dir=cat_output,
            categories=[category]
        ):
            tasks_generated += count
        else:
            print(f"Warning: Failed to generate {category} tasks")
    
    if tasks_generated == 0:
        print("Error: No tasks were generated")
        sys.exit(1)
    
    print(f"\nTotal tasks generated: {tasks_generated}")
    
    # Phase 2: Evaluate tasks
    print("\n🔍 Phase 2: Agent Evaluation")
    print("-" * 40)
    
    all_results = []
    
    for i in range(2):
        batch_dir = f"{OUTPUT_DIR}/batch-{i+1}"
        if not Path(batch_dir).exists():
            continue
            
        result = evaluate_tasks(
            tasks_dir=batch_dir,
            model=MODEL,
            api_key=API_KEY,
            max_steps=15,
            timeout=180
        )
        
        if result and "task_results" in result:
            all_results.extend(result["task_results"])
    
    if not all_results:
        print("Warning: No evaluation results collected")
        print("This may be expected if tasks couldn't be generated")
        return
    
    # Phase 3: Analyze and report
    print("\n📊 Phase 3: Analysis")
    print("-" * 40)
    
    combined_result = {"task_results": all_results}
    report = analyze_results(combined_result)
    print_report(report)
    
    # Save report
    report_path = f"{OUTPUT_DIR}/evaluation_report.json"
    with open(report_path, "w") as f:
        json.dump({
            "total_tasks": report.total_tasks,
            "successful_tasks": report.successful_tasks,
            "failed_tasks": report.failed_tasks,
            "success_rate": report.success_rate,
            "average_duration": report.average_duration,
            "by_difficulty": report.by_difficulty,
            "task_results": report.task_results,
            "recommendations": report.recommendations,
        }, f, indent=2)
    
    print(f"\n📁 Report saved to: {report_path}")
    
    # Phase 4: Determine if iteration needed
    print("\n🔄 Phase 4: Iteration Decision")
    print("-" * 40)
    
    if report.success_rate > 0.8:
        print("⚠️  Tasks are too easy - difficulty increase recommended")
        print("Suggested actions:")
        print("  1. Increase trap complexity in DifficultyAmplifierAgent")
        print("  2. Add more edge cases to task generation")
        print("  3. Reduce hints in problem statements")
    elif report.success_rate < 0.2:
        print("⚠️  Tasks may be too hard or unclear")
        print("Suggested actions:")
        print("  1. Review problem statements for clarity")
        print("  2. Ensure verification criteria are achievable")
        print("  3. Consider adding more context to instructions")
    else:
        print("✅ Difficulty calibration is within acceptable range")
        print(f"   Success rate: {report.success_rate:.1%}")


if __name__ == "__main__":
    main()
