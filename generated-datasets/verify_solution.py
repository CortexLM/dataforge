#!/usr/bin/env python3
"""
Dataforge Solution Verifier
Verifies agent solutions against task.yaml criteria

Usage (local):
    python verify_solution.py <task_dir> <solution_dir>
    
Usage (in Docker container):
    python verify_solution.py /workspace /output
    
Example:
    python verify_solution.py ./fc132552-291c-428d-8430-14ed0db1e1b8 ./my_solution

The verifier checks:
1. Canary token presence (anti-memorization)
2. Automated checks from task.yaml (file_exists, output_contains, etc.)
3. Displays success criteria for manual review
"""

import sys
import os
import re
import subprocess
from pathlib import Path
from typing import Dict, List, Any, Tuple

# Try to import yaml, provide fallback
try:
    import yaml
except ImportError:
    yaml = None
    print("[!] PyYAML not installed. Install with: pip install pyyaml")

try:
    import json
except ImportError:
    json = None


class SolutionVerifier:
    def __init__(self, task_dir: str, solution_dir: str):
        self.task_dir = Path(task_dir)
        self.solution_dir = Path(solution_dir)
        self.task_yaml = self._load_task_yaml()
        self.results: List[Dict[str, Any]] = []
        
    def _load_task_yaml(self) -> Dict:
        task_file = self.task_dir / "task.yaml"
        if not task_file.exists():
            raise FileNotFoundError(f"task.yaml not found in {self.task_dir}")
        with open(task_file, 'r', encoding='utf-8') as f:
            return yaml.safe_load(f)
    
    def verify(self) -> Dict[str, Any]:
        """Run all verification checks and return results."""
        print(f"\n{'='*60}")
        print(f"DATAFORGE SOLUTION VERIFIER")
        print(f"{'='*60}")
        print(f"Task ID: {self.task_yaml.get('id', 'unknown')}")
        print(f"Category: {self.task_yaml.get('metadata', {}).get('category', 'unknown')}")
        print(f"Difficulty: {self.task_yaml.get('difficulty', {}).get('level', 'unknown')}")
        print(f"{'='*60}\n")
        
        # 1. Check canary token
        self._check_canary_token()
        
        # 2. Run automated checks from task.yaml
        self._run_automated_checks()
        
        # 3. Check success criteria (manual review indicators)
        self._check_success_criteria()
        
        # Calculate final score
        return self._calculate_score()
    
    def _check_canary_token(self):
        """Verify canary token is present in solution."""
        canary = self.task_yaml.get('anti_memorization', {}).get('canary_token', '')
        if not canary:
            return
            
        print("[*] Checking canary token presence...")
        found = False
        
        for file in self.solution_dir.rglob('*'):
            if file.is_file():
                try:
                    content = file.read_text(encoding='utf-8', errors='ignore')
                    if canary in content:
                        found = True
                        break
                except:
                    pass
        
        self.results.append({
            'name': 'canary_token',
            'description': 'Canary token present in solution',
            'passed': found,
            'required': True,
            'message': f"Canary '{canary[:30]}...' {'found' if found else 'NOT FOUND'}"
        })
        print(f"    {'[PASS]' if found else '[FAIL]'} Canary token {'found' if found else 'not found'}")
    
    def _run_automated_checks(self):
        """Run automated checks defined in task.yaml."""
        checks = self.task_yaml.get('verification', {}).get('automated_checks', [])
        
        if not checks:
            print("[*] No automated checks defined")
            return
            
        print(f"\n[*] Running {len(checks)} automated checks...")
        
        for i, check in enumerate(checks):
            check_type = check.get('check_type', '')
            target = check.get('target', '')
            expected = check.get('expected', '')
            
            result = self._execute_check(check_type, target, expected)
            self.results.append(result)
            
            status = '[PASS]' if result['passed'] else '[FAIL]'
            print(f"    {status} {check_type}: {target[:50]}...")
    
    def _execute_check(self, check_type: str, target: str, expected: str) -> Dict:
        """Execute a single automated check."""
        result = {
            'name': f'{check_type}_{target[:20]}',
            'description': f'{check_type} check for {target}',
            'passed': False,
            'required': False,
            'message': ''
        }
        
        try:
            if check_type == 'file_exists':
                # Check both absolute path and relative to solution_dir
                if target.startswith('/'):
                    file_path = Path(target)
                else:
                    file_path = self.solution_dir / target
                    
                exists = file_path.exists()
                result['passed'] = (str(exists).lower() == expected.lower())
                result['message'] = f"File {target} exists: {exists}"
                
            elif check_type == 'output_contains':
                # Target might be a command like "cat /path/file" or a file path
                content = None
                
                # First, try to interpret as a shell command
                if any(cmd in target for cmd in ['cat ', 'grep ', 'echo ', '||', '&&', '2>']):
                    try:
                        proc = subprocess.run(
                            target, 
                            shell=True, 
                            capture_output=True, 
                            text=True, 
                            timeout=10,
                            cwd=str(self.solution_dir)
                        )
                        content = proc.stdout + proc.stderr
                    except subprocess.TimeoutExpired:
                        result['message'] = f"Command timed out: {target[:50]}"
                    except Exception as e:
                        result['message'] = f"Command failed: {e}"
                
                # If no content from command, try as file path
                if content is None:
                    if target.startswith('/'):
                        file_path = Path(target)
                    else:
                        file_path = self.solution_dir / target
                        
                    if file_path.exists():
                        content = file_path.read_text(encoding='utf-8', errors='ignore')
                
                if content:
                    result['passed'] = expected.lower() in content.lower()
                    result['message'] = f"'{expected}' found: {result['passed']}"
                else:
                    result['message'] = f"Could not get content from: {target[:50]}"
                    
            elif check_type == 'content_contains':
                if target.startswith('/'):
                    file_path = Path(target)
                else:
                    file_path = self.solution_dir / target
                    
                if file_path.exists():
                    content = file_path.read_text(encoding='utf-8', errors='ignore')
                    result['passed'] = expected in content
                    result['message'] = f"Content check for '{expected}'"
                else:
                    result['message'] = f"File {target} not found"
                    
            elif check_type == 'regex_match':
                if target.startswith('/'):
                    file_path = Path(target)
                else:
                    file_path = self.solution_dir / target
                    
                if file_path.exists():
                    content = file_path.read_text(encoding='utf-8', errors='ignore')
                    result['passed'] = bool(re.search(expected, content))
                    result['message'] = f"Regex '{expected[:30]}' match: {result['passed']}"
                else:
                    result['message'] = f"File {target} not found"
                    
            else:
                result['message'] = f"Unknown check type: {check_type}"
                
        except Exception as e:
            result['message'] = f"Error: {str(e)}"
            
        return result
    
    def _check_success_criteria(self):
        """Display success criteria for manual review."""
        criteria = self.task_yaml.get('verification', {}).get('success_criteria', [])
        
        if not criteria:
            return
            
        print(f"\n[*] Success Criteria (Manual Review Required):")
        print("-" * 50)
        for i, criterion in enumerate(criteria, 1):
            print(f"  {i}. {criterion}")
        print("-" * 50)
        
        # Check partial credit criteria
        partial = self.task_yaml.get('verification', {}).get('partial_credit_criteria', [])
        if partial:
            print(f"\n[*] Partial Credit Options:")
            for item in partial:
                criterion = item.get('criterion', '')
                points = item.get('points', 0)
                print(f"  - {criterion} ({points*100:.0f}%)")
    
    def _calculate_score(self) -> Dict[str, Any]:
        """Calculate final verification score."""
        total = len(self.results)
        passed = sum(1 for r in self.results if r['passed'])
        required_total = sum(1 for r in self.results if r.get('required', False))
        required_passed = sum(1 for r in self.results if r.get('required', False) and r['passed'])
        
        # Base score from automated checks
        auto_score = (passed / total * 100) if total > 0 else 0
        
        # Check if all required checks passed
        all_required_passed = (required_passed == required_total) if required_total > 0 else True
        
        summary = {
            'task_id': self.task_yaml.get('id', 'unknown'),
            'category': self.task_yaml.get('metadata', {}).get('category', 'unknown'),
            'difficulty': self.task_yaml.get('difficulty', {}).get('level', 'unknown'),
            'automated_checks': {
                'total': total,
                'passed': passed,
                'failed': total - passed,
                'score_percentage': round(auto_score, 1)
            },
            'required_checks': {
                'total': required_total,
                'passed': required_passed,
                'all_passed': all_required_passed
            },
            'overall_status': 'PASS' if (auto_score >= 70 and all_required_passed) else 'FAIL',
            'details': self.results
        }
        
        print(f"\n{'='*60}")
        print("VERIFICATION SUMMARY")
        print(f"{'='*60}")
        print(f"Automated Checks: {passed}/{total} passed ({auto_score:.1f}%)")
        print(f"Required Checks:  {required_passed}/{required_total} passed")
        print(f"Overall Status:   {summary['overall_status']}")
        print(f"{'='*60}")
        
        # Note about manual review
        if self.task_yaml.get('verification', {}).get('manual_review_required', True):
            print("\n[!] NOTE: This task requires MANUAL REVIEW of success criteria")
            print("    The automated score is only a partial assessment.")
        
        return summary


def main():
    if yaml is None:
        print("Error: PyYAML is required. Install with: pip install pyyaml")
        sys.exit(1)
        
    if len(sys.argv) < 3:
        print(__doc__)
        print("\nAvailable tasks:")
        tasks_dir = Path(__file__).parent
        for task in tasks_dir.iterdir():
            if task.is_dir() and (task / "task.yaml").exists():
                print(f"  - {task.name}")
        print("\nUsage examples:")
        print("  Local:  python verify_solution.py ./task_id_dir ./my_solution")
        print("  Docker: python verify_solution.py /workspace /output")
        sys.exit(1)
    
    task_dir = sys.argv[1]
    solution_dir = sys.argv[2]
    
    if not Path(task_dir).exists():
        print(f"Error: Task directory not found: {task_dir}")
        sys.exit(1)
        
    if not Path(solution_dir).exists():
        print(f"Warning: Solution directory not found: {solution_dir}")
        print(f"Creating it...")
        Path(solution_dir).mkdir(parents=True, exist_ok=True)
    
    try:
        verifier = SolutionVerifier(task_dir, solution_dir)
        results = verifier.verify()
        
        # Save results
        if json:
            output_file = Path(solution_dir) / "verification_results.json"
            try:
                with open(output_file, 'w', encoding='utf-8') as f:
                    json.dump(results, f, indent=2)
                print(f"\nResults saved to: {output_file}")
            except Exception as e:
                print(f"\nCould not save results: {e}")
        
        # Exit code based on result
        sys.exit(0 if results.get('overall_status') == 'PASS' else 1)
        
    except FileNotFoundError as e:
        print(f"Error: {e}")
        sys.exit(1)
    except Exception as e:
        print(f"Error during verification: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main()
